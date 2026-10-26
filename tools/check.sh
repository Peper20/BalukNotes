#!/usr/bin/env bash
# The full check of this project: the kit's runner (pipeline/scripts/check.sh)
# with the steps of tools/steps.sh (`check.steps` of .claude/pipeline.json).
# Before a pull request run the targeted tests (pipeline/scripts/test.sh); the
# full check runs on the head of every pull request (tools/check-pr.sh).
#
#   tools/check.sh           # everything: Rust, the fixture vault, API types, client, e2e
#   tools/check.sh --fast    # without the client build and e2e (a minute or two)
#   tools/check.sh --rust    # Rust only
#   tools/check.sh --app     # client only
#   tools/check.sh -h        # the steps
cd "$(dirname "$0")/.." || exit 2
exec pipeline/scripts/check.sh "$@"
