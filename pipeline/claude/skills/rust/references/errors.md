# Errors

## Replacing `unwrap` and `expect`

Pick the form by what the caller needs:

```rust
// Early return, the error value is not needed
let Ok(json) = serde_json::from_str::<Value>(&input) else {
    return Err(MyError::InvalidJson);
};

// Recovery that computes something
let config = if let Ok(c) = load(&path) { c } else { Config::default_for(&path) };

// Cheap default
let port = env_port.unwrap_or(8080);
let name = opt.unwrap_or_else(|| default_name(&ctx));
```

A function that has to deal with `None` returns `Result<T, E>` with a crate- or module-level error, not an `Option` that hides why. Convert with `.ok_or_else(|| Error::Missing(key))`, `.ok()`. To inspect or log an error without changing it, use `inspect_err`.

## Error types

Library: an enum with `thiserror`; one variant per failure the caller can tell apart.

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

- Layered code wraps the layer below with `#[from]` (`Db(#[from] DbError)`), so `?` converts.
- A module with a single kind of failure can use a struct instead of an enum.
- Messages are lowercase, without a trailing period, and say what failed with which input.
- Do not return `Box<dyn Error>` from a library: callers cannot match on it.
- Errors that cross threads or `.await` points need `Send + Sync + 'static`.

Binary: `anyhow::Result` with `.context("what was being done")` at each layer that adds information. Do not let `anyhow` leak into a library's public API; it erases what a caller might match on.

## Panics from code you do not control

A compiler or parser dependency may panic on input it cannot handle. Contain it where your code meets it:

- run it on its own thread and treat a failed `join` as an error of that one operation, or
- wrap the call in `std::panic::catch_unwind` (the closure must be `UnwindSafe`; `AssertUnwindSafe` is acceptable when you drop the state afterwards).

Convert the payload to an error value; do not let the panic take down the process or poison shared state. A panic hook that prints is a side effect: set it once, at startup.

## Testing errors

Assert the variant when the error type implements `PartialEq`, or `matches!` / `assert_matches!`; otherwise compare `err.to_string()`. Use `unwrap_err()` in tests, and print the `Err` when an `Ok` case fails:

```rust
let err = divide(10.0, 0.0).unwrap_err();
assert_eq!(err.to_string(), "division by zero");
```

`#[should_panic]` only when panicking is the specified behaviour.
