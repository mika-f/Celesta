//! Stopping plugin processes from a thread other than the one waiting on
//! them: Cancel during an export's frame, or the editor dropping a preview
//! worker whose thread waits on a plugin.

use std::mem;
use std::process::Child;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, Weak};

/// Every plugin process this program started, for [`kill_every_plugin`].
static EVERY: LazyLock<PluginKillSwitch> = LazyLock::new(PluginKillSwitch::new);

/// Kills every plugin process this program started, and any it starts
/// after, as the editor does when it quits: a plugin that hangs never reads
/// the end of its stdin, and would outlive Celesta.
pub fn kill_every_plugin() {
    EVERY.kill();
}

/// Registers a process with [`kill_every_plugin`].
pub(crate) fn register_everywhere(process: PluginKillHandle) {
    EVERY.register(process);
}

/// A plugin process, shared by the [`crate::PluginChild`] that talks to it
/// and the [`PluginKillHandle`]s that can stop it.
pub(crate) struct SharedChild {
    pub(crate) child: Child,
    /// Set once a [`PluginKillHandle`] killed it, so the request it was
    /// waiting on fails as stopped rather than as a process that crashed.
    pub(crate) killed: bool,
}

pub(crate) fn lock(child: &Mutex<SharedChild>) -> MutexGuard<'_, SharedChild> {
    child.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Kills one plugin process from any thread. The request waiting on it then
/// fails with [`crate::RequestError::Stopped`]. It does not keep the process
/// alive: once its [`crate::PluginChild`] is dropped, killing does nothing.
#[derive(Clone)]
pub struct PluginKillHandle(pub(crate) Weak<Mutex<SharedChild>>);

impl PluginKillHandle {
    pub fn kill(&self) {
        if let Some(child) = self.0.upgrade() {
            let mut child = lock(&child);
            child.killed = true;
            // Reaped by its `PluginChild`, which waits on it next.
            let _ = child.child.kill();
        }
    }

    fn is_gone(&self) -> bool {
        self.0.strong_count() == 0
    }
}

/// Kills every plugin process registered with it, at once, from any thread;
/// and every one registered after that, as it is registered. Clones share
/// the switch. A [`crate::PluginHost`] and a `celesta_media::MediaBackend`
/// with one start no process once it is killed.
#[derive(Clone, Default)]
pub struct PluginKillSwitch(Arc<Mutex<Switch>>);

#[derive(Default)]
struct Switch {
    killed: bool,
    processes: Vec<PluginKillHandle>,
}

impl PluginKillSwitch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Kills every registered process; requests waiting on them return.
    pub fn kill(&self) {
        let processes = {
            let mut switch = self.lock();
            switch.killed = true;
            mem::take(&mut switch.processes)
        };
        for process in processes {
            process.kill();
        }
    }

    pub fn is_killed(&self) -> bool {
        self.lock().killed
    }

    /// Has [`Self::kill`] stop `process` too; stops it at once when the
    /// switch was killed already.
    pub fn register(&self, process: PluginKillHandle) {
        let mut switch = self.lock();
        if switch.killed {
            drop(switch);
            process.kill();
            return;
        }
        switch.processes.retain(|process| !process.is_gone());
        switch.processes.push(process);
    }

    fn lock(&self) -> MutexGuard<'_, Switch> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl std::fmt::Debug for PluginKillSwitch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginKillSwitch")
            .field("killed", &self.is_killed())
            .finish_non_exhaustive()
    }
}
