mod common;

use std::fs;
use std::path::Path;
use std::process::Output;

use common::{Home, assert_exit, json_stdout, registry, run, stderr, stdout};
use serde_json::{Value, json};

fn config(project: &Path) -> Value {
    serde_json::from_slice(&fs::read(project.join("celesta.config.json")).unwrap()).unwrap()
}

fn lock(project: &Path) -> Value {
    serde_json::from_slice(&fs::read(project.join("celesta.lock")).unwrap()).unwrap()
}

fn locked_version(project: &Path, id: &str) -> Option<String> {
    lock(project)["plugins"][id]["version"]
        .as_str()
        .map(str::to_owned)
}

#[test]
fn add_writes_config_lock_and_store() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");

    let output = run(&home, &registry, &project, &["add", "acme.charts"]);
    assert_exit(&output, 0);
    assert_eq!(
        config(&project)["plugins"]["dependencies"]["acme.charts"],
        "1.2.3"
    );
    assert_eq!(
        locked_version(&project, "acme.charts").as_deref(),
        Some("1.2.3")
    );
    assert!(home.store().join("acme.charts").join("1.2.3").is_dir());
    let stdout = stdout(&output);
    assert!(
        stdout.contains("added acme.charts 1.2.3, locked 1.2.3"),
        "{stdout}"
    );
    assert!(stdout.contains("installed acme.charts 1.2.3"), "{stdout}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn add_works_from_a_subfolder_and_with_project() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    let inner = project.join("src");
    fs::create_dir_all(&inner).unwrap();

    assert_exit(&run(&home, &registry, &inner, &["add", "acme.charts"]), 0);
    assert!(project.join("celesta.lock").is_file());
    assert!(!inner.join("celesta.lock").exists());

    let output = home
        .with_registry(&registry)
        .args(["plugin", "remove", "acme.charts", "--project"])
        .arg(&inner)
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(config(&project), json!({}));
}

#[test]
fn add_with_a_requirement_writes_it() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    registry.publish("acme.charts", "2.0.0", &["any"]);
    let project = home.project("{}\n");

    let output = run(&home, &registry, &project, &["add", "acme.charts@1.2"]);
    assert_exit(&output, 0);
    assert_eq!(
        config(&project)["plugins"]["dependencies"]["acme.charts"],
        "1.2"
    );
    assert_eq!(
        locked_version(&project, "acme.charts").as_deref(),
        Some("1.2.3")
    );
    assert!(
        stdout(&output).contains("added acme.charts 1.2, locked 1.2.3"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_failed_add_leaves_the_project_as_it_was() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let text = "{\n    \"name\": \"demo\",\n    \"render\": { \"fps\": 30 }\n}\n";
    let project = home.project(text);

    for spec in ["acme.missing", "acme.charts@9"] {
        let output = run(&home, &registry, &project, &["add", spec]);
        assert_exit(&output, 1);
        assert!(
            stderr(&output).contains("celesta: error: "),
            "{}",
            stderr(&output)
        );
        assert_eq!(
            fs::read_to_string(project.join("celesta.config.json")).unwrap(),
            text,
            "{spec}"
        );
        assert!(!project.join("celesta.lock").exists(), "{spec}");
    }
}

#[test]
fn a_failed_add_puts_the_previous_lock_back() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    let config_before = fs::read(project.join("celesta.config.json")).unwrap();
    let lock_before = fs::read(project.join("celesta.lock")).unwrap();

    let output = run(&home, &registry, &project, &["add", "acme.charts@9"]);
    assert_exit(&output, 1);
    assert_eq!(
        fs::read(project.join("celesta.config.json")).unwrap(),
        config_before
    );
    assert_eq!(fs::read(project.join("celesta.lock")).unwrap(), lock_before);
}

#[test]
fn add_refuses_a_media_plugin_without_pin() {
    let home = Home::new();
    let registry = registry();
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    let project = home.project("{}\n");

    let output = run(&home, &registry, &project, &["add", "acme.codec"]);
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains(
            "celesta: error: acme.codec is a media plugin; install it for every project with `celesta plugin install --global acme.codec`, or pin it to this project with --pin"
        ),
        "{}",
        stderr(&output)
    );
    assert_eq!(config(&project), json!({}));
    assert!(!project.join("celesta.lock").exists());
}

