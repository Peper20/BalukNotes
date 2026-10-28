# What a check writes to disk

The task (roadmap, "What makes the shared `target/` reach its 30 GB limit"):
cut the bytes one full check writes and the size of the build dir, by
measurement. Those writes stall every small write on the encrypted ext4
(tech debt "A settings save waits for the disk").

**Conclusion:** `split-debuginfo = "unpacked"` in `[profile.dev]` of
`Cargo.toml`. The cargo part of a check writes 41% less (4.9 -> 2.9 GB) and
is faster; `target/debug` after a clean build is 26% smaller. Adopted.

## Where the bytes go

The shared `target/` was 29 GB: `debug/` 24 GB (`deps/` 14 GB,
`incremental/` 8.8 GB). A test binary that links Typst is ~430 MB, ~320 MB of
it DWARF (`readelf -S`: `.debug_info` 148 MB, `.debug_str` 89 MB,
`.debug_line` and `.debug_ranges` 39 MB each), and a clean build of the tests
has 10 executables of 260-435 MB (30 over 20 MB in all). Every check relinks
them, so it writes their debug info again. Fewer test binaries would save
little: no crate has many, the large ones are the unit tests and the one
`tests/it/` binary of each crate with Typst and the `notes-typst` binary.

## Method

- The laptop (i5-13420H, 12 CPUs, encrypted ext4 on NVMe, the device
  `dm-0`), Rust stable, `CHECK_JOBS=10`.
- A scratch `CARGO_TARGET_DIR` in the worktree, removed after each variant;
  each run under `pipeline/scripts/measure.sh` (the lock of heavy work, the
  CPU cap of the checks).
- Per variant: a clean build `cargo test --workspace --no-run` (then the
  size of `target/debug`), then the clippy step of the check; then two runs of
  the cargo part of the check: the tracked sources of our crates touched (as
  `freshen_sources` does for a new checkout), the step `cargo-test`
  (`tools/lib/cargo-test.sh`: build and run the tests) and the step `clippy`.
- Bytes written, two ways: the process tree - `getrusage(RUSAGE_CHILDREN)`
  `ru_oublock * 512` of the command (the `write_bytes` of `/proc/<pid>/io`
  summed over the waited descendants); the device - the sectors written of
  `dm-0` in `/proc/diskstats`, with `sync` before and after. Wall time
  without the `sync`.
- Variants within the budget (4, 3 runs each): base; `split-debuginfo =
  "unpacked"` (as `CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=unpacked`);
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only` for the check;
  `CARGO_INCREMENTAL=0` for the check. The rule of the task: stop at the first
  variant that cuts the bytes written by 40% - the second did, the last two
  were not measured.

## Numbers

The cargo part of the check (two runs each):

| Variant | Wall, s | Written by the tree, GB | Written to the device, GB |
|---|---|---|---|
| base | 55.1, 69.4 | 4.93, 4.92 | 5.00, 4.94 |
| split-debuginfo | 42.8, 47.3 | 2.93, 2.93 | 2.95, 2.95 |

A clean build of the workspace tests (one run each):

| Variant | Wall, s | Written by the tree, GB | Written to the device, GB | `target/debug`, GB | The largest test binary, MB |
|---|---|---|---|---|---|
| base | 448 | 11.75 | 9.96 | 9.52 | 435 |
| split-debuginfo | 393 | 8.81 | 7.41 | 7.08 | 208 |

After the clippy step `target/debug` is 10.35 and 7.89 GB.

## Checks of the adopted setting

- A failing test (a panic three calls deep, a test of `notes-store`) with
  `RUST_BACKTRACE=1`: every frame names `./tests/<file>.rs:<line>:<col>`; the
  `.dwo` files sit in `deps/` next to the objects.
- The step `wasm` (`cargo clippy -p notes-wasm --target
  wasm32-unknown-unknown`) passes: cargo does not pass the flag for `wasm32`.
- Release builds (`tools/install.sh`, the WASM module) do not use the dev
  profile.
- The pruning of `tools/lib/target-limit.sh` removes `deps/*-<hash>*`, which
  covers the `.dwo` files of a unit.
