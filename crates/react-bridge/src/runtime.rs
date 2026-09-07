use std::path::{Path, PathBuf};

/// Locates the bundled Node.js and React CLI beside the running executable.
/// Source builds without a `runtime` directory use the workspace and PATH.
/// An incomplete bundle is kept selected so spawning reports the missing file
/// instead of silently depending on software installed on the user's machine.
pub fn runtime_paths() -> (PathBuf, PathBuf) {
    paths_for(std::env::current_exe().ok().as_deref())
}

fn paths_for(executable: Option<&Path>) -> (PathBuf, PathBuf) {
    if let Some(directory) = executable.and_then(Path::parent) {
        let runtime = directory.join("runtime");
        let packaged_name = executable
            .and_then(Path::file_stem)
            .is_some_and(|name| name == "Frameweave" || name == "Frameweave-export");
        if packaged_name || runtime.is_dir() {
            return (
                runtime.join(if cfg!(windows) { "node.exe" } else { "node" }),
                runtime.join("react/dist/cli.js"),
            );
        }
    }
    (
        PathBuf::from("node"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/react/dist/cli.js"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_bundle_does_not_fall_back_to_development_runtime() {
        let root = std::env::temp_dir().join(format!("frameweave-runtime-{}", std::process::id()));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let paths = paths_for(Some(&root.join("Frameweave.exe")));
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

    #[test]
    fn missing_bundle_does_not_fall_back_to_development_runtime() {
        let root = Path::new("missing-frameweave-installation");
        let (_, cli) = paths_for(Some(&root.join("Frameweave-export.exe")));
        assert_eq!(cli, root.join("runtime/react/dist/cli.js"));
    }
}
