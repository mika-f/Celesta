//! A project's own settings for plugins: `celesta.config.json` at the
//! project root may list folders of plugins that come with the project
//! (`plugins.paths`), relative to the root and inside it. The editor reads
//! them only once the user trusts the project, since they are code the
//! project brings.

use crate::manifest::is_contained_path;
use celesta_registry_format::{Target, is_registry_id};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The project settings file at a project root.
pub const PROJECT_CONFIG: &str = "celesta.config.json";

/// The targets a project builds for when `plugins.targets` is absent: the
/// platforms Celesta is released for.
pub const RELEASE_TARGETS: [&str; 3] = ["macos-aarch64", "windows-x86_64", "linux-x86_64"];

#[derive(Debug, Default, Deserialize)]
struct ProjectConfig {
    #[serde(default)]
    plugins: ProjectPlugins,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectPlugins {
    #[serde(default)]
    paths: Vec<String>,
    /// Plugin id to a version requirement, as written.
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    /// Build targets, as written.
    targets: Option<Vec<String>>,
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
    #[error("{path}: plugins.dependencies entry {id:?}: {reason}")]
    BadDependency {
        path: PathBuf,
        id: String,
        reason: String,
    },
    #[error(
        "{path}: plugins.targets entry {target:?} is not a target such as \"linux-x86_64\" or \"any\""
    )]
    BadTarget { path: PathBuf, target: String },
    #[error("{path}: plugins.targets lists no target")]
    NoTargets { path: PathBuf },
}

/// The registry plugins a project depends on and the targets it builds for,
/// from `celesta.config.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectDependencies {
    /// Plugin id to the versions the project accepts, with Cargo's meaning
    /// (`"0.2"` is `^0.2`).
    pub dependencies: BTreeMap<String, semver::VersionReq>,
    /// `plugins.targets` as written, or `None` when absent.
    pub targets: Option<Vec<Target>>,
}

impl ProjectDependencies {
    /// The targets the project builds for: `targets`, else
    /// [`RELEASE_TARGETS`]; sorted and without repeats.
    pub fn project_targets(&self) -> Vec<Target> {
        let mut targets = self.targets.clone().unwrap_or_else(|| {
            RELEASE_TARGETS
                .iter()
                .filter_map(|target| Target::parse(target).ok())
                .collect()
        });
        targets.sort();
        targets.dedup();
        targets
    }
}

/// The project folder `entry` belongs to: the nearest folder from the
/// entry's own up that holds `celesta.config.json`, else the entry's folder.
/// It is what the user is asked to trust, where `celesta.config.json` is
/// read, and where TypeScript support (`.celesta/`) is set up and refreshed;
/// nothing above it is written. A `package.json` or `tsconfig.json` does not
/// make a project root: one in a home folder would make the whole home
/// folder the project.
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

/// Makes `root` a project root by writing an empty `celesta.config.json`,
/// keeping one that is already there. A write that fails removes the file
/// it created, so a retry is not stopped by a partial one.
pub fn mark_project_root(root: &Path) -> std::io::Result<()> {
    let path = root.join(PROJECT_CONFIG);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => std::io::Write::write_all(&mut file, b"{}\n").inspect_err(|_| {
            drop(file);
            let _ = std::fs::remove_file(&path);
        }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_file() => Ok(()),
        Err(error) => Err(error),
    }
}

/// `path`'s settings, or `None` when there is no such file.
fn read_config(path: &Path) -> Result<Option<ProjectConfig>, ProjectConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ProjectConfigError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|source| ProjectConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
}

/// The registry plugins `root`'s `celesta.config.json` depends on
/// (`plugins.dependencies`) and the targets it builds for
/// (`plugins.targets`); no dependencies and no targets without the file.
/// Every id must be a registry id and every requirement a Cargo version
/// requirement, and `targets`, when given, must name at least one target:
/// the project wrote them, so a mistake is reported rather than skipped.
pub fn project_dependencies(root: &Path) -> Result<ProjectDependencies, ProjectConfigError> {
    let path = root.join(PROJECT_CONFIG);
    let Some(config) = read_config(&path)? else {
        return Ok(ProjectDependencies::default());
    };
    let dependencies = config
        .plugins
        .dependencies
        .into_iter()
        .map(|(id, text)| {
            let bad = |reason: String| ProjectConfigError::BadDependency {
                path: path.clone(),
                id: id.clone(),
                reason,
            };
            if !is_registry_id(&id) {
                return Err(bad("is not a registry id".into()));
            }
            let requirement = semver::VersionReq::parse(&text)
                .map_err(|error| bad(format!("{text:?} is not a version requirement: {error}")))?;
            Ok((id, requirement))
        })
        .collect::<Result<_, _>>()?;
    let targets = config
        .plugins
        .targets
        .map(|targets| {
            if targets.is_empty() {
                return Err(ProjectConfigError::NoTargets { path: path.clone() });
            }
            targets
                .into_iter()
                .map(|target| {
                    Target::parse(&target).map_err(|_| ProjectConfigError::BadTarget {
                        path: path.clone(),
                        target,
                    })
                })
                .collect()
        })
        .transpose()?;
    Ok(ProjectDependencies {
        dependencies,
        targets,
    })
}

