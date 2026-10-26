# The project's part of the CPU cap (`check.env` of .claude/pipeline.json),
# sourced by the kit's pipeline/scripts/lib/cap.sh after CHECK_JOBS and `below`
# are set: the number is turned into the setting of each tool
# (`.claude/rules/tools.md`, "CPU cap"). Tools that use their own thread pools
# can overshoot a bare job count: the values below are the ones measured to keep
# the total at or under CHECK_JOBS + 1.

# cargo: rustc processes, build scripts, C compilers (jobserver) and the
# codegen threads of every rustc, which take tokens from the same jobserver.
# One below the cap: at the end of a build the linker (lld, its threads are
# limited in .cargo/config.toml) and C compilers run next to the last rustc.
# Overrides `[build] jobs` of ~/.cargo/config.toml.
export CARGO_BUILD_JOBS=$(below 1)
# The libtest harness: the test threads of all the binaries running at once
# (the step cargo-test of check.sh shares them between the binaries,
# tools/lib/cargo-test.sh; a plain `cargo test` runs the binaries in turn, each
# with all of them). One below: the test threads, the rayon pool and the server
# threads of the tests add up to more than the thread count.
export RUST_TEST_THREADS=$(below 1)
# The global rayon pool (Typst layout in tests and in the e2e server, the
# bundler of Vite). Two below: the core also builds two themes at once, each
# in its own thread, next to the pool (`notes-core::world::PARALLEL`).
export RAYON_NUM_THREADS=$(below 2)
# The default tokio runtimes (servers started by tests and e2e).
export TOKIO_WORKER_THREADS=$CHECK_JOBS
# Vitest: the workers that run test files (the main process is one more).
export VITEST_MAX_WORKERS=$(below 1)
# Playwright: the workers of the e2e, each with its own app server and browser
# (app/playwright.config.ts; unset there means one worker). A quarter of the
# cap, 1 to 8: at CHECK_JOBS=10 two workers took 72 s, three 78-82 s (the
# servers and browsers of more workers contend for the same cores).
pw=$(( CHECK_JOBS / 4 ))
export PLAYWRIGHT_WORKERS=$(( pw < 1 ? 1 : pw > 8 ? 8 : pw ))
unset pw

# freshen_sources: set the file times of every tracked file the Rust build can
# read to now. Cargo judges our own crates fresh by source mtime against its
# outputs, and the build dir is shared by all checkouts without the path in
# the unit hash: a build of another checkout, newer than these files, would be
# taken for ours (measured: a crate built from another worktree's edit was
# reused). With fresh times cargo recompiles our crates against this tree;
# dependencies from the registry and the foreign copies in vendor/ are not
# touched and stay built.
freshen_sources() {
  git ls-files -z -- crates baluk fonts docs/writing.md Cargo.toml Cargo.lock .cargo clippy.toml |
    xargs -0 -r touch -c
}

# The first holder of the lock after a run of another checkout freshens the
# sources (above): that run may have built its own tree into the shared dir. A
# repeated run of the same checkout keeps its artifacts (nothing else was built
# in between).
on_heavy_acquire() {
  [[ $1 == 1 ]] && freshen_sources
  return 0
}
