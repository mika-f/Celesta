use std::path::Path;

use celesta_registry_format::{AdvisoryList, FormatError};
use semver::Version;

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

#[test]
fn check_newer_rules() {
    let new = list("2026-11-20T00:00:00Z", &[]).unwrap();
    let old = list("2026-11-19T00:00:00Z", &[]).unwrap();
    assert!(new.check_newer(Some(&old)).is_ok());
    assert!(new.check_newer(Some(&new)).is_ok());
    assert!(new.check_newer(None).is_ok());
    assert!(matches!(
        old.check_newer(Some(&new)),
        Err(FormatError::Rollback)
    ));
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