/// The plugin folders `root`'s `celesta.config.json` lists, absolute, in
/// order; none without the file. Every entry must stay inside the project,
/// by its text and with links resolved, and must exist: the project named
/// it, so a mistyped folder is reported rather than skipped.
pub fn project_plugin_paths(root: &Path) -> Result<Vec<PathBuf>, ProjectConfigError> {
    let path = root.join(PROJECT_CONFIG);
    let Some(config) = read_config(&path)? else {
        return Ok(Vec::new());
    };
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
    fn marking_a_root_keeps_existing_settings() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path();
        mark_project_root(project).unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join(PROJECT_CONFIG)).unwrap(),
            "{}\n"
        );
        let settings = r#"{ "plugins": { "paths": [] } }"#;
        std::fs::write(project.join(PROJECT_CONFIG), settings).unwrap();
        mark_project_root(project).unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join(PROJECT_CONFIG)).unwrap(),
            settings
        );
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

    fn write_config(project: &Path, config: serde_json::Value) {
        std::fs::write(project.join(PROJECT_CONFIG), config.to_string()).unwrap();
    }

    #[test]
    fn dependencies_are_read() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path();
        let none = project_dependencies(project).unwrap();
        assert!(none.dependencies.is_empty());
        assert_eq!(none.targets, None);
        write_config(
            project,
            serde_json::json!({ "plugins": { "dependencies": {
                "vrm": "0.2", "voicevox": "0.9", "ffmpeg-x264": "=0.9.0",
            } } }),
        );
        let read = project_dependencies(project).unwrap();
        let requirement = |text: &str| semver::VersionReq::parse(text).unwrap();
        assert_eq!(
            read.dependencies,
            std::collections::BTreeMap::from([
                ("ffmpeg-x264".to_owned(), requirement("=0.9.0")),
                ("vrm".to_owned(), requirement("^0.2")),
                ("voicevox".to_owned(), requirement("^0.9")),
            ])
        );
    }

    #[test]
    fn targets_default_to_the_release_targets() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path();
        let defaults: Vec<Target> = RELEASE_TARGETS
            .iter()
            .map(|target| Target::parse(target).unwrap())
            .collect();
        let mut sorted = defaults.clone();
        sorted.sort();
        assert_eq!(
            project_dependencies(project).unwrap().project_targets(),
            sorted
        );
        write_config(project, serde_json::json!({ "plugins": { "paths": [] } }));
        let read = project_dependencies(project).unwrap();
        assert_eq!(read.targets, None);
        assert_eq!(read.project_targets(), sorted);
    }

    #[test]
    fn given_targets_are_used() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path();
        write_config(
            project,
            serde_json::json!({ "plugins": { "targets": ["linux-x86_64", "any", "linux-x86_64"] } }),
        );
        let read = project_dependencies(project).unwrap();
        assert_eq!(
            read.targets,
            Some(vec![
                Target::parse("linux-x86_64").unwrap(),
                Target::ANY,
                Target::parse("linux-x86_64").unwrap(),
            ])
        );
        let mut expected = vec![Target::parse("linux-x86_64").unwrap(), Target::ANY];
        expected.sort();
        assert_eq!(read.project_targets(), expected);
    }

    #[test]
    fn bad_entries_are_reported() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path();
        for (id, requirement) in [("VRM", "0.2"), ("a.b.c", "0.2"), ("vrm", "0.2 || 0.3")] {
            write_config(
                project,
                serde_json::json!({ "plugins": { "dependencies": { id: requirement } } }),
            );
            assert!(
                matches!(
                    project_dependencies(project),
                    Err(ProjectConfigError::BadDependency { id: ref got, .. }) if got == id
                ),
                "{id} {requirement}"
            );
        }
        write_config(project, serde_json::json!({ "plugins": { "targets": [] } }));
        assert!(matches!(
            project_dependencies(project),
            Err(ProjectConfigError::NoTargets { .. })
        ));
        write_config(
            project,
            serde_json::json!({ "plugins": { "targets": ["Linux"] } }),
        );
        assert!(matches!(
            project_dependencies(project),
            Err(ProjectConfigError::BadTarget { ref target, .. }) if target == "Linux"
        ));
    }
}
