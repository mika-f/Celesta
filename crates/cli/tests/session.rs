mod common;

use std::process::Command;

use celesta_registry_client::testing::TestRegistry;
use common::Home;

#[test]
fn json_usage_errors_keep_exit_two() {
    let home = Home::new();
    let output = home
        .command()
        .args([
            "plugin",
            "install",
            "--json",
            "--locked",
            "--global",
            "acme.codec",
        ])
        .output()
        .unwrap();
    common::assert_exit(&output, 2);
    assert_eq!(stderr(&output), "");
    let report = common::json_stdout(&output);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "invalid_arguments");
}

#[test]
fn unsupported_json_flag_reports_text_on_stderr() {
    let home = Home::new();
    let output = home
        .command()
        .args(["plugin", "--project", "audit", "search", "chart", "--json"])
        .output()
        .unwrap();
    common::assert_exit(&output, 2);
    assert_eq!(common::stdout(&output), "");
    assert!(stderr(&output).starts_with("celesta: error: unexpected argument '--json'"));
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn an_unreachable_registry_is_a_failure() {
    let home = Home::new();
    let output = home
        .command()
        .args(["plugin", "search", "x"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let stderr = stderr(&output);
    assert!(
        stderr.contains("celesta: error: could not fetch ") && stderr.contains("/config.json"),
        "{stderr}"
    );
}

#[test]
fn half_a_registry_is_a_usage_error() {
    let home = Home::new();
    let output = home
        .command()
        .env("CELESTA_REGISTRY", "https://registry.test")
        .env_remove("CELESTA_REGISTRY_ROOT_KEY")
        .args(["plugin", "search", "x"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("CELESTA_REGISTRY_ROOT_KEY"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_bad_root_key_is_a_usage_error() {
    let home = Home::new();
    let output = home
        .command()
        .env("CELESTA_REGISTRY", "https://registry.test")
        .env("CELESTA_REGISTRY_ROOT_KEY", "not base64")
        .args(["plugin", "search", "x"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("CELESTA_REGISTRY_ROOT_KEY"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn help_lists_every_plugin_command() {
    // No environment at all: help needs no user folder or registry.
    let output = Command::new(env!("CARGO_BIN_EXE_celesta"))
        .env_clear()
        .args(["plugin", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8(output.stdout).unwrap();
    for command in [
        "search", "info", "add", "remove", "install", "update", "list", "audit", "gc",
    ] {
        let listed = |line: &str| {
            line.trim_start()
                .strip_prefix(command)
                .is_some_and(|rest| rest.starts_with(char::is_whitespace))
        };
        assert!(
            stdout.lines().any(listed),
            "{command} missing from:\n{stdout}"
        );
    }
}

#[test]
fn global_conflicts_with_locked_and_offline() {
    let home = Home::new();
    for flag in ["--locked", "--offline"] {
        let output = home
            .command()
            .args(["plugin", "install", flag, "--global", "acme.charts"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
        let stderr = stderr(&output);
        assert!(stderr.starts_with("celesta: error: "), "{stderr}");
        assert!(
            stderr.contains(flag) && stderr.contains("--global"),
            "{stderr}"
        );
    }
}

#[test]
fn a_file_registry_is_reachable() {
    let home = Home::new();
    let registry = TestRegistry::file();
    registry.search(serde_json::json!([]));
    let output = home
        .with_registry(&registry)
        .args(["plugin", "search", "x"])
        .output()
        .unwrap();
    let stderr = stderr(&output);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("future"), "{stderr}");
}

#[test]
fn a_missing_subcommand_shows_help_as_a_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_celesta"))
        .env_clear()
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let stderr = stderr(&output);
    assert!(stderr.contains("Usage: celesta"), "{stderr}");
    assert!(!stderr.contains("celesta: error: Manages"), "{stderr}");
}
