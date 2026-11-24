use std::process::Command;

use serde_json::Value;

#[test]
fn unified_info_and_export_info_share_the_exporters_report() {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/editor-demo.celesta.json"
    );
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_celesta"))
            .args(args)
            .args([source, "--json"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let mut report: Value = serde_json::from_slice(&output.stdout).unwrap();
        report.as_object_mut().unwrap().remove("stats");
        report
    };
    let info = run(&["info"]);
    assert_eq!(info["status"], "ok");
    assert!(info["composition"]["width"].as_u64().unwrap() > 0);
    assert_eq!(info, run(&["export", "--info"]));
}

#[test]
fn unified_export_routes_component_listing_and_json_usage_errors() {
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_celesta"))
            .args(args)
            .output()
            .unwrap()
    };
    let output = run(&["export", "--list-components", "--json"]);
    assert!(output.status.success(), "{output:?}");
    let components: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        components["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component["id"] == "builtin/png")
    );

    let output = run(&["export", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["error"]["code"], "usage");
    assert!(output.stderr.is_empty());
}

#[cfg(feature = "runtime-tests")]
#[test]
fn unified_and_standalone_export_write_identical_react_stills() {
    use std::{fs, path::Path};

    let cli = Path::new(env!("CARGO_BIN_EXE_celesta"));
    let exporter = cli.with_file_name(format!("celesta-exporter{}", std::env::consts::EXE_SUFFIX));
    if !exporter.is_file() {
        eprintln!(
            "skipping exporter parity test: {} is missing; run a workspace build like CI (cargo build --workspace --all-targets --features runtime-tests --locked) first",
            exporter.display()
        );
        return;
    }
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/cli/examples/title.tsx"
    );
    let scratch = tempfile::tempdir().unwrap();
    let run = |binary: &Path, args: &[&str], name: &str| {
        let path = scratch.path().join(name);
        let output = Command::new(binary)
            .args(args)
            .args(["--react", "--frame", "0", "--json", source])
            .arg(&path)
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "could not run {}: {error}; build both CLI packages",
                    binary.display()
                )
            });
        let report: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("invalid export report: {error}; {output:?}"));
        (output, report, path)
    };

    let (unified, report, new) = run(cli, &["export"], "unified.png");
    if report["error"]["code"] == "render"
        && report["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("adapter"))
    {
        assert_ne!(
            std::env::var("CELESTA_REQUIRE_GPU").as_deref(),
            Ok("1"),
            "{report}"
        );
        eprintln!("skipping live export test: no GPU adapter is available: {report}");
        return;
    }
    assert!(unified.status.success(), "{report}");
    let (standalone, report, old) = run(&exporter, &[], "standalone.png");
    assert!(standalone.status.success(), "{report}");
    assert!(
        fs::read(new).unwrap() == fs::read(old).unwrap(),
        "the two entry points rendered different PNGs"
    );
}
