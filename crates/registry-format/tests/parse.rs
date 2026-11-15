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
    let host = Target::host();
    assert_eq!(Target::parse(host.as_str()).unwrap(), host);
}

// The fixture's 0.2.3 builds for exactly these three targets.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
))]
#[test]
fn fixture_builds_for_the_host() {
    let index = Index::parse(&fixture("index-vrm.json"), "vrm").unwrap();
    let host = Target::host();
    assert!(index.versions[1].targets.contains_key(&host), "{host:?}");
}

#[test]
fn any_os_arch_target_is_accepted() {
    // A new OS must not make old clients refuse the whole index.
    for t in [
        "freebsd-x86_64",
        "beos-x86",
        "android-aarch64",
        "linux-riscv64",
        "any",
    ] {
        assert_eq!(Target::parse(t).unwrap().as_str(), t);
    }
    let bytes = tweak("index-vrm.json", |v| {
        v["versions"][1]["targets"]["freebsd-x86_64"] =
            v["versions"][1]["targets"]["linux-x86_64"].clone();
    });
    let index = Index::parse(&bytes, "vrm").unwrap();
    assert_eq!(index.versions[1].targets.len(), 4);
    for t in [
        "",
        "linux",
        "Linux-x86_64",
        "linux-x86-64",
        "linux-",
        "-x86_64",
        "linux_x86_64",
    ] {
        assert!(
            matches!(Target::parse(t), Err(FormatError::InvalidTarget(_))),
            "{t}"
        );
    }
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
        v["versions"][1]["targets"]["BeOS-x86_64"] =
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

#[test]
fn valid_fixture_carries_unknown_fields() {
    // The fixture itself holds fields no schema names, at the top level and
    // nested, so every implementation proves it ignores them.
    let raw: Value = serde_json::from_slice(&fixture("index-vrm.json")).unwrap();
    assert_eq!(raw["futureField"], 1);
    assert!(raw["versions"][1]["futureField"]["nested"].is_array());
    Index::parse(&fixture("index-vrm.json"), "vrm").unwrap();
}

#[test]
fn config_and_index_need_generated() {
    for (name, id) in [("config.json", None), ("index-vrm.json", Some("vrm"))] {
        let bytes = tweak(name, |v| {
            v.as_object_mut().unwrap().remove("generated");
        });
        let err = match id {
            None => Config::parse(&bytes).map(drop),
            Some(id) => Index::parse(&bytes, id).map(drop),
        }
        .unwrap_err();
        assert!(matches!(err, FormatError::Json(_)), "{name}: {err:?}");
    }
}
