//! A project's own settings for plugins: `celesta.config.json` at the
//! project root may list folders of plugins that come with the project
//! (`plugins.paths`), relative to the root and inside it. The editor reads
//! them only once the user trusts the project, since they are code the
//! project brings.

use crate::manifest::is_contained_path;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The project settings file at a project root.
pub const PROJECT_CONFIG: &str = "celesta.config.json";

#[derive(Debug, Default, Deserialize)]
struct ProjectConfig {
    #[serde(default)]
    plugins: ProjectPlugins,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectPlugins {
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectConfigError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a project settings file: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path}: plugins.paths entry {entry:?} is not a relative path inside the project")]
    Outside { path: PathBuf, entry: String },
    #[error("{path}: plugins.paths entry {entry:?} is not a folder that exists")]
    Missing { path: PathBuf, entry: String },
}

/// The project folder `entry` belongs to: the nearest folder from the
/// entry's own up that holds `celesta.config.json`, else the entry's folder.
/// It is what the user is asked to trust, and where `celesta.config.json` is
/// read. A `package.json` or `tsconfig.json` does not make a project root:
/// one in a home folder would make the whole home folder the project.
pub fn project_root(entry: &Path) -> PathBuf {
    let entry = std::path::absolute(entry).unwrap_or_else(|_| entry.to_path_buf());
    let folder = entry
        .parent()
        .map_or_else(|| entry.clone(), Path::to_path_buf);
    folder
        .ancestors()
        .find(|dir| has_project_config(dir))
        .unwrap_or(&folder)
        .to_path_buf()
}

/// Whether `root` holds `celesta.config.json`, which marks it as a project.
/// Only such a root's trust is saved: without the file, the folder trusted
/// would be whatever folder an entry happened to be opened from.
pub fn has_project_config(root: &Path) -> bool {
    root.join(PROJECT_CONFIG).is_file()
}

/// The plugin folders `root`'s `celesta.config.json` lists, absolute, in
/// order; none without the file. Every entry must stay inside the project,
/// by its text and with links resolved, and must exist: the project named
/// it, so a mistyped folder is reported rather than skipped.
pub fn project_plugin_paths(root: &Path) -> Result<Vec<PathBuf>, ProjectConfigError> {
    let path = root.join(PROJECT_CONFIG);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ProjectConfigError::Read { path, source }),
    };
    let config: ProjectConfig =
        serde_json::from_str(&text).map_err(|source| ProjectConfigError::Parse {
            path: path.clone(),
            source,
        })?;
    // Without the root's own resolved path, links out of it could not be
    // told apart, so the folders are refused rather than trusted.
    let canonical_root =
        std::fs::canonicalize(root).map_err(|source| ProjectConfigError::Read {
            path: root.to_path_buf(),
            source,
        })?;
    config
        .plugins
        .paths
        .into_iter()
        .map(|entry| {
            let outside = || ProjectConfigError::Outside {
                path: path.clone(),
                entry: entry.clone(),
            };
            if !is_contained_path(&entry) {
                return Err(outside());
            }
            let folder = root.join(&entry);
            let resolved =
                std::fs::canonicalize(&folder).map_err(|_| ProjectConfigError::Missing {
                    path: path.clone(),
                    entry: entry.clone(),
                })?;
            if !resolved.starts_with(&canonical_root) {
                return Err(outside());
            }
            Ok(folder)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_is_the_nearest_folder_with_project_settings() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(project.join("scenes")).unwrap();
        // A package.json above does not make a root: the entry's own folder.
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            project_root(&project.join("scenes").join("a.tsx")),
            project.join("scenes")
        );
        assert!(!has_project_config(&project));
        std::fs::write(project.join(PROJECT_CONFIG), "{}").unwrap();
        assert!(has_project_config(&project));
        assert_eq!(project_root(&project.join("scenes").join("a.tsx")), project);
        assert_eq!(project_root(&project.join("film.tsx")), project);
    }

    #[test]
    fn plugin_paths_stay_inside_the_project() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path();
        assert!(project_plugin_paths(project).unwrap().is_empty());
        let write = |paths: &[&str]| {
            std::fs::write(
                project.join(PROJECT_CONFIG),
                serde_json::json!({ "plugins": { "paths": paths } }).to_string(),
            )
            .unwrap();
        };
        write(&["plugins", "vendor/more"]);
        // Folders the project names must exist.
        assert!(matches!(
            project_plugin_paths(project),
            Err(ProjectConfigError::Missing { .. })
        ));
        std::fs::create_dir_all(project.join("plugins")).unwrap();
        std::fs::create_dir_all(project.join("vendor/more")).unwrap();
        assert_eq!(
            project_plugin_paths(project).unwrap(),
            [project.join("plugins"), project.join("vendor/more")]
        );
        for outside in ["../elsewhere", "/abs", "C:\\abs", "a/../../b"] {
            write(&[outside]);
            assert!(
                matches!(
                    project_plugin_paths(project),
                    Err(ProjectConfigError::Outside { .. })
                ),
                "{outside}"
            );
        }
        std::fs::write(project.join(PROJECT_CONFIG), "{").unwrap();
        assert!(matches!(
            project_plugin_paths(project),
            Err(ProjectConfigError::Parse { .. })
        ));
    }

    #[test]
    fn plugin_paths_may_not_link_outside() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, project.join("plugins"));
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, project.join("plugins"));
        if linked.is_err() {
            eprintln!("skipped: could not create a symbolic link");
            return;
        }
        std::fs::write(
            project.join(PROJECT_CONFIG),
            r#"{ "plugins": { "paths": ["plugins"] } }"#,
        )
        .unwrap();
        assert!(matches!(
            project_plugin_paths(&project),
            Err(ProjectConfigError::Outside { .. })
        ));
    }
}
