---
name: rust
description: Rules for writing, changing and reviewing Rust code - errors, borrowing, clippy, iterators, generics, concurrency, docs, tests. Load before the first edit of a .rs file and before reviewing a Rust change, even a one-line fix or a test; load again after a context compaction. Not needed to only read Rust, run cargo, or edit documents that mention .rs files.
---

# Rust

Based on the Apollo GraphQL Rust Best Practices handbook (MIT, `LICENSE`). Language-neutral rules (comments, naming, abstraction, scope) are in the `code-quality` skill.

The project comes first: where its own rules (`CLAUDE.md`, the README of the crate or workspace) differ from this skill, they win. Before writing, check `rust-version` in `Cargo.toml` and the lint and format config.

Read the reference when the task touches its topic (all three for a review of a large change):

- [references/errors.md](references/errors.md): error types, `thiserror` and `anyhow`, replacing `unwrap`, panics at a boundary.
- [references/testing.md](references/testing.md): test structure and names, assertions, doc tests, snapshots.
- [references/concurrency.md](references/concurrency.md): `Send` and `Sync`, pointer and lock types, lazy statics, async.

## Errors

- A fallible function returns `Result<T, E>`. Use `?` to propagate; use `map_err`, `or_else` or `if let` only when you recover or convert.
- No `unwrap()` or `expect()` outside tests. Restructure first: `let ... else`, `?`, `unwrap_or_else`, a constructor that cannot fail, a type that makes the case impossible.
- When failure is impossible and the types cannot show it, `expect("<the invariant>")` is allowed, under `#[expect(clippy::expect_used, reason = "...")]` if the project enables that lint.
- Input from users, files, the network or other libraries is never an invariant.
- A panic in a library you cannot fix (a compiler, a parser) is caught at the boundary (`join` of its thread, `std::panic::catch_unwind`) and becomes an error of that one operation.
- Libraries define their errors with `thiserror`; `anyhow` only in binaries (and test helpers).
- `unreachable!` states an invariant you have reasoned out; `todo!` and `unimplemented!` do not reach main.

## Borrowing and ownership

Borrow by default; clone only when you need an owned copy (storing, sending to another thread, changing while keeping the original), and as late as possible. A clone made to silence the borrow checker usually means the API or lifetimes need adjusting. Never clone inside a hot loop. `Cow` fits when ownership depends on a runtime branch. Pass `Copy` types by value.

Parameters take the widest borrowed form: `&str` not `&String`, `&[T]` not `&Vec<T>`, `&Path` not `&PathBuf`. Take ownership only when the function stores or consumes the value, and say so in the signature.

Defer allocation: `ok_or_else`, `unwrap_or_else`, `map_or_else` when the fallback allocates or computes.

## Iterators and control flow

- Use iterator chains to transform collections; use a `for` loop for early exits and side effects. Choose what reads better.
- Do not `collect` an intermediate collection only to iterate it again; pass or return `impl Iterator`.
- `let ... else` for the early-return path, `match` when you handle several variants, `if let ... else` when the else branch computes something.
- Never derive both `Copy` and `Iterator` on one type.

## Generics and dispatch

Start with generics (`impl Trait`, `<T: Trait>`). Use `dyn Trait` for a heterogeneous collection, a plugin-style boundary, or to keep compile time and code size down. Prefer `&dyn Trait` when you do not need ownership; `Arc<dyn Trait + Send + Sync>` for sharing across threads. A trait with methods returning `Self` is not object-safe. Encode valid states in types (type state, newtypes) when a wrong order of calls is a real bug, not by default.

## Concurrency

Types crossing threads must be `Send`; shared references need `Sync`. Share read-only data with `Arc<T>` or `Arc<[T]>`, mutable data with `Arc<Mutex<T>>` or `Arc<RwLock<T>>`; `Rc`, `RefCell` and `Cell` are single-threaded. Do not hold a lock across `.await`. Keep blocking work out of async tasks. Details in the reference.

## Lints and formatting

- Run clippy over everything with warnings as errors: `cargo clippy --all-targets --all-features --locked -- -D warnings`, or the project's check script (`--all-features` may not fit a workspace with platform-specific or tooling-only features).
- Fix a warning rather than silence it. When silencing is right, do it locally with `#[expect(clippy::lint, reason = "...")]`, not `#[allow]`: `expect` warns once the lint stops firing.
- `allow-unwrap-in-tests` and `allow-expect-in-tests` in `clippy.toml` cover only `#[test]` and `#[cfg(test)]`. Helpers of integration tests (`tests/`) need `#![allow(clippy::unwrap_used, clippy::expect_used, reason = "...")]` on the test crate.
- Run `cargo fmt`. Do not add nightly-only rustfmt options (`group_imports`, `imports_granularity`) to a project formatted with stable.

## Toolchain

Do not use std APIs newer than the project's `rust-version` (for example `assert_matches!` needs 1.96).

## Performance

Measure before optimizing and benchmark with `--release`. Do not clone in hot loops. `#[inline]` only when a benchmark shows a gain. Box large enum variants (`clippy::large_enum_variant`) and recursive types.

## Documentation

- `///` documents public items: what it does, `# Errors`, `# Panics`, `# Safety` where they apply, an example for non-obvious use. `//!` documents a module or crate.
- Examples in doc comments are tests; keep them compiling.
- `// SAFETY:` precedes every `unsafe` block and states why it is sound.
- Other comments follow `code-quality`.

## Tests

Unit tests sit in `#[cfg(test)] mod tests` next to the code; integration tests in `tests/` use only the public API. A test checks one behaviour; several asserts that describe it are fine. Name a test after the behaviour it checks, such as `parse_returns_error_when_input_empty`. Test errors too. Details in the reference.
