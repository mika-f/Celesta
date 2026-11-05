//! Ending a bridge's runtime from another thread.

use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Ends a [`ReactBridge`](crate::ReactBridge)'s runtime from another thread,
/// for a deadline or a cancelled request. The runtime is killed, so a call
/// waiting on it (a `prepare()` that never resolves, a frame) fails as the
/// runtime exiting. Give a clone to
/// [`SpawnOptions::stop`](crate::SpawnOptions::stop); a runtime spawned
/// after [`Self::stop`] is killed as soon as it starts.
#[derive(Clone, Debug, Default)]
pub struct RuntimeStop(Arc<Mutex<StopState>>);

#[derive(Debug, Default)]
struct StopState {
    stopped: bool,
    runtime: Option<Arc<Mutex<Child>>>,
}

impl RuntimeStop {
    /// Kills the runtime, now or as soon as it is spawned. The bridge still
    /// reaps it when dropped.
    pub fn stop(&self) {
        let mut state = lock(&self.0);
        state.stopped = true;
        if let Some(runtime) = &state.runtime {
            // A runtime that was already reaped is not signalled again.
            let _ = lock(runtime).kill();
        }
    }

    /// Whether [`Self::stop`] has been called.
    pub fn is_stopped(&self) -> bool {
        lock(&self.0).stopped
    }

    /// Makes [`Self::stop`] kill `runtime`, at once when it was called
    /// already.
    pub(crate) fn watch(&self, runtime: &Arc<Mutex<Child>>) {
        let mut state = lock(&self.0);
        if state.stopped {
            let _ = lock(runtime).kill();
        }
        state.runtime = Some(Arc::clone(runtime));
    }
}

/// Locks `mutex`, also after a panic while it was held: the child and the
/// stop state stay usable either way.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
