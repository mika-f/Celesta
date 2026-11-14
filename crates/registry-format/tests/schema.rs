use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

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
    assert!(files.len() >= 9, "expected the invalid fixtures");
    for file in files {
        let validator = jsonschema::validator_for(&schema_for(&file)).unwrap();
        assert!(
            !validator.is_valid(&read(&file)),
            "{file:?} should be refused"
        );
    }
}
