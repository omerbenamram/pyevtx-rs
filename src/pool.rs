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

/// How many threads the pool has: one per core.
pub(crate) fn num_threads() -> usize {
    thread::available_parallelism().map_or(1, usize::from)
}

/// This process's pool, made when it is first asked for.
pub(crate) fn pool() -> Result<Arc<ThreadPool>, rayon::ThreadPoolBuildError> {
    let process = std::process::id();
    let mut pool = POOL.lock().unwrap_or_else(PoisonError::into_inner);
    match &*pool {
        Some((owner, pool)) if *owner == process => return Ok(Arc::clone(pool)),
        _ => {}
    }
    let new = Arc::new(
        ThreadPoolBuilder::new()
            .num_threads(num_threads())
            .thread_name(|index| format!("evtx-{index}"))
            .build()?,
    );
    if let Some((_, parents)) = pool.replace((process, Arc::clone(&new))) {
        // Its threads are the parent's: there is nothing to stop here, and dropping it would
        // tell threads that don't exist to.
        std::mem::forget(parents);
    }
    Ok(new)
}
