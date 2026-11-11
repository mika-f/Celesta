//! `--inspect`'s command line, which needs no React runtime; the
//! inspections themselves are in the runtime target (`tests/runtime/inspect.rs`).

use std::process::Command;

#[test]
fn inspect_flags_do_not_mix_with_export_flags() {
    let exporter = env!("CARGO_BIN_EXE_celesta-exporter");
    let run = |args: &[&str]| Command::new(exporter).args(args).output().unwrap();
    let missing_output = run(&["--react", "scene.tsx"]);
    assert!(!missing_output.status.success());
    assert!(String::from_utf8_lossy(&missing_output.stderr).contains("<OUTPUT>"));
    let without_react = run(&["--inspect", "scene.tsx"]);
    assert!(!without_react.status.success());
    for flags in [["--runtime", "cli.js"], ["--node", "node"]] {
        let result = run(&[&["--react", "scene.tsx", "out.mp4"][..], &flags].concat());
        assert!(
            !result.status.success(),
            "accepted {flags:?} without --inspect"
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("require --inspect"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for flags in [
        &["out.mp4"][..],
        &["--project", "project.celesta.json"],
        &["--overwrite"],
        &["--from", "0"],
        &["--frame", "0"],
        &["--every", "1"],
        &["--contact-sheet"],
        &["--driver", "auto"],
    ] {
        let result = run(&[&["--inspect", "--react", "scene.tsx"][..], flags].concat());
        assert!(
            !result.status.success(),
            "accepted {flags:?} with --inspect"
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("cannot be used"),
            "{flags:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    // --json reports usage errors on stdout.
    let json = run(&["--inspect", "--react", "scene.tsx", "--json"]);
    assert!(!json.status.success());
    assert!(String::from_utf8_lossy(&json.stdout).contains("cannot be used"));
}
