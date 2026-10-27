#!/usr/bin/env bash
# Start a throwaway Forgejo inside a cloud session and make the clone work
# against it exactly as the local checkout works against the user's server
# (pipeline/docs/README.md, "Forgejo"; the `forge` skill): the same accounts,
# the same `tea` logins, the same protection of `main`. Run it as the first step
# of a cloud session; at the end `pipeline/scripts/cloud-finish.sh` sends the
# result to GitHub. The repository is `forge.repo` of .claude/pipeline.json
# (pipeline/config.md); its owner is the administrator account.
#
#   pipeline/scripts/cloud-setup.sh            # install, initialise, push main, summary
#   pipeline/scripts/cloud-setup.sh --install  # only download and verify the binaries
#   pipeline/scripts/cloud-setup.sh --stop     # stop the server (the data stays)
#
# Idempotent: a second run reuses what exists (and starts the server again if
# it is down). It never touches the user's own Forgejo: it listens on its own
# port and keeps its own data, and it refuses to run when the port is taken by
# something else, when the data dir is not its own, when `origin` already is
# the repository of a Forgejo, or when the `tea` config has the logins `owner`,
# `dev` or `reviewer` for another server.
#
# Settings (environment):
#   FORGEJO_PORT     port on 127.0.0.1 (default 3000: nothing runs there in a
#                    cloud session; a local test uses another)
#   FORGEJO_HOME     data dir (default ~/.cache/<project name as a word>/forgejo)
#   FORGEJO_BIN_DIR  where `tea` goes if it is not installed (default
#                    /usr/local/bin for root, else ~/.local/bin)
#   FORGEJO_BIN      a Forgejo binary to use instead of the pinned download
set -euo pipefail
# The project this script belongs to gives the settings, whatever the current
# directory is.
kit=$(cd "$(dirname "$0")" && pwd -P)
cd "$kit/../.."
source "$kit/lib/config.sh"

# Pinned releases and their sha256 (the files published next to them).
FORGEJO_VERSION=16.0.6
TEA_VERSION=0.16.0
FORGEJO_URL=https://codeberg.org/forgejo/forgejo/releases/download/v$FORGEJO_VERSION
TEA_URL=https://dl.gitea.com/tea/$TEA_VERSION
case "$(uname -m)" in
  x86_64 | amd64)
    arch=amd64
    FORGEJO_SHA=b90e22c2e5534fd7705d93d81ecea792b00f15cad0ee7bbc3061c65bf9524293
    TEA_SHA=9c142c8a0b31485eea2387031c1c404f12b7141075eb4ffbd7ced49441590636
    ;;
  aarch64 | arm64)
    arch=arm64
    FORGEJO_SHA=c55ec791a3924097971c65dfd960f02a63efabc3ebcb4bb9a3363c2b5c693206
    TEA_SHA=0e14a036322eeb1fc55e266acafeb8490e523ed99f9232034ae74861e2339879
    ;;
  *) echo "cloud-setup: unsupported CPU $(uname -m) (amd64 and arm64 only)" >&2; exit 1 ;;
esac

project=$(config_get project)
port=${FORGEJO_PORT:-3000}
fhome=${FORGEJO_HOME:-$(project_cache_dir)/forgejo}
if [[ $EUID -eq 0 ]]; then bindir=${FORGEJO_BIN_DIR:-/usr/local/bin}; else bindir=${FORGEJO_BIN_DIR:-$HOME/.local/bin}; fi
host=localhost
url=http://$host:$port
forge_repo=$(config_get forge.repo)
repo_owner=${forge_repo%%/*}
repo_name=${forge_repo#*/}
repo_url=$url/$repo_owner/$repo_name.git
marker=$fhome/.pipeline-cloud-forgejo
fbin=${FORGEJO_BIN:-$fhome/bin/forgejo}
# user -> tea login (as the `forge` skill expects): the repository's owner is
# the administrator
users=("$repo_owner" agent-dev agent-reviewer)
logins=(owner dev reviewer)

