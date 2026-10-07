---
paths:
  - "vendor/**"
---

# Copies of foreign crates with patches

Wired through `[patch.crates-io]` of the root `Cargo.toml`; they are not
workspace members (`exclude`), project lints do not apply to them. A patch is
as small as possible, with a test next to the code. Remove a copy when the
patch ships in the crate (update together with Typst: the version is the one
`typst` pulls in).

## comemo 0.5.1

Source - the crates.io package `comemo-0.5.1` (MIT OR Apache-2.0, licenses
next to it), `Cargo.toml` - normalized from the package.

**Patch** (`src/tree.rs`, `CallTree::retain`): if the call tree is empty after
eviction, it is recreated. `evict` removes entries but keeps the capacity of
the slab and hash tables - after warming a vault that is ~0.85 GB that
`comemo::evict(0)` did not return (dhat, `docs/tech-debt.md`). With the patch
process memory after warming `tests/vault` is 0.70 GB instead of 1.34 GB.
Ordinary evictions (`evict(N)` after a build) do not empty the tree and
recreate nothing. Test - `test_call_tree` (the last steps).

**Patch 2** (`src/accelerate.rs`, `evict`; `src/memoize.rs`): a full eviction
(`evict(0)`) also frees the accelerators (the vector and the call hash
tables), not just clears them - after warming `tests/vault` that is ~126 MB of
live heap (`docs/research/E5.md`). `evict(N)` after a build keeps the memory,
as before. Test - `test_evict_release`.

Checking the copy:

```sh
CARGO_TARGET_DIR=target/vendor cargo test --manifest-path vendor/comemo/Cargo.toml --features testing
```

Proposing the patch to `typst/comemo` is the user's decision (a PR in their
name).