#[test]
fn add_pin_locks_a_media_plugin() {
    let home = Home::new();
    let registry = registry();
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");

    let output = run(&home, &registry, &project, &["add", "acme.codec", "--pin"]);
    assert_exit(&output, 0);
    assert_eq!(
        locked_version(&project, "acme.codec").as_deref(),
        Some("1.0.0")
    );

    let output = run(&home, &registry, &project, &["add", "acme.charts", "--pin"]);
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("celesta: error: --pin is for media plugins"),
        "{}",
        stderr(&output)
    );
    assert!(locked_version(&project, "acme.charts").is_none());
}

#[test]
fn remove_drops_config_and_lock_entries() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    registry.publish("acme.graphs", "0.1.0", &["any"]);
    let project = home.project("{ \"name\": \"demo\" }\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    assert_exit(&run(&home, &registry, &project, &["add", "acme.graphs"]), 0);

    // Without a registry: remove needs no network.
    let output = home
        .command()
        .current_dir(&project)
        .args(["plugin", "remove", "acme.charts"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(stdout(&output), "removed acme.charts\n");
    assert_eq!(
        config(&project),
        json!({ "name": "demo", "plugins": { "dependencies": { "acme.graphs": "0.1.0" } } })
    );
    assert!(locked_version(&project, "acme.charts").is_none());
    assert_eq!(
        locked_version(&project, "acme.graphs").as_deref(),
        Some("0.1.0")
    );

    let output = home
        .command()
        .current_dir(&project)
        .args(["plugin", "remove", "acme.charts"])
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("celesta: error: acme.charts is not a dependency of this project"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn install_locked_fails_on_an_orphan_entry() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    fs::write(project.join("celesta.config.json"), "{}\n").unwrap();
    let lock_before = fs::read(project.join("celesta.lock")).unwrap();

    let output = run(&home, &registry, &project, &["install", "--locked"]);
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("celesta.lock does not match celesta.config.json for acme.charts"),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read(project.join("celesta.lock")).unwrap(), lock_before);
}

#[test]
fn install_locked_refuses_a_blocked_held_version() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    registry.advisories(json!([{
        "id": "CELESTA-2026-0009",
        "plugin": "acme.charts",
        "versions": "<1.2.4",
        "title": "Bad",
        "description": "Does bad things.",
        "fixed": "1.2.4",
        "published": "2026-11-01T00:00:00Z",
    }]));

    // The store holds the locked version, which the advisory now blocks.
    for args in [&["install", "--locked"][..], &["install"]] {
        let output = run(&home, &registry, &project, args);
        assert_exit(&output, 1);
        assert!(
            stderr(&output).contains(
                "celesta: error: acme.charts 1.2.3 is blocked by advisory CELESTA-2026-0009; run `celesta plugin update acme.charts`"
            ),
            "{}",
            stderr(&output)
        );
        assert!(
            !stdout(&output).contains("is satisfied"),
            "{}",
            stdout(&output)
        );
    }
}

#[test]
fn install_offline_fails_for_a_missing_version() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);

    // Another user, whose store is empty.
    let other = Home::new();
    let output = run(&other, &registry, &project, &["install", "--offline"]);
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("acme.charts 1.2.3"),
        "{}",
        stderr(&output)
    );
    assert!(!other.store().join("acme.charts").exists());

    let output = run(
        &other,
        &registry,
        &project,
        &["install", "--offline", "--json"],
    );
    assert_exit(&output, 1);
    assert_eq!(stderr(&output), "");
    let report = json_stdout(&output);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "plugin_missing");
    assert_eq!(
        report["plugins"],
        json!([
            { "scope": "project", "id": "acme.charts", "version": "1.2.3", "status": "missing" }
        ])
    );
}

