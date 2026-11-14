//! The security advisory list one `celesta` run uses: refreshed once,
//! within [`CLI_ADVISORY_TIMEOUT`], else the cached list.

use celesta_plugin_host::advisories_stale;
use celesta_registry_client::{AdvisoryRefresh, CLI_ADVISORY_TIMEOUT, refresh_advisories_within};
use celesta_registry_format::AdvisoryList;
use time::OffsetDateTime;

use crate::report;
use crate::session::Session;

/// Refreshes the advisory list when the session has a key, then returns
/// the newest verified list, warning when it could not be refreshed, is
/// missing, or is old. `search`, `info`, `add`, `install` (not
/// `--offline`), `update`, `list`, and `audit` call it once per run.
pub fn refresh(session: &Session) -> Option<AdvisoryList> {
    let outcome = session
        .registry_within(CLI_ADVISORY_TIMEOUT)
        .map(|registry| refresh_advisories_within(registry, session.now, CLI_ADVISORY_TIMEOUT));
    let list = match &outcome {
        Some(AdvisoryRefresh::Fresh(list)) => Some(list.clone()),
        _ => match session.registry.cached_advisories() {
            Ok(list) => list,
            Err(error) => {
                report::warn(format_args!(
                    "could not read the cached security advisories ({error}); plugins under an advisory may run"
                ));
                return None;
            }
        },
    };
    for note in notes(outcome.as_ref(), list.as_ref(), session.now) {
        report::warn(note);
    }
    list
}

/// The warnings about a refresh that ended with `refresh` (`None` when
/// there was none) and the list then in use.
pub fn notes(
    refresh: Option<&AdvisoryRefresh>,
    cached: Option<&AdvisoryList>,
    now: OffsetDateTime,
) -> Vec<String> {
    let mut notes = Vec::new();
    match refresh {
        Some(AdvisoryRefresh::Failed(reason)) => notes.push(format!(
            "could not refresh the security advisories ({reason}); using the cached list"
        )),
        Some(AdvisoryRefresh::TimedOut) => notes.push(format!(
            "the security advisories did not arrive within {} seconds; using the cached list",
            CLI_ADVISORY_TIMEOUT.as_secs()
        )),
        Some(AdvisoryRefresh::Fresh(_)) | None => {}
    }
    match cached {
        None => notes.push(
            "no security advisory list has been fetched yet; plugins under an advisory may run"
                .to_owned(),
        ),
        Some(list) => {
            if let Some(age) = advisories_stale(list, now) {
                notes.push(format!(
                    "the security advisory list is {} days old; plugins under newer advisories may run",
                    age.whole_days()
                ));
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use celesta_registry_client::testing::at;
    use time::Duration;

    use super::*;

    fn now() -> OffsetDateTime {
        at("2026-11-21T00:00:00Z")
    }

    fn list_from(days_ago: i64) -> AdvisoryList {
        AdvisoryList {
            generated: now() - Duration::days(days_ago),
            advisories: Vec::new(),
        }
    }

    #[test]
    fn a_list_eight_days_old_is_stale() {
        let notes = notes(None, Some(&list_from(8)), now());
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("8 days old"), "{notes:?}");
    }

    #[test]
    fn a_list_six_days_old_is_not_stale() {
        assert!(notes(None, Some(&list_from(6)), now()).is_empty());
    }

    #[test]
    fn no_list_is_said() {
        let notes = notes(None, None, now());
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("no security advisory list"), "{notes:?}");
    }

    #[test]
    fn a_timeout_is_said() {
        let notes = notes(Some(&AdvisoryRefresh::TimedOut), Some(&list_from(1)), now());
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("3 seconds"), "{notes:?}");
    }

    #[test]
    fn a_failure_names_its_reason() {
        let failed = AdvisoryRefresh::Failed("offline".to_owned());
        let notes = notes(Some(&failed), Some(&list_from(1)), now());
        assert_eq!(
            notes,
            ["could not refresh the security advisories (offline); using the cached list"]
        );
    }
}
