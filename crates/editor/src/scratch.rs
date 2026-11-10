//! Folders of their own for tests.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A new, empty folder under the system's temporary folder, named
/// `celesta-{name}-{pid}-{n}`.
///
/// The counter gives every call in the test process a folder of its own. A
/// clock reading does not: tests run in parallel, and on macOS two of them
/// can read the same microsecond. A folder left by an earlier run whose
/// process had the same id is cleared first.
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "celesta-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
