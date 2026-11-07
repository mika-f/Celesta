//! Keeping the texture cache from growing without bound.
//!
//! An open writes the model's images to the cache (see `materials`) and
//! answers their paths, and Celesta reads them straight away: a renderer
//! decodes every texture file of a model while it loads the model, and
//! never again (a model it drops is opened again, which writes or finds
//! its images again). So a file is needed only for a moment after an open
//! last wrote or found it, in whichever process.
//!
//! - **Last use** is a file's modification time: an open that finds its
//!   image already there sets it to now before it compares the bytes, and
//!   one that writes the image makes a new file.
//! - **The lock**, `celesta-plugin-vrm.lock` in the cache: an open holds it
//!   shared while it writes and finds its images, and a sweep holds it
//!   exclusive. So no sweep runs between an open finding a file and marking
//!   it used, and opens in many processes still write side by side.
//! - **The sweep** runs after each open that wrote or found an image,
//!   unless another process holds the lock (then that process's open
//!   sweeps). Of the files the plugin names (`<sha256>.<png|jpg|webp>`, and
//!   the temporary files they are written through), it removes images
//!   unused for [`MAX_AGE`], then the least recently used until the rest
//!   fit [`SIZE_BUDGET`]. Nothing used within [`IN_USE`] is removed, past
//!   the budget or not: a renderer may not have read it yet. A temporary
//!   file older than that is one a process left when it died. Any other
//!   file, a directory, or a link is left alone, so the cache can be a
//!   folder of the user's own.
//!
//! The sweep never fails an open: a file it cannot read or remove stays.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The lock file's name in the cache.
pub(crate) const LOCK_NAME: &str = "celesta-plugin-vrm.lock";

/// How long a file stays after its last use however full the cache is:
/// far longer than a renderer takes to read what an open answered.
pub(crate) const IN_USE: Duration = Duration::from_secs(24 * 60 * 60);

/// How long an image stays unused before a sweep removes it.
pub(crate) const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The most the cache's images may take on disk before a sweep removes the
/// least recently used.
pub(crate) const SIZE_BUDGET: u64 = 2 << 30;

/// When a sweep removes a file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Policy {
    /// Nothing used within this long is removed.
    pub in_use: Duration,
    /// An image unused for longer is removed.
    pub max_age: Duration,
    /// The bytes the images may take before the least recently used go.
    pub budget: u64,
}

impl Policy {
    /// [`IN_USE`], [`MAX_AGE`], and [`SIZE_BUDGET`].
    pub(crate) const DEFAULT: Self = Self {
        in_use: IN_USE,
        max_age: MAX_AGE,
        budget: SIZE_BUDGET,
    };
}

/// The cache's lock, held until this is dropped.
pub(crate) struct CacheLock {
    _file: File,
}

impl CacheLock {
    /// The lock held shared, once no sweep holds it. `None` where the lock
    /// file cannot be made or locked (a file system without locks): the
    /// open writes without it, and no sweep can take the lock while the
    /// file cannot be locked.
    pub(crate) fn shared(cache: &Path) -> Option<Self> {
        let file = open_lock(cache).ok()?;
        file.lock_shared().ok()?;
        Some(Self { _file: file })
    }

    /// The lock held exclusive, if no one holds it now.
    fn exclusive(cache: &Path) -> Option<Self> {
        let file = open_lock(cache).ok()?;
        file.try_lock().ok()?;
        Some(Self { _file: file })
    }
}

/// The lock file, made private where it is new, never through a link.
fn open_lock(cache: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(cache.join(LOCK_NAME))
}

/// What a sweep did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Swept {
    /// The files it removed.
    pub removed: usize,
    /// The bytes the images left take.
    pub kept: u64,
}

