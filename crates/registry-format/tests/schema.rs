use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use celesta_registry_format::{AdvisoryList, Config, SearchList, Target};
use semver::{Version, VersionReq};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn json_files(dir: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(root().join(dir))
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// `index-vrm.json` -> `index`; `advisories-caret-range.json` -> `advisories`.
fn schema_for(file: &Path) -> Value {
    let stem = file.file_stem().unwrap().to_str().unwrap();
    let name = stem.split('-').next().unwrap();
    read(&root().join(format!("schema/v1/{name}.schema.json")))
}

#[test]
fn valid_fixtures_match_their_schema() {
    let files = json_files("fixtures/valid");
    assert!(files.len() >= 5, "expected the valid fixtures");
    for file in files {
        let validator = jsonschema::validator_for(&schema_for(&file)).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&read(&file))
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{file:?} is invalid: {errors:?}");
    }
}

#[test]
fn invalid_fixtures_are_refused() {
    let files = json_files("fixtures/invalid");
    assert!(files.len() >= 16, "expected the invalid fixtures");
    for file in files {
        let validator = jsonschema::validator_for(&schema_for(&file)).unwrap();
        assert!(
            !validator.is_valid(&read(&file)),
            "{file:?} should be refused"
        );
    }
}

/// The `pattern` of the schema node at `pointer` in `schema/v1/<schema>.schema.json`.
fn pattern(schema: &str, pointer: &str) -> jsonschema::Validator {
    let v = read(&root().join(format!("schema/v1/{schema}.schema.json")));
    let node = v
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{schema}{pointer}"));
    let p = node["pattern"]
        .as_str()
        .unwrap_or_else(|| panic!("{schema}{pointer} has no pattern"));
    jsonschema::validator_for(&json!({ "type": "string", "pattern": p })).unwrap()
}

/// The schema pattern and the Rust parser agree on every sample: both accept
/// `accepted` and both refuse `refused`. `rust_only` lists spellings the parser
/// accepts and the pattern refuses; the reverse is never allowed, since the
/// registry service validates against the schema and clients run the parser.
fn agree(
    name: &str,
    schema: &jsonschema::Validator,
    rust: impl Fn(&str) -> bool,
    accepted: &[&str],
    refused: &[&str],
    rust_only: &[&str],
) {
    for s in accepted {
        assert!(schema.is_valid(&json!(s)), "{name}: schema refuses {s:?}");
        assert!(rust(s), "{name}: parser refuses {s:?}");
    }
    for s in refused {
        assert!(!schema.is_valid(&json!(s)), "{name}: schema accepts {s:?}");
        assert!(!rust(s), "{name}: parser accepts {s:?}");
    }
    for s in rust_only {
        assert!(!schema.is_valid(&json!(s)), "{name}: schema accepts {s:?}");
        assert!(rust(s), "{name}: parser refuses {s:?}");
    }
}

#[test]
fn version_patterns_agree_with_semver() {
    let accepted = [
        "0.0.0",
        "1.2.3",
        "10.20.30",
        "1.0.0-0",
        "1.0.0-0a",
        "1.0.0-rc.1",
        "1.0.0-a-b.--",
        "1.0.0-alpha.10.beta",
        "1.0.0+01",
        "1.0.0+build.5",
        "1.0.0-rc.1+build.5",
        "9999999999999999999.0.0",
    ];
    let refused = [
        "",
        "1",
        "1.2",
        "01.2.3",
        "1.02.3",
        "1.0.0-01",
        "1.0.0-a.01",
        "1.0.0-",
        "1.0.0-a..b",
        "1.0.0+",
        "1.0.0+a..b",
        "v1.0.0",
        " 1.0.0",
        "99999999999999999999.0.0",
    ];
    let rust = |s: &str| Version::parse(s).is_ok();
    for (schema, pointer) in [
        ("index", "/$defs/version"),
        ("search", "/$defs/version"),
        ("advisories", "/$defs/version"),
    ] {
        agree(
            schema,
            &pattern(schema, pointer),
            rust,
            &accepted,
            &refused,
            &[],
        );
    }
}

#[test]
fn celesta_pattern_agrees_with_version_req() {
    let accepted = [
        ">=0.9.0, <0.10.0",
        " >= 0.9.0 , < 0.10.0 ",
        "0.9",
        "^0.9.1",
        "~0.9",
        "=0.9.0",
        "1",
        "*",
        "x",
        "1.*",
        "1.x.X",
        "1.2.*",
        ">=1.*",
        "<1.2.x",
        ">=0.9.0-rc.1, <0.10.0",
    ];
    let refused = [
        "",
        ">=0.9.0 <0.10.0",
        ">=0.9.0,",
        ",",
        "1.2.3, *",
        "=*",
        "1.*.0",
        "1.0-alpha",
        "01.2.3",
        "<1.0.0-01",
        "<1.0.0-a..b",
        "||1.0.0",
        "1.0.0 - 2.0.0",
    ];
    // Build metadata in a comparator is parsed and dropped by `VersionReq`;
    // the schema refuses it.
    agree(
        "celesta",
        &pattern("index", "/properties/versions/items/properties/celesta"),
        |s| VersionReq::parse(s).is_ok(),
        &accepted,
        &refused,
        &["<0.2.3+b"],
    );
    let many = vec![">=1.0.0"; 32].join(", ");
    let too_many = vec![">=1.0.0"; 33].join(", ");
    agree(
        "celesta",
        &pattern("index", "/properties/versions/items/properties/celesta"),
        |s| VersionReq::parse(s).is_ok(),
        &[&many],
        &[&too_many],
        &[],
    );
}

