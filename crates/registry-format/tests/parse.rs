use std::fs;
use std::path::Path;

use celesta_registry_format::{
    AdvisoryList, Config, FormatError, Hash, Index, SearchList, Target, is_registry_id,
    publisher_of,
};
use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/valid")
            .join(name),
    )
    .unwrap()
}

fn tweak(name: &str, f: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut v: Value = serde_json::from_slice(&fixture(name)).unwrap();
    f(&mut v);
    serde_json::to_vec(&v).unwrap()
}

#[test]
fn every_valid_fixture_parses() {
    let config = Config::parse(&fixture("config.json")).unwrap();
    assert_eq!(config.keys.len(), 1);
    assert_eq!(config.keys[0].keyid, "2026-11");
    assert_eq!(config.keys[0].public[1], 1);
    assert_eq!(config.keys[0].signature, [0u8; 64]);

    let vrm = Index::parse(&fixture("index-vrm.json"), "vrm").unwrap();
    assert_eq!(vrm.publisher, "celesta");
    assert_eq!(vrm.versions.len(), 2);
    assert!(vrm.versions[0].yanked);
    let art = &vrm.versions[1].targets[&Target::parse("windows-x86_64").unwrap()];
    assert_eq!(art.size, 1062001);
    assert_eq!(art.sha256, Hash([0x33; 32]));
    assert_eq!(art.tree_hash, Hash([0x44; 32]));

    let third = Index::parse(&fixture("index-third-party.json"), "acme.fancy-title").unwrap();
    assert!(third.deprecated.is_some());
    assert!(third.versions[0].targets.contains_key(&Target::ANY));

    let search = SearchList::parse(&fixture("search.json")).unwrap();
    assert_eq!(search.plugins.len(), 3);

    let adv = AdvisoryList::parse(&fixture("advisories.json")).unwrap();
    assert_eq!(adv.advisories.len(), 2);
    assert!(adv.advisories[1].fixed.is_none());
}

#[test]
fn id_mismatch() {
    let err = Index::parse(&fixture("index-vrm.json"), "other").unwrap_err();
    assert!(matches!(err, FormatError::IdMismatch { .. }), "{err:?}");
}

#[test]
fn unsupported_format() {
    let bytes = tweak("index-vrm.json", |v| v["format"] = 2.into());
    assert!(matches!(
        Index::parse(&bytes, "vrm").unwrap_err(),
        FormatError::Format(2)
    ));
    let bytes = tweak("config.json", |v| v["format"] = 2.into());
    assert!(matches!(
        Config::parse(&bytes).unwrap_err(),
        FormatError::Format(2)
    ));
}

#[test]
fn ids() {
    for ok in ["vrm", "natsuneko.toon-shader", "a_b", "0a"] {
        assert!(is_registry_id(ok), "{ok}");
    }
    for bad in ["Vrm", ".a", "a.", "a.b.c", "-a", "", "a b"] {
        assert!(!is_registry_id(bad), "{bad}");
    }
    assert_eq!(publisher_of("natsuneko.toon-shader"), "natsuneko");
    assert_eq!(publisher_of("vrm"), "celesta");
}

#[test]
fn host_target_is_known() {
    let bytes = fixture("index-vrm.json");
    let index = Index::parse(&bytes, "vrm").unwrap();
    let host = Target::host();
    assert!(index.versions[1].targets.contains_key(&host), "{host:?}");
}

#[test]
fn unknown_fields_are_ignored() {
    let bytes = tweak("index-vrm.json", |v| {
        v["extra"] = 1.into();
        v["versions"][0]["extra"] = "x".into();
        v["versions"][0]["targets"]["macos-aarch64"]["extra"] = Value::Null;
    });
    Index::parse(&bytes, "vrm").unwrap();
    let bytes = tweak("config.json", |v| v["keys"][0]["extra"] = 1.into());
    Config::parse(&bytes).unwrap();
}