/// Sweeps `cache` by `policy` as of `now`, unless another process holds
/// the lock: then `None`, and nothing is removed.
pub(crate) fn sweep(cache: &Path, policy: &Policy, now: SystemTime) -> Option<Swept> {
    let _lock = CacheLock::exclusive(cache)?;
    Some(sweep_locked(cache, policy, now))
}

/// What a file of the cache is to the plugin.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    /// `<sha256>.<extension>`.
    Image,
    /// `<sha256>.<extension>.<pid>-<n>.tmp`, an image being written.
    Temporary,
}

/// The extensions `materials` names images with.
const EXTENSIONS: [&str; 3] = ["png", "jpg", "webp"];

/// What the plugin named `name`, or `None` for a name it never writes.
fn kind_of(name: &str) -> Option<Kind> {
    let (hash, rest) = name.split_at_checked(64)?;
    if !hash
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return None;
    }
    let rest = rest.strip_prefix('.')?;
    let (extension, temporary) = match rest.split_once('.') {
        Some((extension, temporary)) => (extension, Some(temporary)),
        None => (rest, None),
    };
    if !EXTENSIONS.contains(&extension) {
        return None;
    }
    let Some(temporary) = temporary else {
        return Some(Kind::Image);
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let (process, number) = temporary.strip_suffix(".tmp")?.split_once('-')?;
    (digits(process) && digits(number)).then_some(Kind::Temporary)
}

