# Sourced by tools/check.sh (the step cargo-test): `cargo test` with the test
# binaries running at the same time (`.claude/rules/tools.md`, "CPU cap").
# `cargo test` runs the binaries one after another, and each is held up by
# its longest tests while the other CPUs idle. Here cargo only builds them
# (`--no-run`), and the binaries run as cargo would run them (the directory of
# the package, the variables of the package) within the thread budget.
#
# Needs python3 (it reads the build's JSON, as target-limit.sh reads
# `cargo metadata`; no jq).

# cargo_test_binaries <build.json>: one tab-separated line per test binary of the
# build: executable, package directory, package name, package version, label
# (the package and the target). In the order of the labels.
cargo_test_binaries() {
  python3 -I - "$1" <<'PY'
import json, os, sys

rows = []
for line in open(sys.argv[1], encoding="utf-8"):
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    if msg.get("reason") != "compiler-artifact" or not msg.get("executable"):
        continue
    if not msg.get("profile", {}).get("test"):
        continue
    directory = os.path.dirname(msg["manifest_path"])
    pid = msg["package_id"]
    if "#" in pid:  # path+file:///dir/name#0.1.0 or ...#name@0.1.0
        head, _, frag = pid.partition("#")
        name, _, version = frag.rpartition("@")
        if not name:
            name = os.path.basename(head)
    else:  # name 0.1.0 (path+file:///...)
        name, version = pid.split()[:2]
    target = msg["target"]
    kind = target["kind"][0]
    label = f"{name} lib" if kind == "lib" else f"{name} {kind} {target['name']}"
    rows.append((label, msg["executable"], directory, name, version))
for label, exe, directory, name, version in sorted(rows):
    print(exe, directory, name, version, label, sep="\t")
PY
}

# cargo_test_threads <rank> <tests> <budget>: the test threads of a binary: the
# first in the order of starting (rank 0, the longest) gets half of the budget,
# the others a quarter (both rounded up); at least 1, not more than it has tests
# (a binary of 0 tests runs with 1) and not more than the budget.
cargo_test_threads() {
  local rank=$1 n=$2 budget=$3 t
  if (( rank == 0 )); then t=$(( (budget + 1) / 2 )); else t=$(( (budget + 3) / 4 )); fi
  (( t > n )) && t=$n
  (( t > budget )) && t=$budget
  (( t < 1 )) && t=1
  echo "$t"
}

# cargo_test_times_file: the seconds of each binary in the last run, one
# "<label><tab><seconds>" line each, which decide who starts first. In the cache
# folder of the lock of heavy work, shared by all the checkouts (a label names
# a package and a target).
cargo_test_times_file() {
  echo "${CARGO_TEST_TIMES:-${XDG_CACHE_HOME:-$HOME/.cache}/baluk-notes/cargo-test-times}"
}