#[test]
fn rejects_invalid_values() {
    let bytes = tweak("index-vrm.json", |v| v["id"] = "A.b.c".into());
    assert!(matches!(
        Index::parse(&bytes, "A.b.c").unwrap_err(),
        FormatError::InvalidId(_)
    ));
    let bytes = tweak("index-vrm.json", |v| {
        v["versions"][1]["targets"]["beos-x86_64"] =
            v["versions"][1]["targets"]["linux-x86_64"].clone();
    });
    assert!(matches!(
        Index::parse(&bytes, "vrm").unwrap_err(),
        FormatError::InvalidTarget(_)
    ));
    let bytes = tweak("index-vrm.json", |v| {
        v["versions"][1]["targets"]["linux-x86_64"]["sha256"] = "sha256:ABC".into();
    });
    assert!(matches!(
        Index::parse(&bytes, "vrm").unwrap_err(),
        FormatError::InvalidHash(_)
    ));
    let upper = format!("sha256:{}", "A".repeat(64));
    let bytes = tweak("index-vrm.json", |v| {
        v["versions"][1]["targets"]["linux-x86_64"]["sha256"] = upper.into();
    });
    assert!(matches!(
        Index::parse(&bytes, "vrm").unwrap_err(),
        FormatError::InvalidHash(_)
    ));
    assert!(matches!(
        Config::parse(b"not json").unwrap_err(),
        FormatError::Json(_)
    ));
}

#[test]
fn hash_and_target_round_trip() {
    let h = Hash([0xab; 32]);
    let s = serde_json::to_string(&h).unwrap();
    assert_eq!(s, format!("\"sha256:{}\"", "ab".repeat(32)));
    assert_eq!(serde_json::from_str::<Hash>(&s).unwrap(), h);
    assert_eq!(serde_json::to_string(&Target::ANY).unwrap(), "\"any\"");
}

#[test]
fn timestamps_must_be_whole_second_utc() {
    let bad = [
        "2027-11-14T00:00:00.000Z",
        "2027-11-14T00:00:00.5Z",
        "2027-11-14T00:00:00+00:00",
        "2027-11-14T09:00:00+09:00",
        "2027-11-14T00:00:00-00:00",
        "2027-11-14t00:00:00z",
        "2027-11-14 00:00:00Z",
        "2027-11-14T00:00Z",
        "2027-02-30T00:00:00Z",
        "2027-11-14T23:59:60Z",
    ];
    for spelled in bad {
        let checks: [(&str, Vec<u8>); 4] = [
            (
                "config expires",
                tweak("config.json", |v| v["keys"][0]["expires"] = spelled.into()),
            ),
            (
                "index published",
                tweak("index-vrm.json", |v| {
                    v["versions"][0]["published"] = spelled.into()
                }),
            ),
            (
                "search generated",
                tweak("search.json", |v| v["generated"] = spelled.into()),
            ),
            (
                "advisory published",
                tweak("advisories.json", |v| {
                    v["advisories"][0]["published"] = spelled.into()
                }),
            ),
        ];
        for (what, bytes) in checks {
            let err = match what {
                "config expires" => Config::parse(&bytes).map(drop),
                "index published" => Index::parse(&bytes, "vrm").map(drop),
                "search generated" => SearchList::parse(&bytes).map(drop),
                _ => AdvisoryList::parse(&bytes).map(drop),
            }
            .unwrap_err();
            assert!(
                matches!(err, FormatError::InvalidTimestamp(ref t) if t == spelled),
                "{what} {spelled}: {err:?}"
            );
        }
    }
    let bytes = tweak("advisories.json", |v| {
        v["generated"] = "2026-11-20T00:00:00.000Z".into()
    });
    assert!(matches!(
        AdvisoryList::parse(&bytes).unwrap_err(),
        FormatError::InvalidTimestamp(_)
    ));
}

#[test]
fn online_key_keeps_the_raw_strings() {
    let config = Config::parse(&fixture("config.json")).unwrap();
    let key = &config.keys[0];
    assert_eq!(
        key.public_b64,
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    );
    assert_eq!(key.expires_text, "2027-11-14T00:00:00Z");
    let round: Value = serde_json::to_value(key).unwrap();
    assert_eq!(round["public"], key.public_b64.as_str());
    assert_eq!(round["expires"], "2027-11-14T00:00:00Z");
}
