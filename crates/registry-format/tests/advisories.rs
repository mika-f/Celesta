use std::path::Path;

use celesta_registry_format::{Advisory, AdvisoryList, FormatError};
use semver::Version;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn fixture() -> AdvisoryList {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/valid/advisories.json");
    AdvisoryList::parse(&std::fs::read(path).unwrap()).unwrap()
}

fn entry(id: &str, plugin: &str, versions: &str) -> String {
    format!(
        r#"{{"id":"{id}","plugin":"{plugin}","versions":"{versions}","title":"t","description":"d","fixed":null,"published":"2026-11-20T00:00:00Z"}}"#
    )
}

fn list(generated: &str, entries: &[String]) -> Result<AdvisoryList, FormatError> {
    let json = format!(
        r#"{{"format":1,"generated":"{generated}","advisories":[{}]}}"#,
        entries.join(",")
    );
    AdvisoryList::parse(json.as_bytes())
}

fn one(versions: &str) -> Result<AdvisoryList, FormatError> {
    list(
        "2026-11-20T00:00:00Z",
        &[entry("CELESTA-1", "vrm", versions)],
    )
}

fn blocks(l: &AdvisoryList, version: &str) -> bool {
    l.blocking("vrm", &v(version)).is_some()
}

#[test]
fn fixture_blocks_vulnerable_versions() {
    let l = fixture();
    assert!(blocks(&l, "0.2.2"));
    assert!(!blocks(&l, "0.2.3"));
    assert!(blocks(&l, "0.2.3-rc.1"));
    assert!(l.blocking("voicevox", &v("0.2.2")).is_none());
}

#[test]
fn plugin_id_is_compared_exactly() {
    let l = fixture();
    assert!(l.blocking("VRM", &v("0.2.2")).is_none());
    assert!(l.blocking("vrm2", &v("0.2.2")).is_none());
}

#[test]
fn first_advisory_in_file_order_wins() {
    let l = list(
        "2026-11-20T00:00:00Z",
        &[
            entry("FIRST", "vrm", "<1.0.0"),
            entry("SECOND", "vrm", "<2.0.0"),
        ],
    )
    .unwrap();
    assert_eq!(l.blocking("vrm", &v("0.5.0")).unwrap().id, "FIRST");
    assert_eq!(l.blocking("vrm", &v("1.5.0")).unwrap().id, "SECOND");
}

fn at(s: &str) -> OffsetDateTime {
    OffsetDateTime::parse(s, &Rfc3339).unwrap()
}

#[test]
fn check_newer_rules() {
    let now = at("2026-11-21T00:00:00Z");
    let new = list("2026-11-20T00:00:00Z", &[]).unwrap();
    let old = list("2026-11-19T00:00:00Z", &[]).unwrap();
    assert!(new.check_newer(Some(&old), now).is_ok());
    assert!(new.check_newer(Some(&new), now).is_ok());
    assert!(new.check_newer(None, now).is_ok());
    assert!(matches!(
        old.check_newer(Some(&new), now),
        Err(FormatError::Rollback)
    ));
}

#[test]
fn future_dated_list_is_refused() {
    // A list may be dated up to 10 minutes ahead of the client's clock.
    let list = list("2026-11-20T00:10:00Z", &[]).unwrap();
    assert!(list.check_newer(None, at("2026-11-20T00:00:00Z")).is_ok());
    for now in ["2026-11-19T23:59:59Z", "2026-11-19T00:00:00Z"] {
        assert!(
            matches!(
                list.check_newer(None, at(now)),
                Err(FormatError::FutureDated)
            ),
            "{now}"
        );
    }
    // A future-dated list is refused even when it is newer than the cache,
    // so it cannot be cached and then block every later list.
    let cached = self::list("2026-11-01T00:00:00Z", &[]).unwrap();
    assert!(matches!(
        list.check_newer(Some(&cached), at("2026-11-19T00:00:00Z")),
        Err(FormatError::FutureDated)
    ));
}

#[test]
fn invalid_range_reports_the_text_as_written() {
    for r in ["0.2.3", "<0.2.3+b", "<0.2.3-01", ">=0.2"] {
        match one(r) {
            Err(FormatError::InvalidRange(t)) => assert_eq!(t, r),
            other => panic!("{r}: expected InvalidRange, got {other:?}"),
        }
    }
}

