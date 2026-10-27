# Sourced by tools/install.sh, test-env.sh, cargo.sh and web-bundle.sh: the CPU
# cap and the lock of heavy work are the kit's (pipeline/scripts/lib/cap.sh);
# the settings of this project's tools are tools/lib/cpu-env.sh (`check.env` of
# .claude/pipeline.json). See `.claude/rules/tools.md`, "CPU cap".
source "$(dirname "${BASH_SOURCE[0]}")/../../pipeline/scripts/lib/cap.sh"
