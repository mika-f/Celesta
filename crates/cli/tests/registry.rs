mod common;

use std::fs;

use celesta_registry_client::testing::{TestRegistry, celesta_range};
use common::{Home, assert_exit, json_stdout, registry, run, stderr, stdout};
use serde_json::{Value, json};

/// An advisory blocking `versions` of `plugin`, fixed in `fixed`.
fn advisory(id: &str, plugin: &str, versions: &str, fixed: Option<&str>) -> Value {
    json!({
        "id": id,
        "plugin": plugin,
        "versions": versions,
        "title": "Runs code from a chart title",
        "description": "A chart title is evaluated as a script.",
        "fixed": fixed,
        "published": "2026-11-01T00:00:00Z",
    })
}

fn lines(output: &std::process::Output) -> Vec<String> {
    stdout(output).lines().map(str::to_owned).collect()
}

#[test]
fn search_matches_id_and_description() {
    let home = Home::new();
    let registry = registry();
    let entry = |id: &str, description: &str, latest: Option<&str>| {
        json!({
            "id": id,
            "publisher": "acme",
            "kind": { "roles": ["content"], "program": false, "module": true },
            "description": description,
            "latest": latest,
        })
    };
    let mut old = entry("acme.oldcharts", "The first charts.", None);
    old["deprecated"] = json!({ "message": "use acme.charts" });
    registry.search(json!([
        entry("acme.charts", "Bar and line graphs.", Some("1.2.3")),
        entry("acme.graphs", "Draws CHARTS from tables.", Some("0.4.0")),
        entry("acme.voices", "Speech from text.", Some("2.0.0")),
        old,
    ]));

    let output = run(&home, &registry, home.path(), &["search", "Chart"]);
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        [
            "acme.charts  1.2.3  Bar and line graphs.",
            "acme.graphs  0.4.0  Draws CHARTS from tables.",
            "acme.oldcharts  -  The first charts. (deprecated)",
        ]
    );

    let output = run(&home, &registry, home.path(), &["search", "video"]);
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["no plugin matches video"]);
}

