#!/usr/bin/env bash
# End of a cloud session: send `main` of the throwaway Forgejo
# (tools/cloud-setup.sh) to GitHub as a branch, for the user to review and
# open a pull request from. It never writes to `main` of GitHub and never
# forces a push.
#
#   tools/cloud-finish.sh [--branch NAME]
#
# The branch: --branch, else $CLOUD_BRANCH, else the `claude/...` branch the
# session started on (recorded by cloud-setup.sh), else cloud/<date>. A push
# the server rejects (the branch has commits we do not have) is reported, not
# forced: pick another --branch or merge the GitHub branch first.
#
# Settings: FORGEJO_PORT and FORGEJO_HOME as in cloud-setup.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

port=${FORGEJO_PORT:-3000}
fhome=${FORGEJO_HOME:-$HOME/.cache/baluk-notes/forgejo}
repo_url=http://localhost:$port/ibaluk/baluk-notes.git

die() { echo "cloud-finish: $*" >&2; exit 1; }

branch=${CLOUD_BRANCH:-}
while [[ $# -gt 0 ]]; do
  case $1 in
    --branch) [[ $# -ge 2 ]] || die "--branch needs a name"; branch=$2; shift 2 ;;
    *) die "unknown argument '$1' (usage: tools/cloud-finish.sh [--branch NAME])" ;;
  esac
done
[[ -n $branch || ! -f $fhome/session-branch ]] || branch=$(cat "$fhome/session-branch")
[[ -n $branch ]] || branch=cloud/$(date +%F)

branch=${branch#refs/heads/}
case $branch in
  main | master | HEAD) die "refusing to write to '$branch' of GitHub: the result goes to a branch" ;;
esac
git check-ref-format --branch "$branch" >/dev/null 2>&1 || die "'$branch' is not a valid branch name"

[[ $(git remote get-url origin 2>/dev/null || true) == "$repo_url" ]] \
  || die "origin is not the cloud Forgejo ($repo_url): was tools/cloud-setup.sh run here?"
git remote get-url github >/dev/null 2>&1 || die "no remote 'github' to send to"

echo "▶ fetching main of the cloud Forgejo"
git fetch origin main >/dev/null 2>&1 || die "cannot fetch from $repo_url (is the server running? tools/cloud-setup.sh)"
new=$(git rev-parse origin/main)

if git fetch github main >/dev/null 2>&1; then
  if [[ $(git rev-parse github/main) == "$new" ]]; then
    echo "nothing to send: main of the cloud Forgejo equals main of GitHub"
    exit 0
  fi
  echo "  $(git rev-list --count github/main..origin/main) commit(s) ahead of GitHub main"
fi

# merged work only: pull requests still open on the server are not in main
if command -v tea >/dev/null 2>&1; then
  open=$(tea pulls --login dev --output simple --fields index 2>/dev/null | wc -l || true)
  [[ ${open:-0} -eq 0 ]] || echo "  warning: $open pull request(s) still open on the cloud Forgejo; they are not sent"
fi

echo "▶ pushing to github: $branch"
if ! out=$(git push github "refs/remotes/origin/main:refs/heads/$branch" 2>&1); then
  echo "$out" >&2
  echo "cloud-finish: the push of '$branch' was rejected; nothing was forced." >&2
  echo "  If the branch has commits we do not have: fetch it (git fetch github $branch)," >&2
  echo "  merge it into main of the cloud Forgejo through a pull request, and run this again," >&2
  echo "  or send the result to another branch: tools/cloud-finish.sh --branch <name>." >&2
  exit 1
fi
echo "$out"

# a compare link when the github remote looks like owner/repo
gh=$(git remote get-url github)
if [[ $gh == *github.com* || $gh == */git/* ]] && [[ $gh =~ ([^/:]+)/([^/]+)$ ]]; then
  repo=${BASH_REMATCH[2]%.git}
  echo
  echo "Open a pull request: https://github.com/${BASH_REMATCH[1]}/$repo/pull/new/$branch"
fi
echo "Branch on GitHub: $branch (GitHub main is untouched)"
