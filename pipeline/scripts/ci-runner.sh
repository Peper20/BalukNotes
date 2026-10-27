#!/usr/bin/env bash
# The CI runner of the project's Forgejo: runs the full check of a pull request
# on every push to it (pipeline/forgejo/check.yml, pipeline/docs/README.md,
# "Forgejo"). A Forgejo Runner on this machine as a user systemd service, no
# root. The server is `forge.url` and the runner's name `ci.runner` of
# .claude/pipeline.json (pipeline/config.md).
#
#   pipeline/scripts/ci-runner.sh install     # download, register, write the unit, start
#   pipeline/scripts/ci-runner.sh uninstall   # stop, remove the unit, the registration and the files
#   pipeline/scripts/ci-runner.sh status      # the unit and the runner as Forgejo sees it
#
# install: puts the pinned `forgejo-runner` (version and sha256 below; a
# mismatch stops it) into ~/.local/bin, registers a repository runner
# `<ci.runner>` with the label `<ci.runner>:host` (the job runs on this machine,
# no Docker; the registration is made through the API as `owner`, with `tea`),
# capacity 1, and writes
#   ~/.config/forgejo-runner/config.yml              the settings and the token (0600)
#   ~/.config/systemd/user/forgejo-runner.service    the unit (enabled, started)
#   ~/.local/share/forgejo-runner/                   the working dirs of the jobs
# The jobs find the main checkout in $PIPELINE_MAIN (the runner's environment,
# set here: the checkout this script belongs to, also when it is started from a
# worktree). Idempotent: a second run changes nothing (a file is rewritten and
# the service restarted only when its text differs).
# uninstall: also removes the runner in Forgejo, the config, the data and the
# binary.
# status: exit 0 when the unit is active and the runner is online in Forgejo.
#
# The log of the runner is in the journal (`journalctl --user -u
# forgejo-runner`); the log of a job is in Forgejo, on the run of the pull
# request. The service runs while the user is logged in (no linger).
#
# Needs: curl, sha256sum, systemctl (--user), tea (the login `owner`), python3.
set -uo pipefail

# The project this script belongs to (not the current directory) gives the
# settings.
here=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
PIPELINE_PROJECT_ROOT=${PIPELINE_PROJECT_ROOT:-$here}
source "$(dirname "${BASH_SOURCE[0]}")/lib/config.sh"
unset here

# Pinned release and its sha256 (the files published next to the binaries).
RUNNER_VERSION=13.2.0
RUNNER_URL=https://code.forgejo.org/forgejo/runner/releases/download/v$RUNNER_VERSION
case "$(uname -m)" in
  x86_64 | amd64)
    RUNNER_ARCH=amd64
    RUNNER_SHA=fadaec897f5e6641c363f87ecaf75f866f99c81b7097ec6b433e4c48123fcad1
    ;;
  aarch64 | arm64)
    RUNNER_ARCH=arm64
    RUNNER_SHA=9d3d462ee83e4e629f959faeacceb20eed2f25290bddea402e65ace46a31afd9
    ;;
  *)
    RUNNER_ARCH=
    RUNNER_SHA=
    ;;
esac

RUNNER_NAME=$(config_get ci.runner)
RUNNER_LABEL=$RUNNER_NAME:host
FORGEJO_URL=$(config_get forge.url)
PROJECT=$(config_get project)
SERVICE=forgejo-runner.service
# Seconds to wait for the runner to show online after the start.
ONLINE_WAIT=${CI_RUNNER_ONLINE_WAIT:-60}

# Places (the environment overrides are for the tests of this script).
CONF_DIR=${CI_RUNNER_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/forgejo-runner}
DATA_DIR=${CI_RUNNER_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/forgejo-runner}
UNIT_DIR=${CI_RUNNER_UNIT_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user}
BIN_DIR=${CI_RUNNER_BIN_DIR:-$HOME/.local/bin}
CONFIG=$CONF_DIR/config.yml
UNIT=$UNIT_DIR/$SERVICE
BIN=$BIN_DIR/forgejo-runner

die() { echo "ci-runner: $*" >&2; exit 1; }
say() { echo "ci-runner: $*"; }

usage() {
  awk 'NR > 1 && !/^#/ {exit} NR > 1 {sub(/^# ?/, ""); print}' "$0"
}