/// [`sweep`] with the lock held.
fn sweep_locked(cache: &Path, policy: &Policy, now: SystemTime) -> Swept {
    let Ok(entries) = fs::read_dir(cache) else {
        return Swept::default();
    };
    let mut removed = 0;
    // (unused for, bytes, path)
    let mut images: Vec<(Duration, u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let Some(kind) = entry.file_name().to_str().and_then(kind_of) else {
            continue;
        };
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        // A time past now (the clock moved back) counts as just used.
        let unused = now.duration_since(modified).unwrap_or(Duration::ZERO);
        match kind {
            Kind::Temporary => {
                if unused > policy.in_use && fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
            Kind::Image => images.push((unused, metadata.len(), path)),
        }
    }
    // The least recently used first.
    images.sort_by_key(|&(unused, _, _)| std::cmp::Reverse(unused));
    let mut kept = images
        .iter()
        .map(|&(_, bytes, _)| bytes)
        .fold(0u64, u64::saturating_add);
    for (unused, bytes, path) in images {
        // The rest are used more recently still.
        if unused <= policy.in_use || (unused <= policy.max_age && kept <= policy.budget) {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            kept = kept.saturating_sub(bytes);
            removed += 1;
        }
    }
    Swept { removed, kept }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::write_cached;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::TempDir;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn name(bytes: &[u8]) -> String {
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("{hash}.png")
    }

    /// `bytes` written to the cache under their name, last used `ago`.
    fn image(cache: &Path, bytes: &[u8], ago: Duration) -> PathBuf {
        let path = cache.join(name(bytes));
        fs::write(&path, bytes).unwrap();
        used(&path, ago);
        path
    }

    fn used(path: &Path, ago: Duration) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - ago)
            .unwrap();
    }

    fn policy(budget: u64) -> Policy {
        Policy {
            in_use: DAY,
            max_age: 30 * DAY,
            budget,
        }
    }

    #[test]
    fn only_names_the_plugin_writes_are_its() {
        let hash = "0123456789abcdef".repeat(4);
        assert_eq!(kind_of(&format!("{hash}.png")), Some(Kind::Image));
        assert_eq!(kind_of(&format!("{hash}.jpg")), Some(Kind::Image));
        assert_eq!(kind_of(&format!("{hash}.webp")), Some(Kind::Image));
        assert_eq!(
            kind_of(&format!("{hash}.png.123-4.tmp")),
            Some(Kind::Temporary)
        );
        for other in [
            format!("{hash}.gif"),
            format!("{hash}.png.tmp"),
            format!("{hash}.png.12-.tmp"),
            format!("{hash}.png.12-3"),
            format!("{hash}.png.a-3.tmp"),
            format!("{}.png", hash.to_uppercase()),
            format!("{}.png", &hash[1..]),
            format!("{hash}0.png"),
            format!("{hash}png"),
            "photo.png".to_owned(),
            LOCK_NAME.to_owned(),
            "ü".repeat(40),
        ] {
            assert_eq!(kind_of(&other), None, "{other}");
        }
    }

    #[test]
    fn images_unused_past_the_age_are_removed() {
        let cache = TempDir::new().unwrap();
        let old = image(cache.path(), b"old", 31 * DAY);
        let recent = image(cache.path(), b"recent", 29 * DAY);
        let swept = sweep(cache.path(), &policy(u64::MAX), SystemTime::now()).unwrap();
        assert_eq!(
            swept,
            Swept {
                removed: 1,
                kept: 6
            }
        );
        assert!(!old.exists());
        assert!(recent.exists());
    }

    #[test]
    fn the_least_recently_used_go_first_past_the_budget() {
        let cache = TempDir::new().unwrap();
        let oldest = image(cache.path(), &[1; 100], 5 * DAY);
        let older = image(cache.path(), &[2; 100], 4 * DAY);
        let newer = image(cache.path(), &[3; 100], 3 * DAY);
        let swept = sweep(cache.path(), &policy(150), SystemTime::now()).unwrap();
        assert_eq!(
            swept,
            Swept {
                removed: 2,
                kept: 100
            }
        );
        assert!(!oldest.exists());
        assert!(!older.exists());
        assert!(newer.exists());
    }

    /// A file used within `in_use` stays however full the cache is: a
    /// renderer may still be about to read it.
    #[test]
    fn files_in_use_stay_past_the_budget() {
        let cache = TempDir::new().unwrap();
        let old = image(cache.path(), &[1; 100], 2 * DAY);
        let in_use = image(cache.path(), &[2; 100], Duration::from_secs(60));
        let just_written = cache.path().join(name(&[3; 100]));
        fs::write(&just_written, [3; 100]).unwrap();
        let swept = sweep(cache.path(), &policy(0), SystemTime::now()).unwrap();
        assert_eq!(
            swept,
            Swept {
                removed: 1,
                kept: 200
            }
        );
        assert!(!old.exists());
        assert!(in_use.exists());
        assert!(just_written.exists());
    }

    /// A file whose time is ahead of the clock counts as just used.
    #[test]
    fn a_time_past_now_counts_as_in_use() {
        let cache = TempDir::new().unwrap();
        let path = image(cache.path(), b"ahead", Duration::ZERO);
        let swept = sweep(cache.path(), &policy(0), SystemTime::now() - 60 * DAY).unwrap();
        assert_eq!(swept.removed, 0);
        assert!(path.exists());
    }

    #[test]
    fn temporary_files_left_by_a_dead_process_are_removed() {
        let cache = TempDir::new().unwrap();
        let hash = name(b"x");
        let left = cache.path().join(format!("{hash}.1-0.tmp"));
        let writing = cache.path().join(format!("{hash}.2-0.tmp"));
        fs::write(&left, b"left").unwrap();
        used(&left, 2 * DAY);
        fs::write(&writing, b"writing").unwrap();
        let swept = sweep(cache.path(), &policy(0), SystemTime::now()).unwrap();
        assert_eq!(swept.removed, 1);
        assert!(!left.exists());
        assert!(writing.exists());
    }

    /// Files the plugin did not name, directories, and links are never
    /// removed, so the cache can be a folder that holds other things.
    #[test]
    fn what_the_plugin_did_not_write_stays() {
        let cache = TempDir::new().unwrap();
        let photo = cache.path().join("photo.png");
        fs::write(&photo, [0; 100]).unwrap();
        used(&photo, 365 * DAY);
        let directory = cache.path().join(name(b"directory"));
        fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        let link = {
            let target = cache.path().join("target");
            fs::write(&target, b"target").unwrap();
            let link = cache.path().join(name(b"link"));
            std::os::unix::fs::symlink(&target, &link).unwrap();
            link
        };
        let swept = sweep(cache.path(), &policy(0), SystemTime::now() + 365 * DAY).unwrap();
        assert_eq!(swept, Swept::default());
        assert!(photo.exists());
        assert!(directory.is_dir());
        #[cfg(unix)]
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(cache.path().join(LOCK_NAME).is_file());
    }

    /// While an open holds the lock to write or find its images, no sweep
    /// runs: the next open sweeps.
    #[test]
    fn a_held_lock_skips_the_sweep() {
        let cache = TempDir::new().unwrap();
        let old = image(cache.path(), b"old", 60 * DAY);
        let lock = CacheLock::shared(cache.path()).unwrap();
        assert_eq!(sweep(cache.path(), &policy(0), SystemTime::now()), None);
        assert!(old.exists());
        // Other opens still write beside it.
        let other = CacheLock::shared(cache.path()).unwrap();
        drop((lock, other));
        let swept = sweep(cache.path(), &policy(0), SystemTime::now()).unwrap();
        assert_eq!(swept.removed, 1);
        assert!(!old.exists());
    }

    /// A sweep in progress holds back opens, which then write again what it
    /// removed.
    #[test]
    fn an_open_waits_for_a_sweep() {
        let cache = TempDir::new().unwrap();
        let sweeping = CacheLock::exclusive(cache.path()).unwrap();
        assert!(CacheLock::exclusive(cache.path()).is_none());
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let open = scope.spawn(|| {
                let lock = CacheLock::shared(cache.path());
                assert!(done.load(Ordering::SeqCst), "the open did not wait");
                lock.is_some()
            });
            std::thread::sleep(Duration::from_millis(100));
            done.store(true, Ordering::SeqCst);
            drop(sweeping);
            assert!(open.join().unwrap());
        });
    }

    /// Opens finding images that are past the age and the budget, while
    /// sweeps run beside them: every path an open answers still holds its
    /// bytes when the open has answered. The opens and sweeps are threads
    /// with their own lock files open, which the lock keeps apart as it
    /// does processes.
    #[test]
    fn opens_racing_sweeps_keep_the_files_they_answer() {
        const ROUNDS: usize = 20;
        let cache = TempDir::new().unwrap();
        let images: Vec<Vec<u8>> = (0..24u8).map(|i| vec![i; 4096]).collect();
        let policy = Policy {
            in_use: Duration::from_secs(60 * 60),
            max_age: Duration::from_secs(2 * 60 * 60),
            budget: 0,
        };
        for _ in 0..ROUNDS {
            for bytes in &images {
                let path = cache.path().join(name(bytes));
                if path.exists() {
                    used(&path, DAY);
                }
            }
            std::thread::scope(|scope| {
                for open in 0..4 {
                    let images = &images;
                    let cache = cache.path();
                    scope.spawn(move || {
                        for bytes in images.iter().cycle().skip(open * 6).take(images.len()) {
                            let path = {
                                let _lock = CacheLock::shared(cache).unwrap();
                                write_cached(cache, &name(bytes), bytes).unwrap()
                            };
                            assert_eq!(&fs::read(&path).unwrap(), bytes, "{}", path.display());
                        }
                    });
                }
                for _ in 0..2 {
                    let cache = cache.path();
                    scope.spawn(move || {
                        for _ in 0..50 {
                            sweep(cache, &policy, SystemTime::now());
                        }
                    });
                }
            });
        }
        // Everything was used within the hour, so it all stays.
        let swept = sweep(cache.path(), &policy, SystemTime::now()).unwrap();
        assert_eq!(swept.removed, 0);
        assert_eq!(swept.kept, 24 * 4096);
    }
}
