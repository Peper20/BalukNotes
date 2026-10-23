# Concurrency and pointers

`Send`: the value can move to another thread. `Sync`: `&T` can be shared between threads. A pointer is thread-safe only if the data behind it is.

| Type | Use | Threads |
|---|---|---|
| `&T`, `&mut T` | borrow: shared read / exclusive write | `&T` is `Send` if `T: Sync` |
| `Box<T>` | single owner on the heap; recursive types, `dyn` | as `T` |
| `Rc<T>` | several owners, one thread | neither `Send` nor `Sync` |
| `Arc<T>` | several owners, many threads | `Send + Sync` if `T: Send + Sync` |
| `Cell<T>` | interior mutability for `Copy` values | not `Sync` |
| `RefCell<T>` | interior mutability, borrow rules checked at run time (panics on a violation) | not `Sync` |
| `Mutex<T>` | exclusive access across threads | `Send + Sync` if `T: Send` |
| `RwLock<T>` | many readers or one writer | `Sync` if `T: Send + Sync` |
| `OnceCell<T>`, `LazyCell<T>` | one-time / lazy init, one thread | not `Sync` |
| `OnceLock<T>`, `LazyLock<T>` | one-time / lazy init, usable in a `static` | `Sync` |
| `*const T`, `*mut T` | raw pointers: FFI and `unsafe` only | the author guarantees safety |

## Shared state

- Prefer passing owned data or messages over shared state: move a value into the thread, or use a channel.
- Read-only data shared by many threads: `Arc<T>`, `Arc<[T]>`, `Arc<str>`. Mutable: `Arc<Mutex<T>>`, or `Arc<RwLock<T>>` when reads dominate. Clone the `Arc`, not the data.
- Keep a lock's critical section short; do not call unknown code (callbacks, user closures) or block on I/O while holding it. Never hold two locks in different orders in different places.
- `std::sync::Mutex` is poisoned when a holder panics; decide what that means for the data and handle it on purpose (`lock().unwrap_or_else(PoisonError::into_inner)` when the data stays valid) instead of `unwrap()`. `parking_lot` locks do not poison.
- A `RefCell` panic is a bug, not an error: if borrows can overlap, restructure.
- A `static` that initializes once: `OnceLock` (value set by the caller's code) or `LazyLock` (value computed on first use). Avoid `static mut`.

## Async

- A future held across `.await` must be `Send` for a multi-threaded runtime: no `Rc`, no `std::sync::MutexGuard` alive over an `.await`.
- Use an async lock (`tokio::sync::Mutex`) only when the guard must live across an `.await`; otherwise a std or `parking_lot` mutex with a short section is cheaper and simpler.
- Blocking or CPU-heavy work (disk, compilation, hashing) goes to `spawn_blocking` or a dedicated thread, not inline in a task.
- Errors returned from spawned tasks need `Send + 'static`.

## `unsafe`

Avoid it. When it is unavoidable, keep the block minimal, put a `// SAFETY:` comment before it saying why the conditions hold, and wrap it in a safe function whose signature cannot be misused.