die() { echo "cloud-setup: $*" >&2; exit 1; }
say() { echo "▶ $*"; }
# curl to our own server; never through a proxy of the environment
api_curl() { curl -sS --noproxy '*' "$@"; }
# the server answers HTTP (an anonymous API call is refused, that is fine)
server_up() { [[ $(api_curl -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/api/healthz" 2>/dev/null || true) =~ ^[1-5] ]]; }
port_open() { (exec 3<>"/dev/tcp/$1/$port") 2>/dev/null; }
server_pid() { [[ -f $fhome/forgejo.pid ]] && cat "$fhome/forgejo.pid" 2>/dev/null || true; }
# the pid file names our server only if that process runs our config
is_ours() {
  local pid; pid=$(server_pid)
  [[ -n $pid && -r /proc/$pid/cmdline ]] && tr '\0' ' ' < "/proc/$pid/cmdline" | grep -qF -- "$fhome/app.ini"
}

if ! [[ $port =~ ^[0-9]+$ ]]; then die "FORGEJO_PORT must be a number, got '$port'"; fi

if [[ ${1:-} == --stop ]]; then
  if is_ours; then
    kill "$(server_pid)"
    for _ in $(seq 1 150); do is_ours || break; sleep 0.2; done
    if is_ours; then echo "the Forgejo of $fhome is still shutting down (pid $(server_pid))"; exit 1; fi
    echo "stopped the Forgejo of $fhome"
  else
    echo "no Forgejo of $fhome is running"
  fi
  exit 0
fi

# --- preflight: refuse before anything is changed ---------------------------

if [[ -e $fhome && ! -e $marker ]] && [[ -n $(ls -A "$fhome" 2>/dev/null) ]]; then
  die "$fhome is not a data dir of this script (no $(basename "$marker")); refusing to use it. Another FORGEJO_HOME?"
fi

if ! is_ours && { port_open 127.0.0.1 || port_open ::1; }; then
  die "port $port is taken by another program (the user's own Forgejo?); set FORGEJO_PORT to a free port"
fi

mode=full
[[ ${1:-} == --install ]] && mode=install

if [[ $mode == full ]]; then
  [[ $forge_repo =~ ^[^/]+/[^/]+$ ]] || die "\"forge.repo\" (owner/name) is not set in $PROJECT_ROOT/.claude/pipeline.json (or neither jq nor python3 can read it)"
  git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not inside a git checkout"
  origin_url=$(git remote get-url origin 2>/dev/null || true)
  if [[ $origin_url == "$repo_url" ]]; then
    # A rerun, but only when our own state says we arranged the remotes: on the
    # user's machine origin is exactly this URL at the default port.
    if [[ ! -e $marker ]] || [[ $(cat "$fhome/remotes-done" 2>/dev/null || true) != "$repo_url" ]]; then
      die "origin is already $repo_url, but the data dir $fhome did not set it up: it is the user's own Forgejo; refusing to touch it"
    fi
  else
    [[ -n $origin_url ]] || die "the checkout has no remote 'origin' (the GitHub clone is expected)"
    forge_url=$(config_get forge.url)
    forge_origin=${origin_url%/}
    if [[ -n $forge_url && ${forge_origin%.git} == "${forge_url%/}/$forge_repo" ]]; then
      die "origin ($origin_url) is already the repository of a Forgejo, not a GitHub clone; refusing to touch it"
    fi
    if git remote get-url github >/dev/null 2>&1; then
      die "remote 'github' exists and origin ($origin_url) is not this script's Forgejo; refusing to rearrange the remotes"
    fi
  fi
fi

# --- (a) binaries -----------------------------------------------------------

download() { # url sha256 dest
  local tmp; tmp=$(mktemp "$3.XXXXXX")
  curl -fsSL --retry 3 --retry-delay 2 -o "$tmp" "$1" || { rm -f "$tmp"; die "download of $1 failed (is the host allowed by the network policy?)"; }
  if ! echo "$2  $tmp" | sha256sum -c --status; then
    rm -f "$tmp"; die "sha256 of $1 does not match the pinned one"
  fi
  chmod 0755 "$tmp"
  mv -f "$tmp" "$3"
}

tea_bin=
if [[ -x $bindir/tea ]]; then tea_bin=$bindir/tea
elif command -v tea >/dev/null 2>&1; then tea_bin=$(command -v tea)
else
  say "tea $TEA_VERSION -> $bindir"
  mkdir -p "$bindir"
  download "$TEA_URL/tea-$TEA_VERSION-linux-$arch" "$TEA_SHA" "$bindir/tea"
  tea_bin=$bindir/tea
fi

if [[ $mode == full ]]; then
  # A `tea` config that has one of our login names is the user's, unless this
  # script made that login (listed in its data dir) for this very URL: never
  # overwrite or reuse it.
  existing=$("$tea_bin" logins list --output tsv 2>/dev/null || true)
  for l in "${logins[@]}"; do
    line=$(awk -F'\t' -v n="$l" '$1 == n' <<<"$existing")
    [[ -n $line ]] || continue
    if [[ $(cut -f2 <<<"$line") != "$url" ]] || ! grep -qxF "$l" "$fhome/tea-logins" 2>/dev/null; then
      die "the tea config has the login '$l' ($line) that this script did not make for $url; refusing to use or overwrite it. Use a scratch HOME or another port."
    fi
  done
fi

mkdir -p "$fhome"
[[ -e $marker ]] || echo "data of the throwaway Forgejo of pipeline/scripts/cloud-setup.sh" > "$marker"
chmod 0700 "$fhome"

forgejo_ok() { [[ -x $fbin ]] && "$fbin" --version 2>/dev/null | grep -qF "version $FORGEJO_VERSION"; }
if [[ -z ${FORGEJO_BIN:-} ]] && ! forgejo_ok; then
  say "forgejo $FORGEJO_VERSION -> $fhome/bin"
  mkdir -p "$fhome/bin"
  download "$FORGEJO_URL/forgejo-$FORGEJO_VERSION-linux-$arch" "$FORGEJO_SHA" "$fbin"
fi
[[ -x $fbin ]] || die "no Forgejo binary at $fbin"

if [[ $mode == install ]]; then
  echo "installed: $fbin, $tea_bin"
  exit 0
fi

# --- (b) configuration and server -------------------------------------------

ini=$fhome/app.ini
fj() { "$fbin" "$@" --config "$ini" --work-path "$fhome/work"; }
# Forgejo refuses to run as root; the cloud session is root and throwaway.
export I_AM_BEING_UNSAFE_RUNNING_AS_ROOT=true

if [[ ! -f $ini ]]; then
  say "configuration"
  mkdir -p "$fhome/work" "$fhome/data/repositories" "$fhome/data/home" "$fhome/log"
  (
    umask 077
    cat > "$ini" <<EOF
APP_NAME = ${project:-the project} (cloud session)
RUN_MODE = prod
WORK_PATH = $fhome/work

[server]
HTTP_ADDR = 127.0.0.1
HTTP_PORT = $port
DOMAIN = $host
ROOT_URL = $url/
DISABLE_SSH = true
START_SSH_SERVER = false
LFS_START_SERVER = false
OFFLINE_MODE = true

[database]
DB_TYPE = sqlite3
PATH = $fhome/data/forgejo.db

[repository]
ROOT = $fhome/data/repositories
DEFAULT_BRANCH = main

[git]
; Forgejo writes a global git config: keep it out of the real home
HOME_PATH = $fhome/data/home

[security]
INSTALL_LOCK = true
SECRET_KEY = $("$fbin" generate secret SECRET_KEY)
INTERNAL_TOKEN = $("$fbin" generate secret INTERNAL_TOKEN)

[oauth2]
JWT_SECRET = $("$fbin" generate secret JWT_SECRET)

[service]
DISABLE_REGISTRATION = true
REQUIRE_SIGNIN_VIEW = true
ENABLE_NOTIFY_MAIL = false
REGISTER_EMAIL_CONFIRM = false

[mailer]
ENABLED = false

[actions]
ENABLED = false

[picture]
DISABLE_GRAVATAR = true

[log]
MODE = console
LEVEL = Warn
EOF
  )
fi

start_server() {
  say "starting Forgejo on 127.0.0.1:$port"
  (
    cd "$fhome"
    FORGEJO_WORK_DIR=$fhome/work setsid nohup "$fbin" web --config "$ini" --work-path "$fhome/work" \
      --pid "$fhome/forgejo.pid.own" >>"$fhome/server.log" 2>&1 &
    echo $! > "$fhome/forgejo.pid"
  )
  local i
  for i in $(seq 1 150); do
    server_up && return 0
    is_ours || { tail -n 20 "$fhome/server.log" >&2; die "Forgejo exited at start (log: $fhome/server.log)"; }
    sleep 0.4
  done
  die "Forgejo does not answer on port $port (log: $fhome/server.log)"
}
if is_ours && server_up; then
  say "Forgejo already running (pid $(server_pid))"
else
  start_server
fi

# --- (c) users, passwords, tea logins ---------------------------------------

creds=$fhome/credentials
[[ -f $creds ]] || { (umask 077; : > "$creds"); }
chmod 0600 "$creds"
pw_of() { grep -m1 "^$1=" "$creds" | cut -d= -f2- || true; }

existing_users=$(fj admin user list | awk 'NR > 1 {print $2}')
for u in "${users[@]}"; do
  pw=$(pw_of "$u")
  if grep -qxF "$u" <<<"$existing_users"; then
    [[ -n $pw ]] || die "user $u exists but $creds has no password for it; remove $fhome to start over"
    continue
  fi
  [[ -n $pw ]] || { pw=$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n'); echo "$u=$pw" >> "$creds"; }
  say "user $u"
  flags=(--must-change-password=false)
  [[ $u == "$repo_owner" ]] && flags+=(--admin)
  fj admin user create --username "$u" --email "$u@localhost.localdomain" --password "$pw" "${flags[@]}" >/dev/null
done
owner_pw=$(pw_of "$repo_owner")

# tea logins: a token per user, only when the login is missing
for i in "${!users[@]}"; do
  l=${logins[$i]}; u=${users[$i]}
  # (the preflight made sure an existing login of this name is ours)
  if awk -F'\t' -v n="$l" '$1 == n {f = 1} END {exit !f}' <<<"$("$tea_bin" logins list --output tsv 2>/dev/null || true)"; then
    continue
  fi
  say "tea login $l ($u)"
  token=$(fj admin user generate-access-token --username "$u" --token-name "tea-$(date +%s)" --raw)
  "$tea_bin" logins add --name "$l" --url "$url" --token "$token" --no-version-check >/dev/null
  echo "$l" >> "$fhome/tea-logins"
done

# --- (d) repository and remotes ---------------------------------------------

auth=$(mktemp "$fhome/curl-auth.XXXXXX")
trap 'rm -f "$auth"' EXIT
printf 'user = "%s:%s"\nheader = "Content-Type: application/json"\n' "$repo_owner" "$owner_pw" > "$auth"
# api METHOD PATH [JSON]: prints the body, fails on an HTTP error
api() {
  local m=$1 p=$2; shift 2
  local data=(); [[ $# -gt 0 ]] && data=(--data "$1")
  api_curl -f -K "$auth" -X "$m" "${data[@]}" "http://127.0.0.1:$port/api/v1$p"
}
api_status() { # METHOD PATH: only the status code
  api_curl -o /dev/null -w '%{http_code}' -K "$auth" -X "$1" "http://127.0.0.1:$port/api/v1$2"
}

if [[ $(api_status GET "/repos/$repo_owner/$repo_name") != 200 ]]; then
  say "repository $repo_owner/$repo_name"
  api POST /user/repos "{\"name\":\"$repo_name\",\"private\":true,\"default_branch\":\"main\",\"auto_init\":false}" >/dev/null
fi
# merge style and collaborators as on the local server: squash only
api PATCH "/repos/$repo_owner/$repo_name" '{"has_wiki":false,"allow_merge_commits":false,"allow_rebase":false,"allow_rebase_explicit":false,"allow_squash_merge":true,"allow_fast_forward_only_merge":false,"allow_rebase_update":true,"default_merge_style":"squash","default_delete_branch_after_merge":false}' >/dev/null
for u in agent-dev agent-reviewer; do
  api PUT "/repos/$repo_owner/$repo_name/collaborators/$u" '{"permission":"write"}' >/dev/null
done

# remotes: origin = the cloud Forgejo, github = what the clone had as origin
if [[ $origin_url != "$repo_url" ]]; then
  say "remotes: origin -> $repo_url, github -> $origin_url"
  git remote rename origin github
  git remote add origin "$repo_url"
fi
# the proof, for the next run, that this data dir arranged the remotes
echo "$repo_url" > "$fhome/remotes-done"
# git authenticates as the owner (a push goes as the owner, like locally) through
# a helper that only reads the password file: `store` would also remember the
# agent accounts given in a URL and then use them for origin. A proxy of the
# environment must not catch the loopback address.
printf -v helper '!f() { test "$1" = get || exit 0; echo username=%s; echo "password=$(sed -n "s/^%s=//p" "%s")"; }; f' \
  "$repo_owner" "$repo_owner" "$creds"
git config --local --unset-all "credential.$url.helper" 2>/dev/null || true
git config --local --add "credential.$url.helper" ""
git config --local --add "credential.$url.helper" "$helper"
git config --local "http.$url/.proxy" ""
git config --local checkout.defaultRemote origin

# the branch the session started on, for cloud-finish.sh
if [[ ! -f $fhome/session-branch ]]; then
  cur=$(git symbolic-ref --short -q HEAD || true)
  [[ $cur == claude/* ]] && echo "$cur" > "$fhome/session-branch"
fi

if [[ -z $(git ls-remote origin refs/heads/main) ]]; then
  src=
  for r in refs/remotes/github/main refs/heads/main; do
    git rev-parse -q --verify "$r" >/dev/null && { src=$r; break; }
  done
  [[ -n $src ]] || die "no main branch to push (neither github/main nor main)"
  if [[ $(git rev-parse --is-shallow-repository) == true ]]; then
    say "the clone is shallow: fetching the full history from github"
    git fetch --unshallow github
  fi
  say "pushing $src to origin main"
  git push origin "$src:refs/heads/main"
fi
git fetch origin >/dev/null 2>&1
if git rev-parse -q --verify refs/heads/main >/dev/null; then
  git branch --set-upstream-to=origin/main main >/dev/null
else
  git branch --track main origin/main >/dev/null
fi

# --- (e) protection of main (the local server's settings) --------------------

say "protection of main"
protection='{"branch_name":"main","rule_name":"main","enable_push":true,"enable_push_whitelist":true,"push_whitelist_usernames":["@OWNER@"],"enable_merge_whitelist":true,"merge_whitelist_usernames":["agent-dev","@OWNER@"],"enable_status_check":false,"required_approvals":1,"enable_approvals_whitelist":true,"approvals_whitelist_username":["agent-reviewer"],"block_on_rejected_reviews":true,"block_on_official_review_requests":false,"block_on_outdated_branch":false,"dismiss_stale_approvals":true,"ignore_stale_approvals":false,"require_signed_commits":false,"apply_to_admins":false}'
protection=${protection//@OWNER@/$repo_owner}
if [[ $(api_status GET "/repos/$repo_owner/$repo_name/branch_protections/main") == 200 ]]; then
  api PATCH "/repos/$repo_owner/$repo_name/branch_protections/main" "$protection" >/dev/null
else
  api POST "/repos/$repo_owner/$repo_name/branch_protections" "$protection" >/dev/null
fi

# --- (f) summary ------------------------------------------------------------

case ":$PATH:" in *":$(dirname "$tea_bin"):"*) ;; *) echo "note: $(dirname "$tea_bin") is not in PATH" ;; esac
echo
echo "Throwaway Forgejo is ready (nothing is mirrored to GitHub)"
echo "  server    $url  (127.0.0.1 only, pid $(server_pid), log $fhome/server.log)"
echo "  repo      $repo_owner/$repo_name   origin = $repo_url   github = $(git remote get-url github)"
echo "  accounts  $repo_owner=owner (admin, pushes), agent-dev=dev (PRs, merge), agent-reviewer=reviewer (reviews)"
echo "  main      merge needs an approval of agent-reviewer; a new commit makes it stale; only $repo_owner pushes"
echo "  secrets   $creds (0600)"
echo "  session   branch for GitHub: $(cat "$fhome/session-branch" 2>/dev/null || echo "none (cloud-finish.sh uses cloud/<date>)")"
echo "  finish    pipeline/scripts/cloud-finish.sh"
