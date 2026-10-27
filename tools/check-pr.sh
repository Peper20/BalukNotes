#!/usr/bin/env bash
# The kit's pipeline/scripts/check-pr.sh: the CI runner installed before the kit
# still calls this path (docs/tech-debt.md, "Pipeline").
exec "$(dirname "$0")/../pipeline/scripts/check-pr.sh" "$@"
