//! Installs and refreshes a project's `.celesta/` TypeScript support
//! directory from the template staged beside the runtime's `cli.js`
//! (`packages/cli/scripts/stage-project-types.mjs`).
//!
//! Entries always run against the bundled `react` and `@celesta/*`, so
//! the project only needs matching declarations for editors and `tsc`.
//! Copying them from the running Celesta keeps them in step with the runtime
//! without a package registry or an install step.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The directory created at a project root.
pub const PROJECT_TYPES_DIR: &str = ".celesta";

const TEMPLATE_DIR: &str = "project-types";
/// Content stamp written last, so an interrupted copy is redone later.
const STAMP: &str = "version.json";
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

/// Initializes a React project, preserving existing source and configuration.
/// Only `.celesta/` is regenerated. Dependencies are installed separately by
/// the project's package manager; Celesta's runtime packages are bundled.
pub fn initialize_project(template: &Path, root: &Path) -> io::Result<ProjectTypesSetup> {
    // Check the runtime before creating anything in the destination.
    fs::read(template.join(STAMP))?;
    fs::create_dir_all(root)?;
    let setup = set_up_project_types_in(template, root)?;
    write_new(
        &setup.root.join("package.json"),
        r#"{
  "private": true,
  "scripts": {
    "preview": "celesta-editor film.tsx",
    "export": "celesta-exporter --react --overwrite film.tsx output.mp4",
    "typecheck": "tsc --project tsconfig.json"
  },
  "devDependencies": {
    "typescript": "^7.0.2"
  }
}
"#,
    )?;
    write_new(
        &setup.root.join("film.tsx"),
        include_str!("../templates/film.tsx"),
    )?;
    write_new(
        &setup.root.join(".gitignore"),
        "node_modules/\noutput.mp4\n",
    )?;
    Ok(setup)
}

fn write_new(path: &Path, contents: &str) -> io::Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file.write_all(contents.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_file() => Ok(()),
        Err(error) => Err(error),
    }
}

/// Installs `.celesta/` for `entry`'s project and writes a root
/// `tsconfig.json` when there is none. The root is the nearest ancestor of
/// the entry holding a `tsconfig.json` or `package.json`, else its directory.
pub fn set_up_project_types(template: &Path, entry: &Path) -> io::Result<ProjectTypesSetup> {
    let directory = entry_directory(entry)?;
    let root = directory
        .ancestors()
        .find(|dir| dir.join("tsconfig.json").is_file() || dir.join("package.json").is_file())
        .unwrap_or(&directory)
        .to_owned();
    set_up_project_types_in(template, &root)
}

/// Installs `.celesta/` directly in `root` and writes a `tsconfig.json`
/// there when there is none — for a project that has no entry yet, so its
/// first component is written with types already in place. A later entry
/// anywhere under `root` is refreshed by [`refresh_project_types`].
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
/// different Celesta build. Returns the refreshed root; projects that never
/// opted in, or a runtime without a template, are left untouched.
pub fn refresh_project_types(template: &Path, entry: &Path) -> io::Result<Option<PathBuf>> {
    let Ok(expected) = fs::read(template.join(STAMP)) else {
        return Ok(None);
    };
    let directory = entry_directory(entry)?;
    let Some(root) = directory
        .ancestors()
        .find(|dir| dir.join(PROJECT_TYPES_DIR).join(STAMP).is_file())
    else {
        return Ok(None);
    };
    if fs::read(root.join(PROJECT_TYPES_DIR).join(STAMP))? == expected {
        return Ok(None);
    }
    install(template, root)?;
    Ok(Some(root.to_owned()))
}

