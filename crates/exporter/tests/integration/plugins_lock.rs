//! The project's `celesta.lock`: an export whose locked plugins cannot run
//! fails before the React runtime starts, so these need no Node build.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use celesta_plugin_host::{CELESTA_VERSION, LOCK_FILE, PLUGIN_PATH_ENV, PROJECT_CONFIG};
use serde_json::{Value, json};

const HASH: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// A React project declaring `acme.charts`, locked by Celesta `celesta` at
/// 1.0.0 for the Celesta versions `range`, beside an empty user home.
struct Project {
    home: tempfile::TempDir,
    root: tempfile::TempDir,
}

impl Project {
    fn new(celesta: &str, range: &str) -> Self {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let config = json!({ "plugins": { "dependencies": { "acme.charts": "1" } } });
        std::fs::write(root.path().join(PROJECT_CONFIG), config.to_string()).unwrap();
        let lock = json!({
            "format": 1,
            "celesta": celesta,
            "plugins": {
                "acme.charts": {
                    "celesta": range,
                    "registry": "https://registry.example.test",
                    "targets": { "any": { "sha256": HASH, "treeHash": HASH } },
                    "version": "1.0.0",
                },
            },
            "targets": [],
        });
        std::fs::write(root.path().join(LOCK_FILE), format!("{lock:#}\n")).unwrap();
        std::fs::write(
            root.path().join("scene.tsx"),
            "export default function Root() { return null; }\n",
        )
        .unwrap();
        Self { home, root }
    }

    /// Exports `scene.tsx` with `--json` and returns the run and its report.
    fn export(&self) -> (Output, Value) {
        let home = self.home.path();
        let output = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .env_remove(PLUGIN_PATH_ENV)
            .env_remove("CELESTA_REGISTRY")
            .env_remove("CELESTA_REGISTRY_ROOT_KEY")
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home)
            .env("XDG_DATA_HOME", home)
            .env("APPDATA", home)
            .arg("--json")
            .arg("--react")
            .arg(self.root.path().join("scene.tsx"))
            .arg(self.root.path().join("out.mp4"))
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout.clone()).unwrap();
        let report = serde_json::from_str(stdout.lines().last().unwrap_or_default())
            .unwrap_or_else(|error| panic!("{error}: {stdout}"));
        (output, report)
    }
}

/// The error message of a failed report whose code is `plugins_unsatisfied`.
fn unsatisfied(output: &Output, report: &Value) -> String {
    assert!(!output.status.success(), "{report}");
    assert_eq!(report["status"], "error", "{report}");
    assert_eq!(report["error"]["code"], "plugins_unsatisfied", "{report}");
    report["error"]["message"].as_str().unwrap().to_owned()
}

/// Every file and folder under `root`, sorted, with each file's bytes.
fn tree(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path.clone());
                found.push((path, None));
            } else {
                let bytes = std::fs::read(&path).unwrap();
                found.push((path, Some(bytes)));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn exporter_fails_when_the_lock_is_not_installed() {
    let project = Project::new(CELESTA_VERSION, &format!("={CELESTA_VERSION}"));
    let (output, report) = project.export();
    let message = unsatisfied(&output, &report);
    assert!(message.contains("acme.charts 1.0.0"), "{message}");
    assert!(message.contains("celesta plugin install"), "{message}");
    assert!(!project.root.path().join("out.mp4").exists());
}

#[test]
fn exporter_names_both_celesta_versions() {
    let project = Project::new("0.8.0", ">=0.8.0, <0.9.0");
    let (output, report) = project.export();
    let message = unsatisfied(&output, &report);
    assert!(message.contains("0.8.0"), "{message}");
    assert!(message.contains(CELESTA_VERSION), "{message}");
    assert!(message.contains("celesta plugin update"), "{message}");
}

#[test]
fn exporter_never_writes_the_lock() {
    let project = Project::new(CELESTA_VERSION, &format!("={CELESTA_VERSION}"));
    let before = (tree(project.root.path()), tree(project.home.path()));
    let (output, report) = project.export();
    unsatisfied(&output, &report);
    let after = (tree(project.root.path()), tree(project.home.path()));
    assert_eq!(before, after);
}
