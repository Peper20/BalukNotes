use std::sync::atomic::{AtomicUsize, Ordering};

use rustc_hash::FxHashMap;
use parking_lot::{MappedRwLockReadGuard, Mutex, RwLock, RwLockReadGuard};

/// The global list of currently alive accelerators.
static ACCELERATORS: RwLock<(usize, Vec<Accelerator>)> = RwLock::new((0, Vec::new()));

/// The current ID of the accelerator.
static ID: AtomicUsize = AtomicUsize::new(0);

/// The type of each individual accelerator.
///
/// Maps from call hashes to return hashes.
type Accelerator = Mutex<FxHashMap<u128, u128>>;

/// Generate a new accelerator.
pub fn id() -> usize {
    // Get the next ID.
    ID.fetch_add(1, Ordering::SeqCst)
}

/// Evict the accelerators. With `release`, also free their memory (a full
/// eviction: the vector and the maps can hold hundreds of megabytes).
pub fn evict(release: bool) {
    let mut accelerators = ACCELERATORS.write();
    let (offset, vec) = &mut *accelerators;

    // Update the offset.
    *offset = ID.load(Ordering::SeqCst);

    if release {
        *vec = Vec::new();
        return;
    }

    // Clear all accelerators while keeping the memory allocated.
    vec.iter_mut().for_each(|accelerator| accelerator.lock().clear())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evict_release() {
        let a = id();
        get(a).unwrap().lock().insert(1, 2);
        evict(false);
        assert!(get(a).is_none());
        assert!(ACCELERATORS.read().1.capacity() > 0);
        let b = id();
        get(b).unwrap().lock().insert(3, 4);
        evict(true);
        assert_eq!(ACCELERATORS.read().1.capacity(), 0);
        let c = id();
        assert!(get(c).unwrap().lock().is_empty());
    }
}

/// Get an accelerator by ID.
pub fn get(id: usize) -> Option<MappedRwLockReadGuard<'static, Accelerator>> {
    // We always lock the accelerators, as we need to make sure that the
    // accelerator is not removed while we are reading it.
    let mut accelerators = ACCELERATORS.read();

    let mut i = id.checked_sub(accelerators.0)?;
    if i >= accelerators.1.len() {
        drop(accelerators);
        resize(i + 1);
        accelerators = ACCELERATORS.read();

        // Because we release the lock before resizing the accelerator, we need
        // to check again whether the ID is still valid because another thread
        // might evicted the cache.
        i = id.checked_sub(accelerators.0)?;
    }

    Some(RwLockReadGuard::map(accelerators, move |(_, vec)| &vec[i]))
}

/// Adjusts the amount of accelerators.
#[cold]
fn resize(len: usize) {
    let mut pair = ACCELERATORS.write();
    if len > pair.1.len() {
        pair.1.resize_with(len, || Mutex::new(FxHashMap::default()));
    }
}
