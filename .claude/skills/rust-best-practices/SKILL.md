---
name: rust-best-practices
description: >
  Guide for writing idiomatic Rust code based on Apollo GraphQL's best practices handbook. Use this skill when:
  (1) writing new Rust code or functions,
  (2) reviewing or refactoring existing Rust code,
  (3) deciding between borrowing vs cloning or ownership patterns,
  (4) implementing error handling with Result types,
  (5) optimizing Rust code for performance,
  (6) writing tests or documentation for Rust projects.
license: MIT
compatibility: Rust 1.81+, Cargo
metadata:
  author: apollographql
  version: "1.1.2-fixed.1"
allowed-tools: Bash(cargo:*) Bash(rustc:*) Bash(rustfmt:*) Bash(clippy:*) Read Write Edit Glob Grep
---

# Rust Best Practices

Apply these guidelines when writing or reviewing Rust code. Based on Apollo GraphQL's [Rust Best Practices Handbook](https://github.com/apollographql/rust-best-practices)
(MIT, `LICENSE`); the chapters in `references/` are the handbook text with broken examples fixed.

**The project comes first.** Before writing Rust, read the project's own rules (`CLAUDE.md`, the README of the
crate or workspace): where they differ from this skill, they win. Check `rust-version` in `Cargo.toml` and the
project's lint and format config before applying anything below.

## Best Practices Reference

Read the chapters relevant to the task (all of them for a review), in parallel:

- [Chapter 1 - Coding Styles and Idioms](references/chapter_01.md): Borrowing vs cloning, Copy trait, Option/Result handling, iterators, comments, when to extract a function (duplication vs. wrong abstraction)
- [Chapter 2 - Clippy and Linting](references/chapter_02.md): Clippy configuration, important lints, workspace lint setup
- [Chapter 3 - Performance Mindset](references/chapter_03.md): Profiling, avoiding redundant clones, stack vs heap, zero-cost abstractions
- [Chapter 4 - Error Handling](references/chapter_04.md): Result vs panic, thiserror vs anyhow, error hierarchies
- [Chapter 5 - Automated Testing](references/chapter_05.md): Test naming, one assertion per test, snapshot testing
- [Chapter 6 - Generics and Dispatch](references/chapter_06.md): Static vs dynamic dispatch, trait objects
- [Chapter 7 - Type State Pattern](references/chapter_07.md): Compile-time state safety, when to use it
- [Chapter 8 - Comments vs Documentation](references/chapter_08.md): When to comment, doc comments, rustdoc
- [Chapter 9 - Understanding Pointers](references/chapter_09.md): Thread safety, Send/Sync, pointer types

## Quick Reference

### Borrowing & Ownership
- Prefer `&T` over `.clone()` unless ownership transfer is required
- Use `&str` over `String` or `&String`, `&[T]` over `Vec<T>` or `&Vec<T>` in function parameters
- Small `Copy` types (≤24 bytes) can be passed by value
- Use `Cow<'_, T>` when ownership is ambiguous

### Error Handling
- Return `Result<T, E>` for fallible operations; avoid `panic!` in production
- No `unwrap()`/`expect()` outside tests: restructure first (`let ... else`, `?`, a constructor that cannot
  fail, a type that makes the case impossible)
- If failure is truly impossible and the type system cannot show it, `expect("<the invariant>")` - with
  `#[expect(clippy::expect_used, reason = "...")]` when the project enables that lint (handbook 4.2)
- Input from users, files, the network or other libraries is never an invariant
- A panic in a library you cannot fix (a compiler, a parser): catch it at the boundary - `join` of its
  thread or `std::panic::catch_unwind` - and make it an error of that one operation
- Use `thiserror` for library errors, `anyhow` for binaries only
- Prefer `?` operator over match chains for error propagation

### Performance
- Measure before optimizing; always benchmark with `--release`
- Run `cargo clippy -- -D clippy::perf` for performance hints
- Avoid cloning in loops; use `.iter()` instead of `.into_iter()` for Copy types
- Prefer iterators over manual loops; avoid intermediate `.collect()` calls

### Linting
Run regularly (or the project's own check script, if it has one):
`cargo clippy --all-targets --all-features --locked -- -D warnings`
(`--all-features` may not fit a workspace with platform-specific or tooling-only features.)

Key lints to watch:
- `redundant_clone` - unnecessary cloning
- `large_enum_variant` - oversized variants (consider boxing)
- `needless_collect` - premature collection

Silence a lint only locally, with `#[expect(clippy::lint, reason = "...")]` - not `#[allow(...)]`: `expect`
warns once the lint no longer fires.

`allow-unwrap-in-tests`/`allow-expect-in-tests` (`clippy.toml`) cover only `#[test]` and `#[cfg(test)]`; helpers
of integration tests (`tests/`) need `#![allow(clippy::unwrap_used, clippy::expect_used, reason = "...")]` on the
test crate - a rule for the whole crate, where `expect` would fail once the last `unwrap` is gone.

### Toolchain
- Do not use std APIs newer than the project's `rust-version` (e.g. `assert_matches!` needs 1.96).
- Import grouping options (`group_imports`, `imports_granularity`) need nightly rustfmt: do not add them to a
  project formatted with stable.

### Testing
- Name tests descriptively: `process_should_return_error_when_input_empty()`
- One assertion per test when possible
- Use doc tests (`///`) for public API examples
- Consider `cargo insta` for snapshot testing generated output

### Generics & Dispatch
- Prefer generics (static dispatch) for performance-critical code
- Use `dyn Trait` only when heterogeneous collections are needed
- Box at API boundaries, not internally

### Type State Pattern
Encode valid states in the type system to catch invalid operations at compile time:
```rust
struct Connection<State> { /* ... */ _state: PhantomData<State> }
struct Disconnected;
struct Connected;

impl Connection<Connected> {
    fn send(&self, data: &[u8]) { /* only connected can send */ }
}
```

### Documentation
- `//` comments explain *why* (safety, workarounds, design rationale)
- `///` doc comments explain *what* and *how* for public APIs
- Every `TODO` needs a linked issue: `// TODO(#42): ...`
- Enable `#![deny(missing_docs)]` for published libraries
