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
