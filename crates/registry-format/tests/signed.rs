use std::fs;
use std::path::{Path, PathBuf};

use celesta_registry_format::{AdvisoryList, Config, Index, SearchList};
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn signed(rel: &str) -> Vec<u8> {
    fs::read(root().join("signed").join(rel)).unwrap()
}

#[test]
fn signed_tree_parses() {
    let config = Config::parse(&signed("config.json")).unwrap();
    let ids: Vec<_> = config.keys.iter().map(|k| k.keyid.as_str()).collect();
    assert_eq!(ids, ["2026-11", "expired"]);
    Index::parse(&signed("index/vrm.json"), "vrm").unwrap();
    SearchList::parse(&signed("search.json")).unwrap();
    AdvisoryList::parse(&signed("advisories.json")).unwrap();
    AdvisoryList::parse(&signed("expired/advisories.json")).unwrap();
    assert_eq!(signed("root.pub").len(), 45);
}

#[test]
fn signed_copies_match_valid_fixtures() {
    let pairs = [
        ("index/vrm.json", "index-vrm.json"),
        ("search.json", "search.json"),
        ("advisories.json", "advisories.json"),
        ("expired/advisories.json", "advisories.json"),
    ];
    for (rel, source) in pairs {
        let valid = fs::read(root().join("valid").join(source)).unwrap();
        assert_eq!(signed(rel), valid, "{rel}");
    }
}

#[test]
fn every_sig_file_has_keyid_and_signature() {
    for rel in [
        "config.json.sig",
        "index/vrm.json.sig",
        "search.json.sig",
        "advisories.json.sig",
        "expired/advisories.json.sig",
    ] {
        let v: Value = serde_json::from_slice(&signed(rel)).unwrap();
        assert!(
            v["keyid"].is_string() && v["signature"].is_string(),
            "{rel}"
        );
    }
}

#[test]
fn generated_config_matches_schema() {
    let schema: Value =
        serde_json::from_slice(&fs::read(root().join("../schema/v1/config.schema.json")).unwrap())
            .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let config: Value = serde_json::from_slice(&signed("config.json")).unwrap();
    assert!(validator.is_valid(&config));
}
