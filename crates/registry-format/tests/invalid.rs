//! Every file in `fixtures/invalid/` is refused by the Rust parser that reads
//! that kind of file, not only by its schema.

use std::fs;
use std::path::{Path, PathBuf};

use celesta_registry_format::{
    AdvisoryList, Config, FormatError, Index, Keyring, RootKey, SearchList,
};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn signed(rel: &str) -> Vec<u8> {
    fs::read(fixtures().join("signed").join(rel)).unwrap()
}

fn keyring() -> Keyring {
    let root = RootKey::from_base64(&String::from_utf8(signed("root.pub")).unwrap()).unwrap();
    let now = OffsetDateTime::parse("2026-11-14T00:00:00Z", &Rfc3339).unwrap();
    Keyring::from_config(&Config::parse(&signed("config.json")).unwrap(), &root, now).unwrap()
}

/// Feeds `bytes` to the parser the file-name prefix names.
fn parse(prefix: &str, bytes: &[u8]) -> Result<(), FormatError> {
    match prefix {
        "config" => Config::parse(bytes).map(drop),
        "index" => {
            // The client asks for the id the file claims, so only the file's
            // own content can make it fail.
            let id = serde_json::from_slice::<Value>(bytes)
                .ok()
                .and_then(|v| v["id"].as_str().map(str::to_owned))
                .unwrap_or_default();
            Index::parse(bytes, &id).map(drop)
        }
        "search" => SearchList::parse(bytes).map(drop),
        "advisories" => AdvisoryList::parse(bytes).map(drop),
        "signature" => keyring().verify("/index/vrm.json", &signed("index/vrm.json"), bytes),
        other => panic!("no parser for fixture prefix {other}"),
    }
}

#[test]
fn every_invalid_fixture_fails_its_rust_parser() {
    let mut files: Vec<PathBuf> = fs::read_dir(fixtures().join("invalid"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(files.len() >= 10, "expected the invalid fixtures");
    for file in files {
        let stem = file.file_stem().unwrap().to_str().unwrap();
        let prefix = stem.split('-').next().unwrap();
        let bytes = fs::read(&file).unwrap();
        match parse(prefix, &bytes) {
            Err(e) => eprintln!("{stem}: {e}"),
            Ok(()) => panic!("{stem} was accepted by the Rust parser"),
        }
    }
}
