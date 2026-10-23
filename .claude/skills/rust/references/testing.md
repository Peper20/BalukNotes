# Testing

## Structure

- Unit tests: `#[cfg(test)] mod tests { use super::*; ... }` in the same file, so they see private items. Group the tests of one unit in a submodule when there are many (`mod process { ... }`); the module name then appears in the test output.
- Integration tests: `tests/` at the crate root, public API only. Several files each build into their own binary, so a shared setup goes in `tests/common/mod.rs`, or one binary `tests/it/main.rs` with modules. A binary crate keeps its logic in `src/lib.rs` and a thin `src/main.rs`, so the logic can be tested.
- Doc tests: examples in `///` comments for the public API (happy path). They run with `cargo test` but not with `cargo nextest`; run `cargo test --doc` separately in that case. Fence attributes: `no_run` (compile, do not run), `should_panic`, `compile_fail`, `text` for non-code. Hide setup lines with a leading `# `.

## Names and bodies

- Name a test after the behaviour and its condition: `returns_error_when_input_empty`, `process_should_return_blob_when_larger_than_b`. Not `test_add_happy_path`.
- A test checks one behaviour. Several asserts that all describe that behaviour are fine; unrelated behaviours go to separate tests.
- Arrange, act, assert, in that order, readable in the test body. Share setup and fixtures in a helper, but keep the action and the assertion inline even if tests look alike: duplication in tests is cheaper than a helper that couples them.
- Cover errors and edge cases, not only the happy path.
- Tests that need unavailable resources are `#[ignore = "reason"]`, not deleted or silently skipped.

## Assertions

- `assert!(cond, "message with {state}")` and `assert_eq!(actual, expected, "...")` take a format string; put the actual state in it when the default output would not show it.
- For a variant or shape: `assert_matches!(value, Pattern)` (std since 1.96; check `rust-version`), otherwise `assert!(matches!(value, Pattern), "got {value:?}")`; the message belongs to `assert!`, not to `matches!`.
- Helpers such as `pretty_assertions` (readable diffs) are fine where the project already uses them.

## Snapshots

When correctness is structural or visual (rendered output, serialized data), a snapshot test (`insta`, or the project's own files compared with expected output) says more than a pile of asserts. Review every snapshot diff as code; never accept new snapshots without reading them. Do not snapshot values that vary per run (time, ids, absolute paths); redact them.

## Parametrized tests

`rstest` cases with descriptive names (`#[case::empty("")]`) replace a copy-pasted test. They are harder to find with an IDE or grep and put the expectation first; use them only if the project already depends on it.