fn advisory_range_parses(range: &str) -> bool {
    let list = json!({
        "format": 1,
        "generated": "2026-11-20T00:00:00Z",
        "advisories": [{
            "id": "CELESTA-2026-0001", "plugin": "vrm", "versions": range,
            "title": "t", "description": "d", "fixed": null,
            "published": "2026-11-20T00:00:00Z",
        }],
    });
    AdvisoryList::parse(&serde_json::to_vec(&list).unwrap()).is_ok()
}

#[test]
fn range_pattern_agrees_with_the_advisory_parser() {
    let accepted = [
        "<0.2.3",
        ">=0.1.0, <0.2.3",
        " >= 0.1.0 , < 0.2.3 ",
        "=1.0.0",
        "<=1.0.0-rc.1",
        ">0.2.3-0a",
        ">=0.0.0-0",
    ];
    let refused = [
        "",
        "^0.2",
        "~0.2.3",
        "*",
        "0.2.3",
        "0.2.*",
        ">=0.2",
        "<1",
        ">=0.1.0 <0.3.0",
        ">=0.1.0,",
        "<0.2.3-01",
        "<1.0.0-a..b",
        "<01.2.3",
    ];
    agree(
        "versions",
        &pattern(
            "advisories",
            "/properties/advisories/items/properties/versions",
        ),
        advisory_range_parses,
        &accepted,
        &refused,
        &[],
    );
}

#[test]
fn timestamp_pattern_agrees_with_the_parser() {
    let accepted = [
        "2026-11-20T00:00:00Z",
        "1999-12-31T23:59:59Z",
        "2024-02-29T12:30:45Z",
    ];
    let refused = [
        "2026-11-20T00:00:00.000Z",
        "2026-11-20T00:00:00.5Z",
        "2026-11-20T00:00:00+00:00",
        "2026-11-20T09:00:00+09:00",
        "2026-11-20t00:00:00z",
        "2026-11-20 00:00:00Z",
        "2026-11-20T00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-11-32T00:00:00Z",
        "2026-11-20T24:00:00Z",
        "2026-11-20T23:60:00Z",
        "2026-11-20T23:59:60Z",
    ];
    let rust = |s: &str| {
        let list = json!({ "format": 1, "generated": s, "plugins": [] });
        SearchList::parse(&serde_json::to_vec(&list).unwrap()).is_ok()
    };
    for schema in ["config", "index", "search", "advisories"] {
        agree(
            schema,
            &pattern(schema, "/$defs/timestamp"),
            rust,
            &accepted,
            &refused,
            &[],
        );
    }
}

#[test]
fn base64_patterns_agree_with_the_decoder() {
    let decodes = |n: usize| move |s: &str| STANDARD.decode(s).is_ok_and(|b| b.len() == n);
    let key = STANDARD.encode([0xffu8; 32]);
    let sig = STANDARD.encode([0xffu8; 64]);
    let short_key = STANDARD.encode([0u8; 31]);
    let long_key = STANDARD.encode([0u8; 33]);
    let mut key_bad_bits = key.clone();
    key_bad_bits.replace_range(42..43, "/");
    let mut sig_bad_bits = sig.clone();
    sig_bad_bits.replace_range(85..86, "/");
    let key_cases = ["AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", key.as_str()];
    let key_refused = [
        short_key.as_str(),
        long_key.as_str(),
        key_bad_bits.as_str(),
        key.trim_end_matches('='),
        "AAAA",
        "",
    ];
    agree(
        "public",
        &pattern("config", "/$defs/key"),
        decodes(32),
        &key_cases,
        &key_refused,
        &[],
    );
    let sig_refused = [
        sig_bad_bits.as_str(),
        sig.trim_end_matches('='),
        key.as_str(),
    ];
    for (schema, pointer) in [
        ("config", "/$defs/signature"),
        ("signature", "/$defs/signature"),
    ] {
        agree(
            schema,
            &pattern(schema, pointer),
            decodes(64),
            &[&sig],
            &sig_refused,
            &[],
        );
    }
    // The parsers use the same decoder.
    let config = |public: &str| {
        let mut v = read(&root().join("fixtures/valid/config.json"));
        v["keys"][0]["public"] = public.into();
        Config::parse(&serde_json::to_vec(&v).unwrap()).is_ok()
    };
    assert!(config(&key));
    assert!(!config(&short_key));
    assert!(!config(&key_bad_bits));
}

#[test]
fn target_pattern_agrees_with_the_parser() {
    agree(
        "target",
        &pattern("index", "/$defs/target"),
        |s| Target::parse(s).is_ok(),
        &["any", "macos-aarch64", "windows-x86_64", "linux-x86_64"],
        &[
            "",
            "macos",
            "Linux-x86_64",
            "linux-x86-64",
            "linux-",
            "-x86_64",
            "any-",
        ],
        &[],
    );
}
