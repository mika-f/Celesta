//! Installs and refreshes a project's `.celesta/` TypeScript support
//! directory from the template staged beside the runtime's `cli.js`
//! (`packages/cli/scripts/stage-project-types.mjs`).
//!
//! Entries always run against the bundled `react` and `@celesta.studio/*`, so
//! the project only needs matching declarations for editors and `tsc`.
//! Copying them from the running Celesta keeps them in step with the runtime
//! without a package registry or an install step.

use celesta_plugin_host::{InstalledPlugins, PluginEntry, mark_project_root, project_root};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The directory created at a project root.
pub const PROJECT_TYPES_DIR: &str = ".celesta";

const TEMPLATE_DIR: &str = "project-types";
/// Content stamp written last, so an interrupted copy is redone later.
pub(crate) const STAMP: &str = "version.json";
const EXTENDS: &str = "./.celesta/tsconfig.json";

/// The support template staged next to `cli_script`.
pub fn project_types_template(cli_script: &Path) -> PathBuf {
    cli_script
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(TEMPLATE_DIR)
}

/// How the project root's `tsconfig.json` relates to `.celesta/tsconfig.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectTsconfig {
    /// No `tsconfig.json` existed; one extending the support config was written.
    Created,
    /// The existing `tsconfig.json` already extends the support config.
    Extends,
    /// The existing `tsconfig.json` was left alone and does not extend it yet.
    MissingExtends,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectTypesSetup {
    pub root: PathBuf,
    pub tsconfig: ProjectTsconfig,
}

/// Installs `.celesta/` for `entry`'s project and writes a root
/// `tsconfig.json` when there is none. The root is `project_root`'s: the
/// nearest folder from the entry's own up that holds `celesta.config.json`,
/// else the entry's folder. A `package.json` or `tsconfig.json` above the
/// entry does not move it, so nothing outside the project is written.
pub fn set_up_project_types(template: &Path, entry: &Path) -> io::Result<ProjectTypesSetup> {
    set_up_project_types_in(template, &project_root(entry))
}

/// Installs `.celesta/` in `root` and marks it as the project root with
/// `celesta.config.json` when it has none — for a folder the user chose,
/// maybe before the project has an entry. Marked, the folder stays the root
/// for entries in its subfolders too, so [`refresh_project_types`] keeps it
/// up to date from any of them.
pub fn set_up_project_folder(template: &Path, root: &Path) -> io::Result<ProjectTypesSetup> {
    let setup = set_up_project_types_in(template, root)?;
    mark_project_root(&setup.root)?;
    Ok(setup)
}

/// Installs `.celesta/` directly in `root` and writes a `tsconfig.json`
/// there when there is none. [`refresh_project_types`] keeps it up to date
/// from entries whose project root `root` is.
pub fn set_up_project_types_in(template: &Path, root: &Path) -> io::Result<ProjectTypesSetup> {
    let root = std::path::absolute(root)?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a folder", root.display()),
        ));
    }
    install(template, &root)?;
    let path = root.join("tsconfig.json");
    let tsconfig = match fs::read_to_string(&path) {
        Ok(text) if text.contains(".celesta/tsconfig.json") => ProjectTsconfig::Extends,
        Ok(_) => ProjectTsconfig::MissingExtends,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::write(&path, format!("{{\n  \"extends\": \"{EXTENDS}\"\n}}\n"))?;
            ProjectTsconfig::Created
        }
        Err(error) => return Err(error),
    };
    Ok(ProjectTypesSetup { root, tsconfig })
}

/// Re-copies `.celesta/` when a project that already has one was set up by a
/// different Celesta build, and puts `plugins`' declarations in it either
/// way (plugins come and go without a new build). Only the entry's project
/// root is looked at: a `.celesta/` in a folder above it belongs to another
/// project, or to none. Returns the refreshed root; projects that never
/// opted in, or a runtime without a template, are left untouched.
pub fn refresh_project_types(
    template: &Path,
    entry: &Path,
    plugins: &[PluginEntry],
) -> io::Result<Option<PathBuf>> {
    let Ok(expected) = fs::read(template.join(STAMP)) else {
        return Ok(None);
    };
    let root = project_root(entry);
    let stamp = root.join(PROJECT_TYPES_DIR).join(STAMP);
    if !stamp.is_file() {
        return Ok(None);
    }
    let refreshed = if fs::read(stamp)? == expected {
        None
    } else {
        install(template, &root)?;
        Some(root.clone())
    };
    install_plugin_types(&root.join(PROJECT_TYPES_DIR), plugins)?;
    Ok(refreshed)
}

