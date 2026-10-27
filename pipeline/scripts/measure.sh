#!/usr/bin/env bash
# A speed measurement that another agent can repeat (pipeline/docs/README.md,
# "Measuring"): a command N times, each under the lock of heavy work, with the
# CPU cap of the checks and at nice 2.
#
#   pipeline/scripts/measure.sh <runs> -- <command...>
#
# Per run: the wall seconds, the average and the peak number of busy CPUs of the
# whole machine (sampled from /proc/stat every MEASURE_INTERVAL seconds, 0.5 by
# default; none for a run shorter than one interval) and the exit code. Then the
# minimum, median and maximum of the wall seconds. The lock keeps other runs of
# the project off the machine during a run, but not what the project does not
# route through it: measure on a quiet machine. Exit 1 if a run failed.
set -uo pipefail

usage() {
  echo "usage: measure.sh <runs> -- <command...>" >&2
  exit 2
}
[[ ${1:-} == -h || ${1:-} == --help ]] && { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0; }
[[ ${1:-} =~ ^[1-9][0-9]?$ ]] || usage
runs=$1
[[ ${2:-} == -- && $# -ge 3 ]] || usage
shift 2

renice -n 2 -p $$ >/dev/null 2>&1 || true
ionice -c 2 -n 5 -p $$ >/dev/null 2>&1 || true

kit=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
source "$kit/lib/config.sh"
cd "$PROJECT_ROOT" || exit 2
source "$kit/lib/cap.sh"

interval=${MEASURE_INTERVAL:-0.5}
ncpu=$(nproc)

# cpu_jiffies: "<busy> <total>" of all CPUs now.
cpu_jiffies() {
  local user nice system idle iowait irq softirq steal
  read -r _ user nice system idle iowait irq softirq steal _ </proc/stat
  echo $((user + nice + system + irq + softirq + steal)) $((user + nice + system + idle + iowait + irq + softirq + steal))
}

# sample_cpu <file>: until killed, append the busy CPUs (hundredths) of each interval.
sample_cpu() {
  local busy0 total0 busy1 total1
  read -r busy0 total0 < <(cpu_jiffies)
  while sleep "$interval"; do
    read -r busy1 total1 < <(cpu_jiffies)
    (( total1 > total0 )) && echo $(( (busy1 - busy0) * ncpu * 100 / (total1 - total0) )) >>"$1"
    busy0=$busy1 total0=$total1
  done
}

# hundredths <n>: n / 100 with two decimals.
hundredths() { printf '%d.%02d' $(($1 / 100)) $(($1 % 100)); }
# micros <n>: microseconds as seconds with two decimals.
micros() { hundredths $(($1 / 10000)); }

echo "measure: $* ($runs run(s), CHECK_JOBS=$CHECK_JOBS of $ncpu CPUs, nice $(ps -o ni= -p $$ | tr -d ' '))"
samples=$(mktemp)
trap 'rm -f "$samples"' EXIT
walls=() failed=0
for ((run = 1; run <= runs; run++)); do
  heavy_acquire
  : >"$samples"
  sample_cpu "$samples" {HEAVY_FD}>&- &
  sampler=$!
  start=${EPOCHREALTIME//[.,]/}
  "$@" >/dev/null 2>&1 {HEAVY_FD}>&-
  rc=$?
  wall=$((${EPOCHREALTIME//[.,]/} - start))
  kill "$sampler" 2>/dev/null
  wait "$sampler" 2>/dev/null
  heavy_release
  (( rc == 0 )) || failed=1
  walls+=("$wall")
  if [[ -s $samples ]]; then
    read -r avg peak < <(awk '{ s += $1; if ($1 > m) m = $1 } END { printf "%d %d\n", s / NR, m }' "$samples")
    busy="avg $(hundredths "$avg") peak $(hundredths "$peak") busy CPUs"
  else
    busy="busy CPUs: no sample"
  fi
  echo "run $run: $(micros "$wall") s, $busy, exit $rc"
done

mapfile -t sorted < <(printf '%s\n' "${walls[@]}" | sort -n)
n=${#sorted[@]}
if (( n % 2 )); then
  median=${sorted[n / 2]}
else
  median=$(((sorted[n / 2 - 1] + sorted[n / 2]) / 2))
fi
echo "wall: min $(micros "${sorted[0]}") s, median $(micros "$median") s, max $(micros "${sorted[n - 1]}") s"
exit "$failed"