#[test]
fn info_shows_states_and_source() {
    let home = Home::new();
    let registry = registry();
    for version in ["1.0.0", "1.1.0", "1.2.0", "1.3.0"] {
        registry.publish("acme.charts", version, &["any"]);
    }
    registry.publish("acme.charts", "2.0.0", &["linux-x86_64", "macos-aarch64"]);
    let project = home.project("{}\n");
    assert_exit(
        &run(&home, &registry, &project, &["add", "acme.charts@=1.0.0"]),
        0,
    );
    registry.edit("acme.charts", "1.1.0", |entry| {
        entry["yanked"] = json!(true)
    });
    registry.edit("acme.charts", "1.2.0", |entry| {
        entry["withdrawn"] = json!(true)
    });
    registry.edit("acme.charts", "1.3.0", |entry| {
        entry["deprecated"] = json!({ "message": "no longer kept" })
    });
    registry.edit("acme.charts", "2.0.0", |entry| {
        entry["license"] = json!("MIT OR Apache-2.0")
    });
    registry.advisories(json!([advisory(
        "CELESTA-2026-0001",
        "acme.charts",
        "<1.0.1",
        Some("1.0.1")
    )]));

    let output = run(&home, &registry, &project, &["info", "acme.charts"]);
    assert_exit(&output, 0);
    let range = celesta_range();
    assert_eq!(
        lines(&output),
        [
            "acme.charts by acme: A test plugin.".to_owned(),
            "roles: content".to_owned(),
            "license: MIT OR Apache-2.0".to_owned(),
            format!("2.0.0  celesta {range}  linux-x86_64, macos-aarch64  published"),
            format!("1.3.0  celesta {range}  any  deprecated"),
            format!("1.2.0  celesta {range}  any  withdrawn"),
            format!("1.1.0  celesta {range}  any  yanked"),
            format!("1.0.0  celesta {range}  any  blocked (CELESTA-2026-0001)"),
            "source: locked in this project: 1.0.0".to_owned(),
        ]
    );

    // Outside the project, nothing uses it.
    let output = run(&home, &registry, home.path(), &["info", "acme.charts"]);
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output).last().map(String::as_str),
        Some("source: not locked in this project or installed globally")
    );

    let output = run(&home, &registry, &project, &["info", "acme.missing"]);
    assert_exit(&output, 1);
    assert!(
        stderr(&output).contains("celesta: error: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn list_shows_project_and_global() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    assert_exit(
        &run(
            &home,
            &registry,
            &project,
            &["install", "--global", "acme.codec"],
        ),
        0,
    );
    registry.edit("acme.charts", "1.2.3", |entry| {
        entry["yanked"] = json!(true)
    });
    // `info` caches the index that says so.
    assert_exit(
        &run(&home, &registry, &project, &["info", "acme.charts"]),
        0,
    );

    let output = run(&home, &registry, &project, &["list"]);
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        [
            "This project",
            "  acme.charts 1.2.3  installed yanked",
            "Global",
            "  acme.codec 1.0.0  installed",
        ]
    );

    // Another user has neither installed.
    let other = Home::new();
    let output = run(&other, &registry, &project, &["list"]);
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        [
            "This project",
            "  acme.charts 1.2.3  not installed",
            "Global",
            "  none",
        ]
    );

    let output = run(&other, &registry, other.path(), &["list"]);
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["no registry plugins"]);

    let output = run(&home, &registry, &project, &["list", "--json"]);
    assert_exit(&output, 0);
    let report = json_stdout(&output);
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["status"], "ok");
    assert_eq!(report["command"], "list");
    assert_eq!(
        report["plugins"],
        json!([
            { "scope": "project", "id": "acme.charts", "version": "1.2.3", "status": "installed", "yanked": true, "deprecated": false },
            { "scope": "global", "id": "acme.codec", "version": "1.0.0", "status": "installed", "yanked": false, "deprecated": false }
        ])
    );
    let output = run(&other, &registry, &project, &["list", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(json_stdout(&output)["plugins"][0]["status"], "missing");
}

#[test]
fn list_works_without_the_registry() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    // Nothing to reach any more.
    drop(registry);

    let output = home
        .command()
        .current_dir(&project)
        .args(["plugin", "list"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        [
            "This project",
            "  acme.charts 1.2.3  installed",
            "Global",
            "  none"
        ]
    );
    assert!(
        !stderr(&output).contains("celesta: error"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn audit_fails_on_a_blocked_lock_entry() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    registry.advisories(json!([advisory(
        "CELESTA-2026-0002",
        "acme.charts",
        "<=1.2.3",
        Some("1.2.4")
    )]));

    let output = run(&home, &registry, &project, &["audit"]);
    assert_exit(&output, 1);
    assert_eq!(
        lines(&output),
        [
            "acme.charts 1.2.3: blocked by CELESTA-2026-0002: Runs code from a chart title; 1.2.4 fixes it"
        ]
    );

    let output = run(&home, &registry, &project, &["audit", "--json"]);
    assert_exit(&output, 1);
    let report = json_stdout(&output);
    assert_eq!(report["error"]["code"], "audit_failed");
    assert_eq!(report["advisoriesAvailable"], true);
    let plugin = &report["plugins"][0];
    assert_eq!(plugin["scope"], "project");
    assert_eq!(plugin["id"], "acme.charts");
    assert_eq!(plugin["version"], "1.2.3");
    assert_eq!(plugin["status"], "blocked");
    assert_eq!(plugin["findings"][0]["code"], "plugin_blocked");
    assert_eq!(plugin["findings"][0]["advisory"]["id"], "CELESTA-2026-0002");
    assert_eq!(plugin["findings"][0]["advisory"]["fixed"], "1.2.4");
}

#[test]
fn audit_names_withdrawn_yanked_and_deprecated_versions() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    registry.publish("acme.graphs", "0.1.0", &["any"]);
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    assert_exit(&run(&home, &registry, &project, &["add", "acme.graphs"]), 0);
    assert_exit(
        &run(
            &home,
            &registry,
            &project,
            &["install", "--global", "acme.codec"],
        ),
        0,
    );
    registry.edit("acme.charts", "1.2.3", |entry| {
        entry["yanked"] = json!(true)
    });
    registry.edit("acme.graphs", "0.1.0", |entry| {
        entry["deprecated"] = json!({ "message": "no longer kept", "replacement": "acme.charts" })
    });
    registry.edit("acme.codec", "1.0.0", |entry| {
        entry["withdrawn"] = json!(true)
    });

    let output = run(&home, &registry, &project, &["audit"]);
    assert_exit(&output, 1);
    assert_eq!(
        lines(&output),
        [
            "acme.charts 1.2.3 has been yanked",
            "acme.graphs 0.1.0 is deprecated: no longer kept (use acme.charts instead)",
            "acme.codec 1.0.0 is no longer distributed",
        ]
    );

    // When the served indexes cannot be read, the cached ones say the same.
    for entry in fs::read_dir(registry.served_dir().join("index")).unwrap() {
        fs::write(entry.unwrap().path(), "not an index").unwrap();
    }
    let output = run(&home, &registry, &project, &["audit"]);
    assert_exit(&output, 1);
    assert_eq!(lines(&output).len(), 3, "{}", stdout(&output));
    assert!(
        stderr(&output).contains("celesta: warning: using the cached index of acme.charts: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn audit_fails_when_the_registry_dropped_the_plugin() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    let index = registry.served_dir().join("index");
    fs::remove_file(index.join("acme.charts.json")).unwrap();
    fs::remove_file(index.join("acme.charts.json.sig")).unwrap();

    // The cached index still lists it, but the registry no longer does.
    let output = run(&home, &registry, &project, &["audit"]);
    assert_exit(&output, 1);
    assert_eq!(
        lines(&output),
        ["acme.charts 1.2.3 is no longer distributed"]
    );
    assert!(
        !stderr(&output).contains("using the cached index"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn audit_works_without_the_registry() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    // The registry goes away; that run cached its files.
    fs::remove_dir_all(registry.served_dir()).unwrap();

    let output = home
        .with_registry(&registry)
        .current_dir(&project)
        .args(["plugin", "audit"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["no problems found"]);
    assert!(
        stderr(&output).contains("celesta: warning: using the cached index of acme.charts: "),
        "{}",
        stderr(&output)
    );
    assert!(
        !stderr(&output).contains("celesta: error"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn audit_says_when_it_could_not_check_everything() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);

    // Another user, with no registry to reach and nothing cached.
    let output = Home::new()
        .command()
        .current_dir(&project)
        .args(["plugin", "audit"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        ["no problems found in what could be checked"]
    );
    assert!(
        stderr(&output).contains("celesta: warning: could not check acme.charts for yanked, withdrawn, or deprecated versions: "),
        "{}",
        stderr(&output)
    );

    let output = Home::new()
        .command()
        .current_dir(&project)
        .args(["plugin", "audit", "--json"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    let report = json_stdout(&output);
    assert_eq!(report["advisoriesAvailable"], false);
    assert_eq!(report["plugins"][0]["listingChecked"], false);
    assert_eq!(report["plugins"][0]["status"], "skipped");
}

#[test]
fn audit_passes_a_clean_project() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.2.3", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);

    let output = run(&home, &registry, &project, &["audit"]);
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["no problems found"]);
    assert_eq!(stderr(&output), "");
}

#[cfg(unix)]
#[test]
fn audit_reports_an_installed_plugin_that_cannot_launch() {
    use std::os::unix::fs::PermissionsExt;

    use celesta_plugin_host::{MANIFEST, UserSettings};
    use celesta_registry_format::tree_hash;

    let home = Home::new();
    let registry = registry();
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    let project = home.project("{}\n");
    assert_exit(
        &run(
            &home,
            &registry,
            &project,
            &["install", "--global", "acme.codec"],
        ),
        0,
    );

    let directory = home.store().join("acme.codec/1.0.0");
    let manifest_path = directory.join(MANIFEST);
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["roles"] = json!(["media"]);
    manifest["protocol"] = json!(2);
    manifest["executable"] = json!({ (std::env::consts::OS): "codec" });
    fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let executable = directory.join("codec");
    let loader_error = "libc.so.6: version `GLIBC_2.38' not found (required by codec)";
    fs::write(
        &executable,
        format!("#!/bin/sh\ntouch ../ran\necho \"{loader_error}\" >&2\nexit 127\n")
            .replace('`', "\\`"),
    )
    .unwrap();
    fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();

    let output = run(&home, &registry, &project, &["audit", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(json_stdout(&output)["plugins"][0]["findings"], json!([]));

    let mut settings = UserSettings::load(&home.settings()).unwrap();
    settings.registry_plugins[0].tree_hash = tree_hash(&directory).unwrap();
    settings.save(&home.settings()).unwrap();

    let output = run(&home, &registry, &project, &["audit"]);
    assert_exit(&output, 1);
    let text = stdout(&output);
    assert!(
        text.contains("acme.codec 1.0.0: could not launch"),
        "{text}"
    );
    assert!(text.contains(loader_error), "{text}");
    assert!(
        text.contains("hint: This plugin needs a newer glibc"),
        "{text}"
    );

    let output = run(&home, &registry, &project, &["audit", "--json"]);
    assert_exit(&output, 1);
    assert_eq!(stderr(&output), "");
    let report = json_stdout(&output);
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "audit_failed");
    let plugin = &report["plugins"][0];
    assert_eq!(plugin["scope"], "global");
    assert_eq!(plugin["status"], "launch_failed");
    assert_eq!(plugin["launchChecked"], true);
    let finding = &plugin["findings"][0];
    assert_eq!(finding["code"], "plugin_launch_failed");
    assert_eq!(finding["fails"], true);
    assert!(finding["message"].as_str().unwrap().contains(loader_error));
    assert!(finding["hint"].as_str().unwrap().contains("newer glibc"));

    settings.set_enabled("acme.codec", &directory, false);
    settings.save(&home.settings()).unwrap();
    let output = run(&home, &registry, &project, &["audit", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(json_stdout(&output)["plugins"][0]["launchChecked"], false);
    settings.set_enabled("acme.codec", &directory, true);
    settings.save(&home.settings()).unwrap();

    registry.advisories(json!([advisory(
        "CELESTA-2026-0002",
        "acme.codec",
        "=1.0.0",
        None
    )]));
    let output = run(&home, &registry, &project, &["audit", "--json"]);
    assert_exit(&output, 1);
    let report = json_stdout(&output);
    assert_eq!(report["plugins"][0]["status"], "blocked");
    assert_eq!(report["plugins"][0]["launchChecked"], false);
    let findings = report["plugins"][0]["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["code"], "plugin_blocked");

    let cache = home.store().with_file_name("registry-cache");
    fs::copy(
        registry.served_dir().join("advisories.json"),
        cache.join("advisories.json"),
    )
    .unwrap();
    let alternate = TestRegistry::file();
    alternate.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    alternate.advisories(json!([]));
    let ran = directory.with_file_name("ran");
    fs::remove_file(&ran).unwrap();
    let output = run(&home, &alternate, &project, &["audit", "--json"]);
    assert_exit(&output, 1);
    let report = json_stdout(&output);
    assert_eq!(report["plugins"][0]["status"], "blocked");
    assert_eq!(
        report["plugins"][0]["findings"][0]["advisory"]["id"],
        "CELESTA-2026-0002"
    );
    assert!(!ran.exists());
}

#[test]
fn install_global_records_registry_plugins() {
    let home = Home::new();
    let registry = registry();
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    registry.publish_with_roles("acme.codec", "1.1.0", &["any"], &["media"]);

    let output = run(
        &home,
        &registry,
        home.path(),
        &["install", "--global", "acme.codec@~1.0"],
    );
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        ["installed acme.codec 1.0.0 for every project"]
    );
    assert!(home.store().join("acme.codec").join("1.0.0").is_dir());
    let settings: Value = serde_json::from_slice(&fs::read(home.settings()).unwrap()).unwrap();
    let plugins = settings["registryPlugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1, "{settings}");
    assert_eq!(plugins[0]["id"], "acme.codec");
    assert_eq!(plugins[0]["version"], "1.0.0");

    // Without a requirement, the newest; it replaces the entry.
    let output = run(
        &home,
        &registry,
        home.path(),
        &["install", "--global", "acme.codec"],
    );
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        ["installed acme.codec 1.1.0 for every project"]
    );
    let settings: Value = serde_json::from_slice(&fs::read(home.settings()).unwrap()).unwrap();
    assert_eq!(settings["registryPlugins"].as_array().unwrap().len(), 1);
    assert_eq!(settings["registryPlugins"][0]["version"], "1.1.0");

    let output = run(
        &home,
        &registry,
        home.path(),
        &["install", "--global", "@1"],
    );
    assert_exit(&output, 2);

    let output = run(
        &home,
        &registry,
        home.path(),
        &["install", "--json", "--global", "@1"],
    );
    assert_exit(&output, 2);
    assert_eq!(json_stdout(&output)["error"]["code"], "invalid_plugin_spec");

    let output = run(
        &home,
        &registry,
        home.path(),
        &["install", "--json", "--global", "acme.codec"],
    );
    assert_exit(&output, 0);
    let report = json_stdout(&output);
    assert_eq!(report["scope"], "global");
    assert_eq!(
        report["plugins"],
        json!([
            { "scope": "global", "id": "acme.codec", "version": "1.1.0", "status": "installed" }
        ])
    );
}

#[test]
fn install_global_warns_about_a_deprecated_plugin() {
    let home = Home::new();
    let registry = registry();
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    registry.edit("acme.codec", "1.0.0", |entry| {
        entry["deprecated"] = json!({ "message": "no longer kept", "replacement": "acme.media" });
    });

    let output = run(
        &home,
        &registry,
        home.path(),
        &["install", "--global", "acme.codec"],
    );
    assert_exit(&output, 0);
    assert_eq!(
        lines(&output),
        ["installed acme.codec 1.0.0 for every project"]
    );
    assert_eq!(
        stderr(&output),
        "celesta: warning: acme.codec 1.0.0 is deprecated: no longer kept (use acme.media instead)\n"
    );
}

/// Two projects locking 1.0.0 and 2.0.0 of `acme.charts`; the first one's
/// folder is deleted.
fn two_projects_one_gone(home: &Home, registry: &TestRegistry) {
    registry.publish("acme.charts", "1.0.0", &["any"]);
    registry.publish("acme.charts", "2.0.0", &["any"]);
    let gone = home.project("{}\n");
    let kept = home.project("{}\n");
    assert_exit(&run(home, registry, &gone, &["add", "acme.charts@1"]), 0);
    assert_exit(&run(home, registry, &kept, &["add", "acme.charts@2"]), 0);
    fs::remove_dir_all(&gone).unwrap();
}

#[test]
fn gc_dry_run_removes_nothing() {
    let home = Home::new();
    let registry = registry();
    two_projects_one_gone(&home, &registry);
    let record = fs::read(home.store().join(".projects.json")).unwrap();

    let output = home
        .command()
        .args(["plugin", "gc", "--dry-run"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["would remove acme.charts 1.0.0"]);
    assert!(home.store().join("acme.charts").join("1.0.0").is_dir());
    assert_eq!(
        fs::read(home.store().join(".projects.json")).unwrap(),
        record
    );
}

#[test]
fn gc_removes_an_unused_version() {
    let home = Home::new();
    let registry = registry();
    two_projects_one_gone(&home, &registry);

    // Without a registry to reach: gc needs no network.
    let output = home.command().args(["plugin", "gc"]).output().unwrap();
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["removed acme.charts 1.0.0"]);
    assert!(!home.store().join("acme.charts").join("1.0.0").exists());
    assert!(home.store().join("acme.charts").join("2.0.0").is_dir());

    let output = home.command().args(["plugin", "gc"]).output().unwrap();
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["nothing to remove"]);
}

#[test]
fn gc_keeps_the_current_project_and_global_plugins() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.0.0", &["any"]);
    registry.publish_with_roles("acme.codec", "1.0.0", &["any"], &["media"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);
    assert_exit(
        &run(
            &home,
            &registry,
            &project,
            &["install", "--global", "acme.codec"],
        ),
        0,
    );
    // The project is no longer recorded, but gc runs in it.
    fs::remove_file(home.store().join(".projects.json")).unwrap();

    let output = home
        .command()
        .current_dir(&project)
        .args(["plugin", "gc"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(lines(&output), ["nothing to remove"]);
}

#[test]
fn gc_refuses_unreadable_settings_and_names_the_record() {
    let home = Home::new();
    let registry = registry();
    registry.publish("acme.charts", "1.0.0", &["any"]);
    let project = home.project("{}\n");
    assert_exit(&run(&home, &registry, &project, &["add", "acme.charts"]), 0);

    fs::create_dir_all(home.settings().parent().unwrap()).unwrap();
    fs::write(home.settings(), "{ not json").unwrap();
    let output = home.command().args(["plugin", "gc"]).output().unwrap();
    assert_exit(&output, 1);
    assert!(
        stderr(&output).starts_with("celesta: error: "),
        "{}",
        stderr(&output)
    );
    assert!(home.store().join("acme.charts").join("1.0.0").is_dir());
    fs::remove_file(home.settings()).unwrap();

    // A recorded project whose lock does not load stops the plan.
    fs::write(project.join("celesta.lock"), "{ not json").unwrap();
    let output = home.command().args(["plugin", "gc"]).output().unwrap();
    assert_exit(&output, 1);
    let record = home.store().join(".projects.json");
    assert!(
        stderr(&output).contains(&format!(
            "; the store's project record is {}",
            record.display()
        )),
        "{}",
        stderr(&output)
    );
    assert!(home.store().join("acme.charts").join("1.0.0").is_dir());
}