/// The plugins `InstalledPlugins::find` returns, as the exporter finds
/// them: not a project's own plugin folders, and including plugins that
/// wait for the user's approval. Only their declarations are copied, which
/// run nothing.
fn installed_plugins() -> Vec<PluginEntry> {
    let found = InstalledPlugins::find(&[], &[]).0;
    found.plugins.into_iter().chain(found.waiting).collect()
}

/// Copies each plugin's declarations to `target/plugins/<id>/index.d.ts`,
/// where the support tsconfig maps `@celesta-plugin/<id>`, replacing what
/// earlier plugins left.
pub fn install_plugin_types(target: &Path, plugins: &[PluginEntry]) -> io::Result<()> {
    let directory = target.join("plugins");
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    for plugin in plugins {
        // `discover` already skips bad ids; the id names a directory here.
        let Some(types) = plugin.types() else {
            continue;
        };
        if !celesta_plugin_host::is_valid_id(&plugin.manifest.id) {
            continue;
        }
        let destination = directory.join(&plugin.manifest.id);
        fs::create_dir_all(&destination)?;
        fs::copy(&types, destination.join("index.d.ts"))?;
    }
    Ok(())
}

fn install(template: &Path, root: &Path) -> io::Result<()> {
    let stamp = fs::read(template.join(STAMP)).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "TypeScript support files are missing from the Celesta runtime ({}): {error}",
                template.display()
            ),
        )
    })?;
    let target = root.join(PROJECT_TYPES_DIR);
    fs::create_dir_all(&target)?;
    match fs::remove_file(target.join(STAMP)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let modules = target.join("node_modules");
    if modules.exists() {
        fs::remove_dir_all(&modules)?;
    }
    copy_dir(&template.join("node_modules"), &modules)?;
    fs::copy(template.join("tsconfig.json"), target.join("tsconfig.json"))?;
    fs::write(
        target.join(".gitignore"),
        "# Generated by Celesta and refreshed when Celesta updates.\n*\n",
    )?;
    install_plugin_types(&target, &installed_plugins())?;
    fs::write(target.join(STAMP), stamp)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let destination = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A new temporary folder, at `.0`, removed with everything in it when
    /// this drops.
    pub(crate) struct Scratch(
        pub(crate) PathBuf,
        #[expect(dead_code, reason = "held only to remove the folder when it drops")]
        tempfile::TempDir,
    );

    impl Scratch {
        pub(crate) fn new(name: &str) -> Self {
            let folder = tempfile::Builder::new()
                .prefix(&format!("celesta-project-types-{name}-"))
                .tempdir()
                .unwrap();
            Self(folder.path().to_owned(), folder)
        }

        /// A scratch folder that no `celesta.config.json` above puts inside
        /// another project, for a test of an entry without one: its project
        /// root is then its own folder, inside the scratch folder.
        ///
        /// # Panics
        ///
        /// When a folder above holds one: a stray `celesta.config.json` there
        /// is a mistake on the machine, not a reason to skip.
        pub(crate) fn unmarked(name: &str) -> Self {
            let scratch = Self::new(name);
            if let Some(marked) = scratch
                .0
                .ancestors()
                .skip(1)
                .find(|folder| celesta_plugin_host::has_project_config(folder))
            {
                panic!(
                    "{} holds celesta.config.json, so every temporary folder is inside \
                     that project; remove the file or point TMPDIR elsewhere",
                    marked.display()
                );
            }
            scratch
        }
    }

    pub(crate) fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    pub(crate) fn template(root: &Path, stamp: &str) -> PathBuf {
        let template = root.join("runtime/dist/project-types");
        write(&template.join(STAMP), stamp);
        write(&template.join("tsconfig.json"), "{}");
        write(
            &template.join("node_modules/@celesta.studio/react/dist/index.d.ts"),
            stamp,
        );
        template
    }

    #[test]
    fn copies_plugin_declarations_where_the_tsconfig_maps_them() {
        let scratch = Scratch::new("plugins");
        let plugin = scratch.0.join("installed/example");
        write(&plugin.join("js/index.d.ts"), "export declare const x: 1;");
        let entry = PluginEntry {
            manifest: celesta_plugin_host::Manifest {
                id: "example".into(),
                name: None,
                version: "0.1.0".into(),
                protocol: 1,
                celesta: Some("*".into()),
                executable: Default::default(),
                module: Some("js/index.js".into()),
                types: Some("js/index.d.ts".into()),
                components: Vec::new(),
                roles: vec![celesta_plugin_host::PluginRole::Content],
                media: None,
            },
            directory: plugin,
            source: celesta_plugin_host::PluginSource::Explicit,
        };
        let target = scratch.0.join("project/.celesta");
        write(&target.join("plugins/old/index.d.ts"), "");

        install_plugin_types(&target, std::slice::from_ref(&entry)).unwrap();

        assert_eq!(
            fs::read_to_string(target.join("plugins/example/index.d.ts")).unwrap(),
            "export declare const x: 1;"
        );
        // A plugin no longer installed loses its declarations.
        assert!(!target.join("plugins/old").exists());
    }

    #[test]
    fn template_sits_beside_the_cli_script() {
        assert_eq!(
            project_types_template(Path::new("Resources/react/dist/cli.js")),
            Path::new("Resources/react/dist/project-types")
        );
    }

    #[test]
    fn set_up_installs_support_files_and_creates_a_tsconfig() {
        let scratch = Scratch::unmarked("create");
        let template = template(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(&project.join("main.tsx"), "");

        let setup = set_up_project_types(&template, &project.join("main.tsx")).unwrap();

        assert_eq!(
            setup,
            ProjectTypesSetup {
                root: project.clone(),
                tsconfig: ProjectTsconfig::Created,
            }
        );
        let support = project.join(PROJECT_TYPES_DIR);
        assert_eq!(fs::read_to_string(support.join(STAMP)).unwrap(), "v1");
        assert!(support.join(".gitignore").is_file());
        assert!(support.join("tsconfig.json").is_file());
        assert!(
            support
                .join("node_modules/@celesta.studio/react/dist/index.d.ts")
                .is_file()
        );
        assert!(
            fs::read_to_string(project.join("tsconfig.json"))
                .unwrap()
                .contains(EXTENDS)
        );
    }

    #[test]
    fn set_up_uses_the_folder_with_project_settings_and_keeps_its_tsconfig() {
        let scratch = Scratch::new("existing");
        let template = template(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(&project.join(celesta_plugin_host::PROJECT_CONFIG), "{}");
        write(
            &project.join("tsconfig.json"),
            "{ \"compilerOptions\": {} }",
        );
        write(&project.join("src/main.tsx"), "");

        let setup = set_up_project_types(&template, &project.join("src/main.tsx")).unwrap();

        assert_eq!(setup.root, project);
        assert_eq!(setup.tsconfig, ProjectTsconfig::MissingExtends);
        assert_eq!(
            fs::read_to_string(project.join("tsconfig.json")).unwrap(),
            "{ \"compilerOptions\": {} }"
        );
        assert!(!project.join("src").join(PROJECT_TYPES_DIR).exists());
    }

    #[test]
    fn set_up_does_not_climb_to_a_package_or_tsconfig_above_the_entry() {
        let scratch = Scratch::unmarked("package-above");
        let template = template(&scratch.0, "v1");
        // As in a home folder that happens to hold these files.
        write(&scratch.0.join("package.json"), "{}");
        write(&scratch.0.join("tsconfig.json"), "{}");
        let project = scratch.0.join("video");
        write(&project.join("main.tsx"), "");

        let setup = set_up_project_types(&template, &project.join("main.tsx")).unwrap();

        assert_eq!(setup.root, project);
        assert!(project.join(PROJECT_TYPES_DIR).join(STAMP).is_file());
        assert!(!scratch.0.join(PROJECT_TYPES_DIR).exists());
        assert_eq!(
            fs::read_to_string(scratch.0.join("tsconfig.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn set_up_folder_installs_without_an_entry_and_marks_the_root() {
        let scratch = Scratch::new("folder");
        let old = template(&scratch.0.join("old"), "v1");
        let project = scratch.0.join("video");
        // A package root above the chosen folder must not redirect it.
        write(&scratch.0.join("package.json"), "{}");
        fs::create_dir_all(&project).unwrap();

        let setup = set_up_project_folder(&old, &project).unwrap();

        assert_eq!(
            setup,
            ProjectTypesSetup {
                root: project.clone(),
                tsconfig: ProjectTsconfig::Created,
            }
        );
        assert!(project.join(PROJECT_TYPES_DIR).join(STAMP).is_file());
        assert!(
            fs::read_to_string(project.join("tsconfig.json"))
                .unwrap()
                .contains(EXTENDS)
        );
        // The chosen folder is marked as the project root.
        assert!(celesta_plugin_host::has_project_config(&project));

        // An entry written in a subfolder later is kept up to date.
        let new = template(&scratch.0.join("new"), "v2");
        assert_eq!(
            refresh_project_types(&new, &project.join("src/main.tsx"), &[]).unwrap(),
            Some(project.clone())
        );
    }

    #[test]
    fn set_up_in_rejects_a_missing_folder() {
        let scratch = Scratch::new("missing-folder");
        let template = template(&scratch.0, "v1");
        let error = set_up_project_types_in(&template, &scratch.0.join("nope")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn refresh_updates_only_projects_set_up_by_another_build() {
        let scratch = Scratch::unmarked("refresh");
        let old = template(&scratch.0.join("old"), "v1");
        let new = template(&scratch.0.join("new"), "v2");
        let project = scratch.0.join("video");
        let entry = project.join("src/main.tsx");
        write(&entry, "");

        assert_eq!(refresh_project_types(&new, &entry, &[]).unwrap(), None);
        assert!(!project.join(PROJECT_TYPES_DIR).exists());

        // Without project settings the entry's own folder is the root.
        let root = project.join("src");
        set_up_project_types(&old, &entry).unwrap();
        write(
            &root.join(PROJECT_TYPES_DIR).join("node_modules/stale.d.ts"),
            "",
        );
        assert_eq!(refresh_project_types(&old, &entry, &[]).unwrap(), None);
        assert_eq!(
            refresh_project_types(&new, &entry, &[]).unwrap(),
            Some(root.clone())
        );

        let support = root.join(PROJECT_TYPES_DIR);
        assert_eq!(fs::read_to_string(support.join(STAMP)).unwrap(), "v2");
        assert_eq!(
            fs::read_to_string(support.join("node_modules/@celesta.studio/react/dist/index.d.ts"))
                .unwrap(),
            "v2"
        );
        assert!(!support.join("node_modules/stale.d.ts").exists());
    }

    #[test]
    fn refresh_leaves_support_files_above_the_project_root_alone() {
        let scratch = Scratch::unmarked("above");
        let new = template(&scratch.0.join("new"), "v2");
        // Support files another project, or a stray setup, left in a folder
        // above the entry's project root.
        let above = scratch.0.join("home");
        write(&above.join(PROJECT_TYPES_DIR).join(STAMP), "v1");
        let entry = above.join("video/main.tsx");
        write(&entry, "");

        assert_eq!(refresh_project_types(&new, &entry, &[]).unwrap(), None);
        assert_eq!(
            fs::read_to_string(above.join(PROJECT_TYPES_DIR).join(STAMP)).unwrap(),
            "v1"
        );
        assert!(!above.join(PROJECT_TYPES_DIR).join("plugins").exists());
    }

    #[test]
    fn refresh_without_a_template_leaves_the_project_alone() {
        let scratch = Scratch::new("no-template");
        let project = scratch.0.join("video");
        write(&project.join(PROJECT_TYPES_DIR).join(STAMP), "v1");
        let missing = scratch.0.join("missing");
        assert_eq!(
            refresh_project_types(&missing, &project.join("main.tsx"), &[]).unwrap(),
            None
        );
    }
}
