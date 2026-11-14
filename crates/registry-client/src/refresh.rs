//! The advisory refresh the `celesta` command runs, bounded to one budget.

use std::sync::mpsc;
use std::time::Duration;

use celesta_registry_format::AdvisoryList;
use time::OffsetDateTime;

use crate::Registry;

/// How a bounded advisory refresh ended.
#[derive(Debug)]
pub enum AdvisoryRefresh {
    /// The list was fetched, verified, and cached.
    Fresh(AdvisoryList),
    /// The refresh failed; the message says why. The cached list stays.
    Failed(String),
    /// The budget passed first; the cached list is used.
    TimedOut,
}

/// Refreshes the advisory list of `registry`, giving up after `budget`.
///
/// The refresh runs on a thread that owns `registry`. When the budget
/// passes the thread is left to finish, or to die with the process.
pub fn refresh_advisories_within(
    registry: Registry,
    now: OffsetDateTime,
    budget: Duration,
) -> AdvisoryRefresh {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        // The receiver is gone once the budget has passed.
        let _ = sender.send(registry.refresh_advisories(now));
    });
    match receiver.recv_timeout(budget) {
        Ok(Ok(list)) => AdvisoryRefresh::Fresh(list),
        Ok(Err(error)) => AdvisoryRefresh::Failed(error.to_string()),
        Err(_) => AdvisoryRefresh::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::time::Instant;

    use celesta_registry_format::RootKey;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::testing::{BASE, TestRegistry, at};
    use crate::{DirTransport, Transport, TransportError};

    struct Hanging;

    impl Transport for Hanging {
        fn open(&self, _url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
            std::thread::sleep(Duration::from_secs(5));
            Err(TransportError::NotFound(String::new()))
        }
    }

    fn now() -> OffsetDateTime {
        at("2026-11-21T00:00:00Z")
    }

    #[test]
    fn a_hanging_transport_times_out_within_the_budget() {
        let cache = TempDir::new().unwrap();
        let root = TestRegistry::new().root_key();
        let registry = Registry::new(BASE, root, Box::new(Hanging), cache.path().to_path_buf());
        let started = Instant::now();
        let outcome = refresh_advisories_within(registry, now(), Duration::from_millis(200));
        assert!(matches!(outcome, AdvisoryRefresh::TimedOut), "{outcome:?}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_working_registry_is_fresh() {
        let served = TestRegistry::new();
        served.advisories(json!([{
            "id": "CELESTA-2026-0009",
            "plugin": "acme.one",
            "versions": "<1.0.1",
            "title": "Bad",
            "description": "Does bad things.",
            "fixed": "1.0.1",
            "published": "2026-11-20T00:00:00Z",
        }]));
        let cache = TempDir::new().unwrap();
        let registry = served.client(cache.path());
        let list = match refresh_advisories_within(registry, now(), Duration::from_secs(5)) {
            AdvisoryRefresh::Fresh(list) => list,
            other => panic!("{other:?}"),
        };
        assert_eq!(list.advisories.len(), 1);
        let cached = served.client(cache.path()).cached_advisories().unwrap();
        assert_eq!(cached, Some(list));
    }

    #[test]
    fn a_failure_is_reported() {
        let cache = TempDir::new().unwrap();
        let empty = TempDir::new().unwrap();
        let transport = DirTransport {
            base: BASE.into(),
            root: empty.path().to_path_buf(),
        };
        let root: RootKey = TestRegistry::new().root_key();
        let registry = Registry::new(BASE, root, Box::new(transport), cache.path().to_path_buf());
        match refresh_advisories_within(registry, now(), Duration::from_secs(5)) {
            AdvisoryRefresh::Failed(message) => {
                assert!(
                    message.contains("https://registry.test/config.json"),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
