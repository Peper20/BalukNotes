//! A monotonic clock on every target: `std::time::Instant` natively; in the
//! browser (WASM) `std`'s panics, so it comes from `performance.now()`.

#[cfg(not(target_family = "wasm"))]
pub use std::time::Instant;
#[cfg(target_family = "wasm")]
pub use web_time::Instant;
