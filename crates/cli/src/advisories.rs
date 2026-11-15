//! The security advisory list one `celesta` run uses: refreshed once,
//! within [`CLI_ADVISORY_TIMEOUT`], else the cached list.

use celesta_plugin_host::advisories_stale;
use celesta_registry_client::{AdvisoryRefresh, CLI_ADVISORY_TIMEOUT, refresh_advisories_within};
use celesta_registry_format::AdvisoryList;
use time::OffsetDateTime;

use crate::report;
use crate::session::Session;

/// Refreshes the advisory list, then returns
/// the newest verified list, warning when it could not be refreshed, is
/// missing, or is old. `search`, `info`, `add`, `install` (not
/// `--offline`), `update`, `list`, and `audit` call it once per run.
pub fn refresh(session: &Session) -> Option<AdvisoryList> {
    let outcome = refresh_advisories_within(
        session.registry_within(CLI_ADVISORY_TIMEOUT),
        session.now,
        CLI_ADVISORY_TIMEOUT,
    );
    let (list, notes) = match &outcome {
        AdvisoryRefresh::Fresh(list) => {
            let list = list.clone();
            let notes = notes(Some(&outcome), Some(&list), session.now);
            (Some(list), notes)
        }
        _ => match session.registry.cached_advisories() {
            Ok(list) => {
                let notes = notes(Some(&outcome), list.as_ref(), session.now);
                (list, notes)
            }
            Err(error) => (
                None,
                unreadable_cache_notes(Some(&outcome), &error.to_string()),
            ),
        },
    };
    for note in notes {
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
    let list_note = match cached {
        None => Some(
            "no security advisory list has been fetched yet; plugins under an advisory may run"
                .to_owned(),
        ),
        Some(list) => advisories_stale(list, now).map(|age| {
            format!(
                "the security advisory list is {} days old; plugins under newer advisories may run",
                age.whole_days()
            )
        }),
    };
    refresh_note(refresh).into_iter().chain(list_note).collect()
}

/// The warnings when the refresh ended with `refresh` and the cached list
/// could not be read, for `error`.
fn unreadable_cache_notes(refresh: Option<&AdvisoryRefresh>, error: &str) -> Vec<String> {
    let cache_note = format!(
        "could not read the cached security advisories ({error}); plugins under an advisory may run"
    );
    refresh_note(refresh)
        .into_iter()
        .chain([cache_note])
        .collect()
}

/// What to say about a refresh that ended with `refresh`, when it did not
/// bring a fresh list.
fn refresh_note(refresh: Option<&AdvisoryRefresh>) -> Option<String> {
    match refresh? {
        AdvisoryRefresh::Failed(reason) => Some(format!(
            "could not refresh the security advisories ({reason}); using the cached list"
        )),
        AdvisoryRefresh::TimedOut => Some(format!(
            "the security advisories did not arrive within {} seconds; using the cached list",
            CLI_ADVISORY_TIMEOUT.as_secs()
        )),
        AdvisoryRefresh::Fresh(_) => None,
    }
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
    fn an_unreadable_cache_keeps_the_refresh_note() {
        let notes = unreadable_cache_notes(Some(&AdvisoryRefresh::TimedOut), "cache is broken");
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("3 seconds"), "{notes:?}");
        assert!(notes[1].contains("cache is broken"), "{notes:?}");
        assert_eq!(unreadable_cache_notes(None, "cache is broken").len(), 1);
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