# cargo_test <work dir> <cargo test arguments...>: builds every test binary,
# runs them at the same time so that the test threads running at once never
# exceed RUST_TEST_THREADS, and passes if every binary and the doc tests pass.
# The output of each is in <work dir>/<n>.log; the output of this function is
# their logs one after another (by the labels, the doc tests last) under a line
# naming the binary, then a table (tests, threads, seconds) and, if any failed,
# the failed binaries with their failed tests at the very end.
#
# The scheme: the binaries start in order of their seconds in the last run
# (`cargo_test_times_file`), the longest first; those with no seconds yet go
# first, the one with the most tests first among them. The longest gets half
# of the budget as test threads (`cargo_test_threads`), the others a quarter;
# a binary starts as soon as that many threads are free, and a smaller
# one fills a gap where a larger one does not fit. A binary of 0 tests is run as
# well, as cargo does. The doc tests (`cargo test --doc`) are one more job of a
# thread.
cargo_test() {
  local work=$1 budget=${RUST_TEST_THREADS:-$(nproc)}
  shift
  rm -rf "$work"
  mkdir -p "$work"

  # --no-run builds the binaries only; rendered diagnostics go to stderr.
  if ! cargo test "$@" --no-run --message-format=json-render-diagnostics >"$work/build.json" 2>"$work/build.err"; then
    cat "$work/build.err"
    return 1
  fi
  cat "$work/build.err"
  local rows
  rows=$(cargo_test_binaries "$work/build.json") || { echo "cargo_test: could not read the list of test binaries"; return 1; }

  # The jobs, by index. The tests of a binary are counted by itself, in the
  # directory and with the variables it runs with.
  local -a exes=() dirs=() pkgs=() vers=() labels=() counts=() threads=() started=() seconds=() status=()
  local exe dir pkg ver label n i
  while IFS=$'\t' read -r exe dir pkg ver label; do
    [[ -n $exe ]] || continue
    n=$(cargo_test_run "$exe" "$dir" "$pkg" "$ver" 1 --list 2>/dev/null | grep -c ': test$')
    exes+=("$exe") dirs+=("$dir") pkgs+=("$pkg") vers+=("$ver") labels+=("$label") counts+=("$n")
  done <<<"$rows"
  if (( ${#exes[@]} == 0 )); then
    echo "cargo_test: no test binaries in the build"
    return 1
  fi
  # The doc tests: the last job, `--doc` instead of a binary.
  exes+=("") dirs+=("") pkgs+=("") vers+=("") labels+=("doc tests") counts+=(1)
  local jobs=${#exes[@]}

  # The order of starting.
  local -A last=()
  local times_file key secs
  times_file=$(cargo_test_times_file)
  if [[ -r $times_file ]]; then
    while IFS=$'\t' read -r key secs; do
      [[ $secs =~ ^[0-9]+$ ]] && last[$key]=$secs
    done <"$times_file"
  fi
  local order
  order=$(for ((i = 0; i < jobs; i++)); do
    key=${labels[$i]}
    if [[ -n ${last[$key]:-} ]]; then echo "1 ${last[$key]} ${counts[$i]} $i"; else echo "0 0 ${counts[$i]} $i"; fi
  done | sort -k1,1n -k2,2nr -k3,3nr -k4,4n | cut -d' ' -f4)
  local rank=0
  for ((i = 0; i < jobs; i++)); do threads+=(1) started+=("") seconds+=("") status+=(""); done
  for i in $order; do
    [[ -n ${exes[$i]} ]] || continue
    threads[i]=$(cargo_test_threads "$rank" "${counts[$i]}" "$budget")
    rank=$((rank + 1))
  done

  # Start what fits, wait for a binary to end, repeat.
  local -a queue=($order) pids=() pid_job=()
  local free=$budget j p q k now
  while (( ${#queue[@]} > 0 || ${#pids[@]} > 0 )); do
    for k in "${!queue[@]}"; do
      j=${queue[$k]}
      if (( threads[j] <= free )); then
        free=$((free - threads[j]))
        started[j]=$SECONDS
        if [[ -z ${exes[$j]} ]]; then
          ( RUST_TEST_THREADS=1 cargo test "$@" --doc >"$work/$j.log" 2>&1; echo $? >"$work/$j.rc" ) &
        else
          ( cargo_test_run "${exes[$j]}" "${dirs[$j]}" "${pkgs[$j]}" "${vers[$j]}" "${threads[$j]}" >"$work/$j.log" 2>&1
            echo $? >"$work/$j.rc" ) &
        fi
        pids+=($!) pid_job+=("$j")
        unset 'queue[k]'
      fi
    done
    queue=("${queue[@]}")
    (( ${#pids[@]} > 0 )) || break
    wait -n
    local -a left=() left_job=()
    now=$SECONDS
    for q in "${!pids[@]}"; do
      p=${pids[$q]} j=${pid_job[$q]}
      if kill -0 "$p" 2>/dev/null; then
        left+=("$p") left_job+=("$j")
      else
        wait "$p" 2>/dev/null
        seconds[j]=$((now - started[j]))
        free=$((free + threads[j]))
      fi
    done
    pids=("${left[@]}") pid_job=("${left_job[@]}")
  done

  # The logs one after another, the doc tests last.
  local failed=() rc tests_failed
  for ((i = 0; i < jobs; i++)); do
    rc=$(cat "$work/$i.rc" 2>/dev/null || echo 1)
    status[i]=$rc
    echo "===== ${labels[$i]} (${exes[$i]:+$(basename "${exes[$i]}"), }${counts[$i]} test$([[ ${counts[$i]} == 1 ]] || echo s), ${threads[$i]} threads, ${seconds[$i]} s) ====="
    cat "$work/$i.log"
    echo
  done
  echo "===== summary: ${budget} test threads ====="
  printf '%-40s %6s %7s %6s  %s\n' binary tests threads "s" result
  for ((i = 0; i < jobs; i++)); do
    printf '%-40s %6s %7s %6s  %s\n' "${labels[$i]}" "${counts[$i]}" "${threads[$i]}" "${seconds[$i]}" \
      "$([[ ${status[$i]} == 0 ]] && echo ok || echo FAILED)"
    [[ ${status[$i]} == 0 ]] || failed+=("$i")
  done

  # The seconds of the binaries that passed, for the order of the next run
  # (those of binaries not in this build stay); a file that cannot be written is
  # no failure.
  for ((i = 0; i < jobs; i++)); do
    [[ ${status[$i]} == 0 ]] && last[${labels[$i]}]=${seconds[$i]}
  done
  {
    mkdir -p "$(dirname "$times_file")" &&
      for key in "${!last[@]}"; do printf '%s\t%s\n' "$key" "${last[$key]}"; done | sort >"$times_file.$$" &&
      mv "$times_file.$$" "$times_file"
  } 2>/dev/null || true

  if (( ${#failed[@]} > 0 )); then
    echo
    echo "FAILED test binaries:"
    for i in "${failed[@]}"; do
      echo "  ${labels[$i]} (exit ${status[$i]})"
      tests_failed=$(grep -E '^test .* \.\.\. FAILED$' "$work/$i.log" | sed 's/^test \(.*\) \.\.\. FAILED$/\1/')
      [[ -z $tests_failed ]] || sed 's/^/    /' <<<"$tests_failed"
    done
    return 1
  fi
}

# cargo_test_run <exe> <dir> <package> <version> <threads> [arguments...]: a test
# binary as cargo runs it: in the directory of its package with the variables of
# the package.
cargo_test_run() {
  local exe=$1 dir=$2 pkg=$3 ver=$4 threads=$5
  shift 5
  (
    cd "$dir" || exit 1
    export CARGO_MANIFEST_DIR=$dir CARGO_PKG_NAME=$pkg CARGO_PKG_VERSION=$ver CARGO_PRIMARY_PACKAGE=1
    export RUST_TEST_THREADS=$threads
    exec "$exe" "$@"
  )
}