#[test]
fn install_without_the_registry_uses_the_store() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);

    let install = |home: &Home| {
        home.command()
            .current_dir(&project)
            .args(["plugin", "install"])
            .output()
            .unwrap()
    };
    let output = install(&home);
    assert_exit(&output, 0);
    assert!(
        stdout(&output).contains("celesta.lock is satisfied"),
        "{}",
        stdout(&output)
    );

    // A store without the locked version needs the registry.
    let output = install(&Home::new());
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("celesta: error: could not fetch "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn install_without_a_lock_locks() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project(r#"{ "plugins": { "dependencies": { "acme.charts": "1" } } }"#);

    let output = run(&home, &registry, &project, &["install"]);
    assert_exit(&output, 0);
    assert_eq!(
        locked_version(&project, "acme.charts").as_deref(),
        Some("1.2.3")
    );
    let stdout = stdout(&output);
    assert!(stdout.contains("installed acme.charts 1.2.3"), "{stdout}");
    assert!(stdout.contains("wrote celesta.lock"), "{stdout}");

    // The lock now checks out, so a second run installs nothing.
    let output = run(&home, &registry, &project, &["install"]);
    assert_exit(&output, 0);
    assert_eq!(trimmed_stdout(&output), "celesta.lock is satisfied");
}

/// Stdout without its trailing newline.
fn trimmed_stdout(output: &Output) -> String {
    stdout(output).trim_end().to_owned()
}

#[test]
fn install_downloads_what_the_lock_names() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    // A newer version does not move the lock.
    registry.publish("acme.charts", "1.3.0", &["any"]);
    let lock_before = fs::read(project.join("celesta.lock")).unwrap();

    let other = Home::new();
    let output = run(&other, &registry, &project, &["install"]);
    assert_exit(&output, 0);
    assert_eq!(
        trimmed_stdout(&output),
        "installed acme.charts 1.2.3\ncelesta.lock is satisfied"
    );
    assert!(other.store().join("acme.charts").join("1.2.3").is_dir());
    assert_eq!(fs::read(project.join("celesta.lock")).unwrap(), lock_before);
}

#[test]
fn update_moves_to_the_newest_match() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    registry.publish("acme.charts", "1.3.0", &["any"]);
    registry.publish("acme.charts", "2.0.0", &["any"]);

    let output = run(&home, &registry, &project, &["update"]);
    assert_exit(&output, 0);
    assert!(
        stdout(&output).contains("acme.charts 1.2.3 -> 1.3.0"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        locked_version(&project, "acme.charts").as_deref(),
        Some("1.3.0")
    );

    let output = run(&home, &registry, &project, &["update", "acme.charts"]);
    assert_exit(&output, 0);
    assert_eq!(trimmed_stdout(&output), "celesta.lock is up to date");

    let output = run(&home, &registry, &project, &["update", "acme.graphs"]);
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("celesta: error: acme.graphs is not a dependency of this project"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn update_outside_a_project_does_nothing() {
    let home = Home::new();
    let registry = registry();
    let folder = home.path().join("loose");
    fs::create_dir_all(&folder).unwrap();

    let output = run(&home, &registry, &folder, &["update"]);
    assert_exit(&output, 0);
    assert_eq!(
        trimmed_stdout(&output),
        "this project has no registry plugins"
    );
    assert_eq!(stderr(&output), "");
    assert!(!folder.join("celesta.lock").exists());
    assert!(!home.store().join(".projects.json").exists());
}

#[test]
fn yanked_locked_version_is_warned_about() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    registry.edit("acme.charts", "1.2.3", |entry| {
        entry["yanked"] = json!(true)
    });

    // Another user installs the locked version, yanked or not.
    let other = Home::new();
    let output = run(&other, &registry, &project, &["install"]);
    assert_exit(&output, 0);
    assert!(
        stderr(&output)
            .contains("celesta: warning: acme.charts 1.2.3 is locked and has been yanked"),
        "{}",
        stderr(&output)
    );
    assert!(other.store().join("acme.charts").join("1.2.3").is_dir());
}

#[test]
fn deprecation_names_the_replacement() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    registry.edit("acme.charts", "1.2.3", |entry| {
        entry["deprecated"] = json!({ "message": "no longer kept", "replacement": "acme.graphs" });
    });
    let project = home.project("{}\n");

    let output = run(&home, &registry, &project, &["add", "acme.charts"]);
    assert_exit(&output, 0);
    let expected = "celesta: warning: acme.charts 1.2.3 is deprecated: no longer kept (use acme.graphs instead)";
    assert!(stderr(&output).contains(expected), "{}", stderr(&output));

    let other = Home::new();
    let output = run(&other, &registry, &project, &["install"]);
    assert_exit(&output, 0);
    assert!(stderr(&output).contains(expected), "{}", stderr(&output));
}

