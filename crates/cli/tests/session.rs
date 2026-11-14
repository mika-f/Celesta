mod common;

use std::process::Command;

use common::Home;

const NO_KEY: &str = "this build of Celesta has no registry key yet, so it cannot reach the plugin registry; set CELESTA_REGISTRY and CELESTA_REGISTRY_ROOT_KEY to use another registry";

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn no_key_fails_with_the_exact_message() {
    let home = Home::new();
    let output = home
        .command()
        .args(["plugin", "search", "x"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains(&format!("celesta: error: {NO_KEY}")),
        "{}",
        stderr(&output)
    );
}

#[test]
fn half_a_registry_is_a_usage_error() {
    let home = Home::new();
    let output = home
        .command()
        .env("CELESTA_REGISTRY", "https://registry.test")
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
        assert!(
            stdout
                .lines()
                .any(|line| line.trim_start().starts_with(command)),
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
    }
}
