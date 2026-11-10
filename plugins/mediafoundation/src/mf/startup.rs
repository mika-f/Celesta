//! COM and Media Foundation start-up, once per [`crate::MfCodec`].

use std::thread::{self, ThreadId};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFShutdown, MFStartup};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// What a successful start-up holds, and gives back on drop.
pub(crate) struct Started {
    /// The thread that called `CoInitializeEx`, which alone may balance it:
    /// the provider may be moved to another thread before it is dropped.
    thread: ThreadId,
    /// Whether `CoInitializeEx` succeeded, so `CoUninitialize` is owed.
    com: bool,
}

impl Started {
    /// Joins the multithreaded apartment and starts Media Foundation in
    /// full, or says what failed, as `describe` gives it.
    pub(crate) fn new() -> Result<Self, String> {
        // SAFETY: plain calls with no pointers; each success is balanced in `drop`.
        let com = match unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
            Ok(()) => true,
            // A thread already in a single-threaded apartment keeps it, and
            // Media Foundation works there; nothing is owed.
            Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
            Err(error) => {
                return Err(format!("CoInitializeEx failed: 0x{:08X}", error.code().0));
            }
        };
        // SAFETY: as above; `MFShutdown` in `drop` balances it.
        if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            if com {
                // SAFETY: balances this call's `CoInitializeEx`, on its thread.
                unsafe { CoUninitialize() };
            }
            return Err(format!("MFStartup failed: 0x{:08X}", error.code().0));
        }
        Ok(Self {
            thread: thread::current().id(),
            com,
        })
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        // SAFETY: balances `MFStartup`; it may run on any thread.
        let _ = unsafe { MFShutdown() };
        if self.com && thread::current().id() == self.thread {
            // SAFETY: balances `CoInitializeEx`, on the thread that called it.
            unsafe { CoUninitialize() };
        }
    }
}
