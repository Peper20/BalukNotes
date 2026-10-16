#!/usr/bin/env bash
# Smoke launch of the app window (`notes-app`): it opens, loads the fixture
# vault through its own core and quits cleanly - all inside a headless
# gamescope, so no window appears on the user's display.
#
#   tools/app-smoke.sh [--bin <notes-app>] [--seconds <n>] [-v]
#
# --bin      the app to run (default: the installed ~/.local/bin/notes-app;
#            `notes-typst` of the same version must be next to it - the window
#            starts that core). A fresh build: target/<profile>/notes-app,
#            when `notes-typst` is built into the same directory.
# --seconds  how long the window stays open before SIGTERM (default 5).
# -v         print the app's log at the end.
#
# The run is isolated from the user's session: its own XDG_RUNTIME_DIR (the
# single-instance socket and the core socket - a window the user has open is
# neither handed anything nor stopped), XDG_DATA_HOME, XDG_CONFIG_HOME and
# XDG_CACHE_HOME (window state, WebKit data), a copy of tests/vault and its
# own --data directory. All of it is deleted at the end.
#
# gamescope exits with 0 whatever its child does, so the inner script writes
# the result to a file, which this one checks: the app quit with code 0 on
# SIGTERM, created its window, started its core and the client's script ran and
# loaded the notes of the vault from the core (the proxy logs the requests of the page). Without
# gamescope the script fails (exit 2): it never opens a visible window.
set -euo pipefail

if [[ -n ${APP_SMOKE_DIR:-} ]]; then
  # The inner part: inside gamescope. Everything it needs is in the
  # environment; the result goes to $APP_SMOKE_DIR/status.
  tmp=$APP_SMOKE_DIR
  unset WAYLAND_DISPLAY   # only the X server of gamescope, never the user's session
  export GDK_BACKEND=x11
  # gamescope itself keeps the real XDG_RUNTIME_DIR (its sockets); only the app is isolated.
  export XDG_RUNTIME_DIR=$tmp/run XDG_DATA_HOME=$tmp/data XDG_CONFIG_HOME=$tmp/config XDG_CACHE_HOME=$tmp/cache
  export RUST_LOG=notes_app=debug
  rc=
  trap '[[ -n ${app_pid:-} ]] && kill -KILL "$app_pid" 2>/dev/null || true; echo "${rc:-failed}" >"$tmp/status"' EXIT
  "$APP_SMOKE_BIN" --data "$tmp/notes-data" --vault "$tmp/vault" >"$tmp/app.log" 2>&1 &
  app_pid=$!
  sleep "$APP_SMOKE_SECONDS"
  kill -TERM "$app_pid" 2>/dev/null || true
  # The app quits by itself on SIGTERM; one that does not in 20 s is a failure.
  for _ in $(seq 200); do
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$app_pid" 2>/dev/null; then
    rc=hung
  else
    rc=0
    wait "$app_pid" || rc=$?
  fi
  app_pid=
  exit 0
fi

cd "$(dirname "$0")/.."
bin=${NOTES_APP:-$HOME/.local/bin/notes-app}
seconds=5
verbose=0
while (( $# )); do
  case $1 in
    --bin) bin=$2; shift 2 ;;
    --seconds) seconds=$2; shift 2 ;;
    -v) verbose=1; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
if ! command -v gamescope >/dev/null; then
  echo "error: gamescope is not installed; the window is run only inside a headless gamescope (never on the user's display)" >&2
  exit 2
fi
[[ -x $bin ]] || { echo "error: no notes-app at $bin (--bin)" >&2; exit 2; }
[[ -x $(dirname "$bin")/notes-typst ]] || { echo "error: no notes-typst next to $bin" >&2; exit 2; }

# Under /tmp, not $TMPDIR: unix socket paths (the app and its core) are limited to 107 bytes.
tmp=$(mktemp -d /tmp/app-smoke.XXXXXX)
cleanup() {
  # What is left of a killed run: the app's core is a child of the app; stop what runs from this directory.
  if [[ -S $tmp/run/baluk-notes/notes.sock ]]; then
    fuser -k -TERM "$tmp/run/baluk-notes/notes.sock" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT
mkdir -p "$tmp/run" "$tmp/data" "$tmp/config" "$tmp/cache" "$tmp/notes-data"
chmod 700 "$tmp/run"
cp -r tests/vault "$tmp/vault"

APP_SMOKE_DIR=$tmp APP_SMOKE_BIN=$bin APP_SMOKE_SECONDS=$seconds \
  timeout 120 nice -n 2 gamescope --backend headless -W 1300 -H 900 -- "$PWD/tools/app-smoke.sh" \
  >"$tmp/gamescope.log" 2>&1 || true

fail() {
  echo "app smoke FAILED: $1" >&2
  [[ -f $tmp/app.log ]] && sed 's/^/  app: /' "$tmp/app.log" | tail -n 30 >&2
  [[ -f $tmp/status ]] || sed 's/^/  gamescope: /' "$tmp/gamescope.log" | tail -n 20 >&2
  exit 1
}
[[ -f $tmp/status ]] || fail "no result: gamescope did not run the script"
status=$(<"$tmp/status")
[[ $status == 0 ]] || fail "the app did not quit cleanly (exit status: $status)"
log=$tmp/app.log
grep -q "core started" "$log" || fail "the app did not start its core"
grep -q "window main created" "$log" || fail "the window was not created"
# The client has booted when its script ran and asked the core for the vault's notes.
grep -q "GET notes://localhost/api/vaults/vault/notes 200" "$log" || fail "the client did not load the vault's notes from the core"
grep -Eq "notes://[^ ]+ 502 " "$log" && fail "a request of the page got 502 from the core"
(( verbose )) && cat "$log"
echo "app smoke ok: window created, page loaded, quit with code 0 (${seconds} s)"
