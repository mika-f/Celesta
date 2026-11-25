use std::path::{Path, PathBuf};

/// Locates the bundled Node.js and React CLI beside the running executable.
/// Source builds without a `runtime` directory use the workspace and PATH.
/// A moved Cargo build can still use the runtime in its moved source tree.
/// An incomplete bundle is kept selected so spawning reports the missing file
/// instead of silently depending on software installed on the user's machine.
pub fn runtime_paths() -> (PathBuf, PathBuf) {
    paths_for(
        std::env::current_exe().ok().as_deref(),
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    )
}

fn paths_for(executable: Option<&Path>, build_workspace: &Path) -> (PathBuf, PathBuf) {
    let build_runtime = build_workspace.join("packages/cli/dist/cli.js");
    if let Some(directory) = executable.and_then(Path::parent) {
        if directory.file_name().is_some_and(|name| name == "MacOS")
            && let Some(contents) = directory.parent()
            && contents.file_name().is_some_and(|name| name == "Contents")
        {
            return (
                contents.join("Helpers/node"),
                contents.join("Resources/react/dist/cli.js"),
            );
        }
        let runtime = directory.join("runtime");
        let packaged_name = executable.and_then(Path::file_stem).is_some_and(|name| {
            name == "Celesta"
                || name == "Celesta-export"
                || name == "Celesta-mcp"
                || name == "Celesta-cli"
        });
        if packaged_name || runtime.is_dir() {
            return (
                runtime.join(if cfg!(windows) { "node.exe" } else { "node" }),
                runtime.join("react/dist/cli.js"),
            );
        }
        if cfg!(target_os = "linux")
            && !build_runtime.is_file()
            && !build_workspace.join("Cargo.toml").is_file()
            && let Some(workspace) = directory
                .ancestors()
                .find(|path| path.file_name().is_some_and(|name| name == "target"))
                .and_then(Path::parent)
        {
            let cli = workspace.join("packages/cli/dist/cli.js");
            if cli.is_file() {
                return (PathBuf::from("node"), cli);
            }
        }
    }
    (PathBuf::from("node"), build_runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_for(executable: Option<&Path>) -> (PathBuf, PathBuf) {
        super::paths_for(executable, Path::new("build"))
    }

    #[test]
    fn app_bundle_paths_are_relative_to_the_installed_app_even_if_files_are_missing() {
        let contents = Path::new("Applications/Celesta.app/Contents");
        for name in ["Celesta", "Celesta-export", "Celesta-mcp", "Celesta-cli"] {
            assert_eq!(
                paths_for(Some(&contents.join("MacOS").join(name))),
                (
                    contents.join("Helpers/node"),
                    contents.join("Resources/react/dist/cli.js"),
                )
            );
        }
    }

    #[test]
    fn incomplete_bundle_does_not_fall_back_to_development_runtime() {
        let root = std::env::temp_dir().join(format!("celesta-runtime-{}", std::process::id()));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let paths = paths_for(Some(&root.join("Celesta.exe")));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            paths,
            (
                root.join(if cfg!(windows) {
                    "runtime/node.exe"
                } else {
                    "runtime/node"
                }),
                root.join("runtime/react/dist/cli.js"),
            )
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_moved_cargo_build_uses_its_own_workspace_even_if_an_empty_build_directory_remains() {
        let root = tempfile::tempdir().unwrap();
        let cli = root.path().join("packages/cli/dist/cli.js");
        let build_workspace = root.path().join("missing");
        std::fs::create_dir(&build_workspace).unwrap();
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "").unwrap();

        for subdirectory in [
            "target/release",
            "target/x86_64-unknown-linux-gnu/debug/deps",
        ] {
            let directory = root.path().join(subdirectory);
            let executable = directory.join("celesta");
            assert_eq!(
                super::paths_for(Some(&executable), &build_workspace),
                (PathBuf::from("node"), cli.clone())
            );

            let runtime = directory.join("runtime");
            std::fs::create_dir_all(&runtime).unwrap();
            assert_eq!(
                paths_for(Some(&executable)),
                (runtime.join("node"), runtime.join("react/dist/cli.js"))
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_shared_cargo_target_uses_the_runtime_from_the_checkout_that_built_it() {
        let root = tempfile::tempdir().unwrap();
        let build_workspace = root.path().join("build");
        std::fs::create_dir(&build_workspace).unwrap();
        std::fs::write(build_workspace.join("Cargo.toml"), "[workspace]\n").unwrap();
        let other_runtime = root.path().join("other/packages/cli/dist/cli.js");
        std::fs::create_dir_all(other_runtime.parent().unwrap()).unwrap();
        std::fs::write(other_runtime, "").unwrap();
        let executable = root.path().join("other/target/release/celesta");
        let build_runtime = build_workspace.join("packages/cli/dist/cli.js");
        assert_eq!(
            super::paths_for(Some(&executable), &build_workspace),
            (PathBuf::from("node"), build_runtime.clone())
        );
        std::fs::remove_file(build_workspace.join("Cargo.toml")).unwrap();
        std::fs::create_dir_all(build_runtime.parent().unwrap()).unwrap();
        std::fs::write(&build_runtime, "").unwrap();
        assert_eq!(
            super::paths_for(Some(&executable), &build_workspace),
            (PathBuf::from("node"), build_runtime)
        );
    }

    #[test]
    fn missing_bundle_does_not_fall_back_to_development_runtime() {
        let root = Path::new("missing-celesta-installation");
        for name in ["Celesta-export.exe", "Celesta-cli.exe"] {
            let (_, cli) = paths_for(Some(&root.join(name)));
            assert_eq!(cli, root.join("runtime/react/dist/cli.js"));
        }
    }

    #[test]
    fn packaged_mcp_uses_the_runtime_folder() {
        let root = Path::new("missing-celesta-installation");
        let (node, cli) = paths_for(Some(&root.join("Celesta-mcp.exe")));
        assert_eq!(
            node,
            root.join(if cfg!(windows) {
                "runtime/node.exe"
            } else {
                "runtime/node"
            })
        );
        assert_eq!(cli, root.join("runtime/react/dist/cli.js"));
    }
}