# config_text <main checkout> <url> <uuid> <token>: config.yml of the runner.
# Capacity 1: jobs queue, so the checks of pull requests never overlap. The
# timeout is the runner's cap on a job (the workflow's own is lower); on a stop
# a running job is cancelled after 30 s. No cache server: the workflow uses no
# cache action.
config_text() {
  cat <<EOF
# Written by pipeline/scripts/ci-runner.sh install; do not edit (it is rewritten).
# Holds the token of the runner.
log:
  level: info
  job_level: info

runner:
  capacity: 1
  timeout: 4h
  shutdown_timeout: 30s
  envs:
    PIPELINE_MAIN: "$1"
  labels: ["$RUNNER_LABEL"]

host:
  workdir_parent: "$DATA_DIR/work"

cache:
  enabled: false

server:
  connections:
    forgejo:
      url: "$2"
      uuid: "$3"
      token: "$4"
EOF
}

# unit_text <binary> <config> <PATH of the jobs>: the user unit. Nice and ionice
# as check.sh sets for itself, so the runner and the checks it starts leave the
# computer responsive.
unit_text() {
  cat <<EOF
# Written by pipeline/scripts/ci-runner.sh install; do not edit (it is rewritten).
[Unit]
Description=Forgejo Runner of ${PROJECT:-the project} (full check of pull requests, host execution)
StartLimitIntervalSec=0

[Service]
Type=simple
ExecStart=$1 daemon --config $2
WorkingDirectory=$DATA_DIR
Environment=PATH=$3
Nice=2
IOSchedulingClass=best-effort
IOSchedulingPriority=5
Restart=on-failure
RestartSec=10
TimeoutStopSec=60

[Install]
WantedBy=default.target
EOF
}

# jobs_path: the PATH of the jobs: our own bin dirs, then the system ones.
jobs_path() {
  echo "$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:/usr/bin"
}

