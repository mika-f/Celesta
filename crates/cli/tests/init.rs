use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn lists_and_initializes_from_an_installed_runtime_without_node() {
    let scratch = tempfile::tempdir().unwrap();
    let binary = scratch
        .path()
        .join(format!("celesta{}", std::env::consts::EXE_SUFFIX));
    let built = env!("CARGO_BIN_EXE_celesta");
    if fs::hard_link(built, &binary).is_err() {
        fs::copy(built, &binary).unwrap();
    }
    let runtime = scratch.path().join("runtime/react/dist");
    let types = runtime.join("project-types");
    fs::create_dir_all(types.join("node_modules")).unwrap();
    fs::write(types.join("version.json"), "{}").unwrap();
    fs::write(types.join("tsconfig.json"), "{}").unwrap();
    for name in ["blank", "slideshow"] {
        let template = runtime.join("templates").join(name);
        fs::create_dir_all(&template).unwrap();
        fs::write(
            template.join("template.json"),
            format!(r#"{{"title":"{name}","description":"Starter","edit":"Edit film.tsx"}}"#),
        )
        .unwrap();
        fs::write(template.join("film.tsx"), name).unwrap();
    }

    let run = |args: &[&str], directory: &Path| {
        Command::new(&binary)
            .args(args)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .current_dir(directory)
            .output()
            .unwrap()
    };
    let output = run(&["templates"], scratch.path());
    assert!(output.status.success(), "{output:?}");
    let listed = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        listed
            .lines()
            .map(|line| line.split_whitespace().next().unwrap())
            .collect::<Vec<_>>(),
        ["blank", "slideshow"]
    );

    let output = run(
        &["init", "video", "--template", "slideshow"],
        scratch.path(),
    );
    assert!(output.status.success(), "{output:?}");
    let project = scratch.path().join("video");
    assert_eq!(
        fs::read_to_string(project.join("film.tsx")).unwrap(),
        "slideshow"
    );
    assert!(project.join(".celesta/version.json").is_file());

    let output = run(&["init"], &project);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(project.join("film.tsx")).unwrap(),
        "slideshow"
    );
    assert_eq!(
        run(&["init", "--template", "blank"], &project)
            .status
            .code(),
        Some(1)
    );

    let output = run(
        &["init", "bad-project", "--template", "missing"],
        scratch.path(),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!scratch.path().join("bad-project").exists());
}