#[test]
fn install_without_any_plugins_does_nothing() {
    let home = Home::new();
    let registry = registry();
    let project = home.project("{ \"name\": \"demo\" }\n");

    // With a registry and without one to reach.
    let with_registry = run(&home, &registry, &project, &["install"]);
    let without_registry = home
        .command()
        .current_dir(&project)
        .args(["plugin", "install"])
        .output()
        .unwrap();
    for output in [with_registry, without_registry] {
        assert_exit(&output, 0);
        assert_eq!(
            trimmed_stdout(&output),
            "this project has no registry plugins"
        );
        assert_eq!(stderr(&output), "");
    }
    assert!(!project.join("celesta.lock").exists());
}

#[test]
fn install_json_reports_the_lock_and_each_plugin() {
    let home = Home::new();
    let registry = registry();
    let other_target = if cfg!(windows) {
        "linux-x86_64"
    } else {
        "windows-x86_64"
    };
    registry.publish("acme.charts", "1.2.3", &["any"]);
    registry.publish_with_roles("acme.codec", "1.0.0", &[other_target], &["media"]);
    let project = home.project(
        &json!({
            "plugins": {
                "dependencies": { "acme.charts": "1", "acme.codec": "1" },
                "targets": [other_target],
            }
        })
        .to_string(),
    );
    let output = run(&home, &registry, &project, &["install", "--json"]);
    assert_exit(&output, 0);
    let report = json_stdout(&output);
    assert_eq!(report["command"], "install");
    assert_eq!(report["scope"], "project");
    assert_eq!(report["lockfile"], "written");
    assert_eq!(
        report["plugins"],
        json!([
            { "scope": "project", "id": "acme.charts", "version": "1.2.3", "status": "installed" },
            { "scope": "project", "id": "acme.codec", "version": "1.0.0", "status": "skipped", "reason": "no_build" }
        ])
    );

    let output = run(
        &home,
        &registry,
        &project,
        &["install", "--locked", "--offline", "--json"],
    );
    assert_exit(&output, 0);
    let report = json_stdout(&output);
    assert_eq!(report["lockfile"], "satisfied");
    assert_eq!(report["plugins"][0]["status"], "already_installed");
    assert_eq!(report["plugins"][1]["status"], "skipped");
}

#[test]
fn install_json_reports_a_written_lock_even_if_download_fails() {
    let home = Home::new();
    let registry = registry();
    let project = home.project(r#"{ "plugins": { "dependencies": { "acme.charts": "1" } } }"#);
    let output = run(&home, &registry, &project, &["install", "--json"]);
    assert_exit(&output, 1);
    let report = json_stdout(&output);
    assert_eq!(report["error"]["code"], "plugin_not_found");
    assert!(report.get("lockfile").is_none());
    assert!(!project.join("celesta.lock").exists());

    registry.publish("acme.charts", "1.2.3", &["any"]);
    fs::remove_file(
        registry
            .served_dir()
            .join("archives/acme.charts/1.2.3/any.zip"),
    )
    .unwrap();
    let output = run(&home, &registry, &project, &["install", "--json"]);
    assert_exit(&output, 1);
    let report = json_stdout(&output);
    assert_eq!(report["error"]["code"], "install_failed");
    assert_eq!(report["lockfile"], "written");
    assert_eq!(
        locked_version(&project, "acme.charts").as_deref(),
        Some("1.2.3")
    );
}