# conn_field <uuid|token>: the value in the config file (nothing without it).
conn_field() {
  [[ -f $CONFIG ]] || return 0
  awk -v key="$1" '
    $1 == key ":" {
      v = $2
      gsub(/"/, "", v)
      print v
      exit
    }' "$CONFIG"
}

# put_file <path> <mode>: the text on stdin becomes the file. Returns 0 when the
# file changed (new, or another text or mode), 1 when it was already so.
put_file() {
  local tmp
  mkdir -p "$(dirname "$1")" || die "cannot create $(dirname "$1")"
  tmp=$(mktemp "$1.XXXXXX") || die "cannot write in $(dirname "$1")"
  cat >"$tmp"
  chmod "$2" "$tmp"
  if [[ -f $1 ]] && cmp -s "$tmp" "$1" && [[ $(stat -c %a "$1") == "$2" ]]; then
    rm -f "$tmp"
    return 1
  fi
  mv -f "$tmp" "$1"
}

sc() { systemctl --user "$@"; }
api() { tea api --login owner "$@"; }

# parse_runners: the runners of the repository from the API answer on stdin, the
# ones named $RUNNER_NAME: "<id> <uuid> <status> <version> <labels>" per line.
parse_runners() {
  python3 -I -c '
import json, sys
name = sys.argv[1]
for r in json.load(sys.stdin):
    if r["name"] == name:
        print(r["id"], r["uuid"], r["status"], r.get("version") or "-", ",".join(r.get("labels") or []) or "-")
' "$RUNNER_NAME"
}

# parse_registration: "<id> <uuid> <token>" from the answer of a registration on stdin.
parse_registration() {
  python3 -I -c '
import json, sys
d = json.load(sys.stdin)
print(d["id"], d["uuid"], d["token"])
'
}

runner_list() {
  local answer
  answer=$(api '/repos/{owner}/{repo}/actions/runners') || return 1
  parse_runners <<<"$answer"
}

main_root() {
  local here common
  if [[ -n ${PIPELINE_MAIN:-} ]]; then
    echo "$PIPELINE_MAIN"
    return
  fi
  here=$PIPELINE_PROJECT_ROOT
  common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || return 1
  dirname "$common"
}

# ensure_binary: returns 0 when it installed the pinned runner, 1 when it was there.
ensure_binary() {
  local have tmp
  have=$("$BIN" --version 2>/dev/null | awk '{print $NF}')
  if [[ $have == "v$RUNNER_VERSION" ]]; then
    say "forgejo-runner $RUNNER_VERSION is in $BIN_DIR"
    return 1
  fi
  [[ -n $RUNNER_SHA ]] || die "unsupported CPU $(uname -m) (amd64 and arm64 only)"
  say "forgejo-runner $RUNNER_VERSION -> $BIN_DIR"
  mkdir -p "$BIN_DIR" || die "cannot create $BIN_DIR"
  tmp=$(mktemp "$BIN.XXXXXX") || die "cannot write in $BIN_DIR"
  if ! curl -fsSL --retry 3 --retry-delay 2 -o "$tmp" "$RUNNER_URL/forgejo-runner-$RUNNER_VERSION-linux-$RUNNER_ARCH"; then
    rm -f "$tmp"
    die "download of the runner failed ($RUNNER_URL)"
  fi
  if ! echo "$RUNNER_SHA  $tmp" | sha256sum -c --status; then
    rm -f "$tmp"
    die "sha256 of the downloaded runner does not match the pinned one"
  fi
  chmod 0755 "$tmp"
  mv -f "$tmp" "$BIN"
}

# warn_tools: the tools the jobs call (the kit's, and `ci.tools` of the config,
# space-separated); a missing one is only a warning.
warn_tools() {
  local t missing=() project_tools
  read -ra project_tools <<<"$(config_get ci.tools)"
  for t in git tea python3 flock "${project_tools[@]}"; do
    PATH=$(jobs_path) command -v "$t" >/dev/null 2>&1 || missing+=("$t")
  done
  if (( ${#missing[@]} > 0 )); then
    echo "ci-runner: WARNING: not found for the jobs: ${missing[*]} (PATH of the unit: $(jobs_path))" >&2
  fi
}

cmd_install() {
  require_settings
  local c
  for c in curl sha256sum python3 tea systemctl; do
    command -v "$c" >/dev/null || die "$c is missing"
  done
  sc show-environment >/dev/null 2>&1 || die "no user systemd session (systemctl --user does not answer)"

  local main
  main=$(main_root) || die "git: not in a checkout of the project"
  [[ -x $main/pipeline/scripts/check-pr.sh ]] || die "$main/pipeline/scripts/check-pr.sh is missing: not the main checkout of the project?"

  local changed=0
  ensure_binary && changed=1
  warn_tools

  # The registration: kept while the config names a runner that Forgejo has.
  local uuid token list id answer
  uuid=$(conn_field uuid)
  token=$(conn_field token)
  list=$(runner_list) || die "the server: cannot list the runners (tea api --login owner; is Forgejo at $FORGEJO_URL up?)"
  if [[ -n $uuid && -n $token ]] && grep -q " $uuid " <<<"$list"; then
    say "the runner $RUNNER_NAME is registered"
  else
    # A runner of our name that the config does not know cannot be used (its
    # token is lost): remove it, so that there is one.
    while read -r id _; do
      [[ -n $id ]] || continue
      say "removing the runner $RUNNER_NAME (id $id) that the config does not know"
      api -X DELETE "/repos/{owner}/{repo}/actions/runners/$id" >/dev/null || die "cannot remove the runner $id"
    done <<<"$list"
    say "registering the runner $RUNNER_NAME ($RUNNER_LABEL)"
    answer=$(api -X POST -d "{\"name\":\"$RUNNER_NAME\",\"description\":\"${PROJECT:-the project} CI, host execution (pipeline/scripts/ci-runner.sh)\"}" \
      '/repos/{owner}/{repo}/actions/runners') || die "the server did not register the runner"
    read -r id uuid token < <(parse_registration <<<"$answer" 2>/dev/null) || die "the answer of the server is not a registration"
    [[ -n $uuid && -n $token ]] || die "the answer of the server has no uuid and token"
  fi

  mkdir -p "$DATA_DIR/work" "$UNIT_DIR" || die "cannot create the directories"
  config_text "$main" "$FORGEJO_URL" "$uuid" "$token" | put_file "$CONFIG" 600 && {
    say "wrote $CONFIG"
    changed=1
  }
  unit_text "$BIN" "$CONFIG" "$(jobs_path)" | put_file "$UNIT" 644 && {
    say "wrote $UNIT"
    changed=1
    sc daemon-reload || die "systemctl --user daemon-reload failed"
  }

  local was_active=0
  sc is-active --quiet "$SERVICE" && was_active=1
  sc enable "$SERVICE" >/dev/null 2>&1 || die "cannot enable $SERVICE"
  if (( was_active )); then
    if (( changed )); then
      say "restarting $SERVICE"
      sc restart "$SERVICE" || die "cannot restart $SERVICE"
    fi
  else
    say "starting $SERVICE"
    sc start "$SERVICE" || die "cannot start $SERVICE"
  fi

  # Online in Forgejo?
  local i status=
  for ((i = 0; i < ONLINE_WAIT; i++)); do
    status=$(runner_list 2>/dev/null | awk '{print $3}')
    [[ $status == idle || $status == active ]] && break
    sleep 1
  done
  [[ $status == idle || $status == active ]] ||
    die "the runner is not online in Forgejo after ${ONLINE_WAIT}s (journalctl --user -u forgejo-runner)"
  say "the runner $RUNNER_NAME is online ($status)"
}

cmd_uninstall() {
  require_settings
  command -v systemctl >/dev/null || die "systemctl is missing"
  local uuid list id
  uuid=$(conn_field uuid)

  if [[ -e $UNIT ]] || sc cat "$SERVICE" >/dev/null 2>&1; then
    say "stopping and disabling $SERVICE"
    sc disable --now "$SERVICE" >/dev/null 2>&1
  fi
  if [[ -e $UNIT ]]; then
    rm -f "$UNIT"
    sc daemon-reload
    say "removed $UNIT"
  fi

  # The registration: every runner of our name.
  if command -v tea >/dev/null && list=$(runner_list 2>/dev/null); then
    while read -r id _; do
      [[ -n $id ]] || continue
      if api -X DELETE "/repos/{owner}/{repo}/actions/runners/$id" >/dev/null; then
        say "removed the runner $RUNNER_NAME (id $id) in Forgejo"
      else
        echo "ci-runner: WARNING: cannot remove the runner $id in Forgejo (the repository settings, Actions, Runners)" >&2
      fi
    done <<<"$list"
  elif [[ -n $uuid ]]; then
    echo "ci-runner: WARNING: Forgejo does not answer; the runner $RUNNER_NAME stays registered (the repository settings, Actions, Runners)" >&2
  fi

  if [[ -d $CONF_DIR ]]; then rm -rf "$CONF_DIR" && say "removed $CONF_DIR"; fi
  if [[ -d $DATA_DIR ]]; then rm -rf "$DATA_DIR" && say "removed $DATA_DIR"; fi
  if [[ -e $BIN ]]; then rm -f "$BIN" && say "removed $BIN"; fi
  return 0
}

cmd_status() {
  require_settings
  command -v systemctl >/dev/null || die "systemctl is missing"
  local active enabled ok=0 list
  active=$(sc is-active "$SERVICE" 2>/dev/null)
  enabled=$(sc is-enabled "$SERVICE" 2>/dev/null)
  echo "unit:    $SERVICE ${active:-unknown}, ${enabled:-not installed} ($UNIT)"
  echo "binary:  $("$BIN" --version 2>/dev/null || echo "not installed") ($BIN)"
  echo "config:  $([[ -f $CONFIG ]] && echo "$CONFIG" || echo "none")"
  [[ $active == active ]] && ok=1
  if list=$(runner_list 2>/dev/null); then
    if [[ -z $list ]]; then
      echo "runner:  none registered in Forgejo"
      ok=0
    else
      local id uuid status version labels
      while read -r id uuid status version labels; do
        echo "runner:  $RUNNER_NAME id $id $status, $version, labels $labels"
        [[ $status == idle || $status == active ]] || ok=0
      done <<<"$list"
    fi
  else
    echo "runner:  Forgejo does not answer (tea api --login owner)"
    ok=0
  fi
  (( ok ))
}

# require_settings: the keys of the config that every command needs.
require_settings() {
  [[ -n $RUNNER_NAME ]] || die "\"ci.runner\" (the name of the runner) is not set in $PROJECT_ROOT/.claude/pipeline.json"
  [[ -n $FORGEJO_URL ]] || die "\"forge.url\" is not set in $PROJECT_ROOT/.claude/pipeline.json"
}

main() {
  case "${1:-}" in
    -h | --help)
      usage
      exit 0
      ;;
    install | uninstall | status)
      if [[ $# -ne 1 ]]; then
        echo "usage: pipeline/scripts/ci-runner.sh install|uninstall|status" >&2
        exit 2
      fi
      ;;
    *)
      echo "usage: pipeline/scripts/ci-runner.sh install|uninstall|status" >&2
      exit 2
      ;;
  esac
  case "$1" in
    install) cmd_install ;;
    uninstall) cmd_uninstall ;;
    status) cmd_status ;;
  esac
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  main "$@"
fi