#[test]
fn build_metadata_in_a_range_is_refused() {
    assert!(matches!(
        one("<0.2.3+build.1"),
        Err(FormatError::InvalidRange(_))
    ));
}

#[test]
fn every_deserialize_path_validates() {
    // Deserializing without `parse` still runs the range and id checks.
    let bad_range = entry("X", "vrm", "^0.2");
    assert!(serde_json::from_str::<Advisory>(&bad_range).is_err());
    let list = format!(r#"{{"generated":"2026-11-20T00:00:00Z","advisories":[{bad_range}]}}"#);
    assert!(serde_json::from_str::<AdvisoryList>(&list).is_err());
    let bad_id = entry("X", "-vrm", "<1.0.0");
    assert!(serde_json::from_str::<Advisory>(&bad_id).is_err());
    let good = entry("X", "vrm", "<1.0.0");
    serde_json::from_str::<Advisory>(&good).unwrap();
}

#[test]
fn plugin_uses_the_host_id_rule() {
    for id in ["a.b.c", "acme.fancy-title", "my.plugin.v2", "a..b"] {
        let l = list("2026-11-20T00:00:00Z", &[entry("X", id, "<1.0.0")])
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(l.blocking(id, &v("0.1.0")).is_some());
    }
    for id in ["Vrm", "-a", "", ".a", "a/b", "a b"] {
        match list("2026-11-20T00:00:00Z", &[entry("X", id, "<1.0.0")]) {
            Err(FormatError::InvalidId(got)) => assert_eq!(got, id),
            other => panic!("{id:?}: expected InvalidId, got {other:?}"),
        }
    }
}

#[test]
fn lowest_bound_blocks_every_version() {
    let l = one(">=0.0.0-0").unwrap();
    for version in ["0.0.0-0", "0.0.0-alpha", "0.0.0", "0.2.3-rc.1", "999.0.0"] {
        assert!(blocks(&l, version), "{version}");
    }
}

#[test]
fn caret_range_is_refused() {
    match one("^0.2") {
        Err(FormatError::InvalidRange(r)) => assert_eq!(r, "^0.2"),
        other => panic!("expected InvalidRange, got {other:?}"),
    }
}

#[test]
fn other_operators_are_refused() {
    for r in [
        "~0.2.3",
        "*",
        "0.2.3",
        "0.2.*",
        ">=0.2",
        "<1",
        ">=0.1.0, <0.3",
    ] {
        assert!(
            matches!(one(r), Err(FormatError::InvalidRange(_))),
            "{r} should be refused"
        );
    }
}

#[test]
fn bounded_range_blocks_inside_only() {
    let l = one(">=0.1.0, <0.2.3").unwrap();
    assert!(blocks(&l, "0.1.0"));
    assert!(blocks(&l, "0.2.2"));
    assert!(!blocks(&l, "0.0.9"));
    assert!(!blocks(&l, "0.2.3"));
}

#[test]
fn build_metadata_is_ignored() {
    let l = one("<=0.2.3").unwrap();
    assert!(blocks(&l, "0.2.3+build.5"));
    assert!(!blocks(&l, "0.2.4+build.5"));
}

#[test]
fn equals_matches_exactly_one_version() {
    let l = one("=0.2.2").unwrap();
    assert!(blocks(&l, "0.2.2"));
    assert!(!blocks(&l, "0.2.2-rc.1"));
    assert!(!blocks(&l, "0.2.3"));
}

#[test]
fn prerelease_comparators_use_total_ordering() {
    let l = one(">0.2.3-rc.1").unwrap();
    assert!(blocks(&l, "0.2.3-rc.2"));
    assert!(blocks(&l, "0.2.3"));
    assert!(!blocks(&l, "0.2.3-rc.1"));
    assert!(!blocks(&l, "0.2.3-alpha"));
}

#[test]
fn boundary_cases() {
    assert!(!blocks(&one(">=0.2.3").unwrap(), "0.2.3-rc.1"));
    assert!(blocks(&one("=0.2.2").unwrap(), "0.2.2+build.1"));
}
