//! The thread pool that chunks are parsed on.
//!
//! It is not rayon's global pool, because threads don't survive a `fork`: in a child process
//! (a `multiprocessing` worker started with the fork method, the default on Linux before
//! Python 3.14), a pool made in the parent has no threads left, and work handed to it is never
//! run. So the pool belongs to the process that made it, and a process that finds its parent's
//! makes its own.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use rayon::{ThreadPool, ThreadPoolBuilder};

/// The pool, and the ID of the process that made it.
static POOL: Mutex<Option<(u32, Arc<ThreadPool>)>> = Mutex::new(None);

/// How many threads the pool has: `RAYON_NUM_THREADS` if it is set, as for rayon's own pool,
/// or one per core.
pub(crate) fn num_threads() -> usize {
    std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|threads| threads.parse().ok())
        .filter(|&threads| threads > 0)
        .unwrap_or_else(|| thread::available_parallelism().map_or(1, usize::from))
}

/// This process's pool, made when it is first asked for.
pub(crate) fn pool() -> Result<Arc<ThreadPool>, rayon::ThreadPoolBuildError> {
    let process = std::process::id();
    // The lock is held only to look and to store, never while threads start: a fork in
    // another thread copies a lock that is held, and nothing in the child would release it.
    let lock = || POOL.lock().unwrap_or_else(PoisonError::into_inner);
    let ours = |slot: &Option<(u32, Arc<ThreadPool>)>| match slot {
        Some((owner, pool)) if *owner == process => Some(Arc::clone(pool)),
        _ => None,
    };
    if let Some(pool) = ours(&lock()) {
        return Ok(pool);
    }
    let new = Arc::new(
        ThreadPoolBuilder::new()
            .num_threads(num_threads())
            .thread_name(|index| format!("evtx-{index}"))
            .build()?,
    );
    let mut pool = lock();
    // Another thread made one meanwhile.
    if let Some(pool) = ours(&pool) {
        return Ok(pool);
    }
    if let Some((_, parents)) = pool.replace((process, Arc::clone(&new))) {
        // Its threads are the parent's: there is nothing to stop here, and dropping it would
        // tell threads that don't exist to.
        std::mem::forget(parents);
    }
    Ok(new)
}