fn entry_directory(entry: &Path) -> io::Result<PathBuf> {
    let entry = std::path::absolute(entry)?;
    Ok(entry.parent().map_or_else(|| entry.clone(), Path::to_owned))
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
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "celesta-project-types-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn template(root: &Path, stamp: &str) -> PathBuf {
        let template = root.join("runtime/dist/project-types");
        write(&template.join(STAMP), stamp);
        write(&template.join("tsconfig.json"), "{}");
        write(
            &template.join("node_modules/@celesta/react/dist/index.d.ts"),
            stamp,
        );
        template
    }

    #[test]
    fn template_sits_beside_the_cli_script() {
        assert_eq!(
            project_types_template(Path::new("Resources/react/dist/cli.js")),
            Path::new("Resources/react/dist/project-types")
        );
    }

    #[test]
    fn initialize_creates_a_project_and_refreshes_only_generated_files() {
        let scratch = Scratch::new("initialize");
        let old = template(&scratch.0.join("old"), "v1");
        let new = template(&scratch.0.join("new"), "v2");
        let project = scratch.0.join("new/nested/video");
        // Initializing a subdirectory must not use an ancestor's package root.
        write(&scratch.0.join("package.json"), "{}");
        let setup = initialize_project(&old, &project).unwrap();
        assert_eq!(setup.root, project);
        assert_eq!(setup.tsconfig, ProjectTsconfig::Created);
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(project.join("package.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["private"], true);
        assert_eq!(manifest["devDependencies"]["typescript"], "^7.0.2");
        assert!(manifest.get("dependencies").is_none());
        assert!(
            fs::read_to_string(project.join("film.tsx"))
                .unwrap()
                .contains("<Composition")
        );

        let files = [
            (
                "package.json",
                "{\"dependencies\":{\"ag-psd\":\"^31.0.2\"},\"scripts\":{\"prepare\":\"node prepare-assets.ts\"}}",
            ),
            (
                "tsconfig.json",
                "{\"extends\":\"./.celesta/tsconfig.json\",\"compilerOptions\":{\"strict\":false}}",
            ),
            ("film.tsx", "// My composition\n"),
            (".gitignore", "custom-output/\n"),
        ];
        for (name, contents) in files {
            write(&project.join(name), contents);
        }
        initialize_project(&new, &project).unwrap();
        initialize_project(&new, &project).unwrap();
        for (name, contents) in files {
            assert_eq!(fs::read_to_string(project.join(name)).unwrap(), contents);
        }
        assert_eq!(
            fs::read_to_string(project.join(PROJECT_TYPES_DIR).join(STAMP)).unwrap(),
            "v2"
        );
    }

    #[test]
    fn initialize_keeps_an_unrelated_tsconfig_and_rejects_an_unavailable_runtime() {
        let scratch = Scratch::new("initialize-existing");
        let template = template(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(
            &project.join("tsconfig.json"),
            "{\"extends\":\"./custom.json\"}",
        );
        let setup = initialize_project(&template, &project).unwrap();
        assert_eq!(setup.tsconfig, ProjectTsconfig::MissingExtends);
        assert_eq!(
            fs::read_to_string(project.join("tsconfig.json")).unwrap(),
            "{\"extends\":\"./custom.json\"}"
        );
        assert!(project.join("film.tsx").is_file());

        let missing = scratch.0.join("missing-project");
        assert!(initialize_project(&scratch.0.join("missing-runtime"), &missing).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn set_up_installs_support_files_and_creates_a_tsconfig() {
        let scratch = Scratch::new("create");
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
                .join("node_modules/@celesta/react/dist/index.d.ts")
                .is_file()
        );
        assert!(
            fs::read_to_string(project.join("tsconfig.json"))
                .unwrap()
                .contains(EXTENDS)
        );
    }

    #[test]
    fn set_up_uses_the_nearest_package_root_and_keeps_its_tsconfig() {
        let scratch = Scratch::new("existing");
        let template = template(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(&project.join("package.json"), "{}");
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
    }

    #[test]
    fn set_up_in_installs_into_the_chosen_folder_without_an_entry() {
        let scratch = Scratch::new("folder");
        let old = template(&scratch.0.join("old"), "v1");
        let project = scratch.0.join("video");
        // A package root above the chosen folder must not redirect it.
        write(&scratch.0.join("package.json"), "{}");
        fs::create_dir_all(&project).unwrap();

        let setup = set_up_project_types_in(&old, &project).unwrap();

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

        // An entry written there later is kept up to date like any other.
        let new = template(&scratch.0.join("new"), "v2");
        assert_eq!(
            refresh_project_types(&new, &project.join("src/main.tsx")).unwrap(),
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
        let scratch = Scratch::new("refresh");
        let old = template(&scratch.0.join("old"), "v1");
        let new = template(&scratch.0.join("new"), "v2");
        let project = scratch.0.join("video");
        let entry = project.join("src/main.tsx");
        write(&entry, "");

        assert_eq!(refresh_project_types(&new, &entry).unwrap(), None);
        assert!(!project.join(PROJECT_TYPES_DIR).exists());

        // Without a package root the entry's own directory is the root.
        let root = project.join("src");
        set_up_project_types(&old, &entry).unwrap();
        write(
            &root.join(PROJECT_TYPES_DIR).join("node_modules/stale.d.ts"),
            "",
        );
        assert_eq!(refresh_project_types(&old, &entry).unwrap(), None);
        assert_eq!(
            refresh_project_types(&new, &entry).unwrap(),
            Some(root.clone())
        );

        let support = root.join(PROJECT_TYPES_DIR);
        assert_eq!(fs::read_to_string(support.join(STAMP)).unwrap(), "v2");
        assert_eq!(
            fs::read_to_string(support.join("node_modules/@celesta/react/dist/index.d.ts"))
                .unwrap(),
            "v2"
        );
        assert!(!support.join("node_modules/stale.d.ts").exists());
    }

    #[test]
    fn refresh_without_a_template_leaves_the_project_alone() {
        let scratch = Scratch::new("no-template");
        let project = scratch.0.join("video");
        write(&project.join(PROJECT_TYPES_DIR).join(STAMP), "v1");
        let missing = scratch.0.join("missing");
        assert_eq!(
            refresh_project_types(&missing, &project.join("main.tsx")).unwrap(),
            None
        );
    }
}
