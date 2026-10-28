//! Creates a project from a template staged beside the runtime's `cli.js`
//! (`packages/cli/scripts/stage-templates.mjs`).
//!
//! Each template is a folder with a `template.json` (shown to the user, never
//! copied) and the files a new project starts with. The project's `.celesta/`
//! support directory and the files every project needs (`tsconfig.json`,
//! `package.json`, `.gitignore`) are not part of a template; they are always
//! created when missing and never overwritten.

use crate::project_types::{
    ProjectTypesSetup, STAMP, project_types_template, set_up_project_types_in,
};
use serde::Deserialize;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The template used when none is chosen.
pub const DEFAULT_TEMPLATE: &str = "blank";

const TEMPLATES_DIR: &str = "templates";
/// Template metadata, read for the chooser and never copied into a project.
const METADATA: &str = "template.json";

/// The templates staged next to `cli_script`.
pub fn project_templates_dir(cli_script: &Path) -> PathBuf {
    cli_script
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(TEMPLATES_DIR)
}

/// What a template's `template.json` says about it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TemplateInfo {
    /// The template's folder name.
    #[serde(skip)]
    pub name: String,
    pub title: String,
    pub description: String,
    /// What to do after creating the project.
    pub edit: String,
}

/// The templates in `dir`, `blank` first and the rest by name. Folders
/// without a parseable `template.json` are skipped.
pub fn list_templates(dir: &Path) -> Result<Vec<TemplateInfo>, TemplateError> {
    let entries = fs::read_dir(dir).map_err(|source| TemplateError::Missing {
        dir: dir.to_owned(),
        source,
    })?;
    let mut templates = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(text) = fs::read_to_string(entry.path().join(METADATA)) else {
            continue;
        };
        if let Ok(info) = serde_json::from_str::<TemplateInfo>(&text) {
            templates.push(TemplateInfo { name, ..info });
        }
    }
    templates.sort_by(|a, b| {
        (a.name != DEFAULT_TEMPLATE, &a.name).cmp(&(b.name != DEFAULT_TEMPLATE, &b.name))
    });
    Ok(templates)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSetup {
    pub types: ProjectTypesSetup,
    pub template: TemplateInfo,
}

/// Initializes a React project, preserving existing source and configuration.
/// Only `.celesta/` is regenerated. Dependencies are installed separately by
/// the project's package manager; Celesta's runtime packages are bundled.
///
/// `template: None` is `blank`, keeping every file that already exists.
/// `Some(name)` refuses to write anything when a file the template would
/// create already exists. Either way nothing is created in `root` before the
/// runtime and the template have been checked.
pub fn initialize_project(
    cli_script: &Path,
    root: &Path,
    template: Option<&str>,
) -> Result<ProjectSetup, TemplateError> {
    let types = project_types_template(cli_script);
    fs::read(types.join(STAMP))?;

    let name = template.unwrap_or(DEFAULT_TEMPLATE);
    let templates = project_templates_dir(cli_script);
    let available = list_templates(&templates)?;
    let Some(info) = available.iter().find(|info| info.name == name).cloned() else {
        return Err(TemplateError::Unknown {
            name: name.to_owned(),
            available: available.into_iter().map(|info| info.name).collect(),
        });
    };

    let source = templates.join(&info.name);
    let mut files = Vec::new();
    collect_files(&source, Path::new(""), &mut files)?;
    files.retain(|file| file != Path::new(METADATA));
    files.sort();

    let keep_existing = template.is_none();
    if !keep_existing {
        let conflicts: Vec<_> = files
            .iter()
            .filter(|file| root.join(file).exists())
            .cloned()
            .collect();
        if !conflicts.is_empty() {
            return Err(TemplateError::Conflicts {
                root: std::path::absolute(root)?,
                paths: conflicts,
            });
        }
    }

    fs::create_dir_all(root)?;
    let setup = set_up_project_types_in(&types, root)?;
    for file in &files {
        let destination = setup.root.join(file);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_new(&source.join(file), &destination, keep_existing)?;
    }
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
        &setup.root.join(".gitignore"),
        "node_modules/\noutput.mp4\n",
    )?;
    Ok(ProjectSetup {
        types: setup,
        template: info,
    })
}

/// Every file under `dir`, as paths relative to the template folder.
fn collect_files(dir: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            collect_files(&entry.path(), &path, files)?;
        } else if !is_junk_file(&entry.file_name()) {
            files.push(path);
        }
    }
    Ok(())
}

/// OS and editor leftovers that never belong to a template: Finder and
/// Explorer metadata, AppleDouble files, editor backups and vim swap files.
/// Keep in step with `isJunkFile` in `packages/cli/scripts/stage-templates.mjs`.
fn is_junk_file(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let bytes = name.as_bytes();
    matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini")
        || name.starts_with("._")
        || name.ends_with('~')
        || (name.starts_with('.')
            && bytes.len() >= 4
            && &bytes[bytes.len() - 4..bytes.len() - 1] == b".sw")
}

/// Copies `from` to `to` without replacing a file that is already there.
/// With `keep_existing` an existing file is left alone; otherwise it is an
/// error.
fn copy_new(from: &Path, to: &Path, keep_existing: bool) -> io::Result<()> {
    match fs::OpenOptions::new().write(true).create_new(true).open(to) {
        Ok(mut file) => io::copy(&mut fs::File::open(from)?, &mut file).map(drop),
        Err(error)
            if keep_existing && error.kind() == io::ErrorKind::AlreadyExists && to.is_file() =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
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

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// The runtime has no `templates/` folder.
    #[error("Template files are missing from the Celesta runtime ({dir}): {source}")]
    Missing { dir: PathBuf, source: io::Error },
    #[error("Unknown template `{name}`; available: {}", .available.join(", "))]
    Unknown {
        name: String,
        available: Vec<String>,
    },
    /// Files the template would create that already exist, relative to
    /// `root` (made absolute) and sorted.
    #[error("{root} already has files from this template: {}", template_paths(.paths))]
    Conflicts { root: PathBuf, paths: Vec<PathBuf> },
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// `paths` joined by commas, each read the same on every platform:
/// `assets/a.png`.
fn template_paths(paths: &[PathBuf]) -> String {
    let paths: Vec<_> = paths
        .iter()
        .map(|path| {
            path.components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    paths.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_types::tests::{Scratch, template, write};
    use crate::project_types::{PROJECT_TYPES_DIR, ProjectTsconfig, STAMP};
    use std::fs;

    const BLANK_FILM: &str = "export default () => <Composition />;\n";

    fn metadata(title: &str) -> String {
        format!(r#"{{"title":"{title}","description":"About {title}.","edit":"Edit {title}."}}"#)
    }

    /// A runtime with `cli.js`, `project-types/` and the given templates.
    fn runtime(root: &Path, stamp: &str) -> PathBuf {
        template(root, stamp);
        let cli = root.join("runtime/dist/cli.js");
        write(&cli, "");
        let templates = root.join("runtime/dist/templates");
        write(&templates.join("blank/template.json"), &metadata("Blank"));
        write(&templates.join("blank/film.tsx"), BLANK_FILM);
        write(&templates.join("slides/template.json"), &metadata("Slides"));
        write(&templates.join("slides/film.tsx"), "// slides\n");
        write(&templates.join("slides/props.json"), "{\"title\":\"\"}\n");
        write(&templates.join("slides/assets/a.png"), "png");
        cli
    }

    #[test]
    fn templates_sit_beside_the_cli_script() {
        assert_eq!(
            project_templates_dir(Path::new("Resources/react/dist/cli.js")),
            Path::new("Resources/react/dist/templates")
        );
    }

    #[test]
    fn lists_blank_first_and_skips_folders_without_metadata() {
        let scratch = Scratch::new("tpl-list");
        let cli = runtime(&scratch.0, "v1");
        let templates = project_templates_dir(&cli);
        write(&templates.join("broken/template.json"), "{");
        write(&templates.join("empty/film.tsx"), "");
        write(&templates.join("alpha/template.json"), &metadata("Alpha"));

        let listed = list_templates(&templates).unwrap();

        let names: Vec<_> = listed.iter().map(|info| info.name.as_str()).collect();
        assert_eq!(names, ["blank", "alpha", "slides"]);
        assert_eq!(listed[0].title, "Blank");
        assert_eq!(listed[0].edit, "Edit Blank.");
    }

    #[test]
    fn initialize_without_template_keeps_todays_behaviour() {
        let scratch = Scratch::new("tpl-initialize");
        let old = runtime(&scratch.0.join("old"), "v1");
        let new = runtime(&scratch.0.join("new"), "v2");
        let project = scratch.0.join("new/nested/video");
        // Initializing a subdirectory must not use an ancestor's package root.
        write(&scratch.0.join("package.json"), "{}");
        let setup = initialize_project(&old, &project, None).unwrap();
        assert_eq!(setup.types.root, project);
        assert_eq!(setup.types.tsconfig, ProjectTsconfig::Created);
        assert_eq!(setup.template.name, DEFAULT_TEMPLATE);
        assert_eq!(setup.template.edit, "Edit Blank.");
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(project.join("package.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["private"], true);
        assert_eq!(manifest["devDependencies"]["typescript"], "^7.0.2");
        assert!(manifest.get("dependencies").is_none());
        assert_eq!(
            fs::read_to_string(project.join("film.tsx")).unwrap(),
            BLANK_FILM
        );
        assert_eq!(
            fs::read_to_string(project.join(".gitignore")).unwrap(),
            "node_modules/\noutput.mp4\n"
        );
        assert!(!project.join("template.json").exists());

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
        initialize_project(&new, &project, None).unwrap();
        initialize_project(&new, &project, None).unwrap();
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
        let scratch = Scratch::new("tpl-existing");
        let cli = runtime(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(
            &project.join("tsconfig.json"),
            "{\"extends\":\"./custom.json\"}",
        );
        let setup = initialize_project(&cli, &project, None).unwrap();
        assert_eq!(setup.types.tsconfig, ProjectTsconfig::MissingExtends);
        assert_eq!(
            fs::read_to_string(project.join("tsconfig.json")).unwrap(),
            "{\"extends\":\"./custom.json\"}"
        );
        assert!(project.join("film.tsx").is_file());

        let missing = scratch.0.join("missing-project");
        let unavailable = scratch.0.join("missing-runtime/cli.js");
        assert!(initialize_project(&unavailable, &missing, None).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn template_copies_recursively_without_metadata() {
        let scratch = Scratch::new("tpl-copy");
        let cli = runtime(&scratch.0, "v1");
        let project = scratch.0.join("日本語 project");

        let setup = initialize_project(&cli, &project, Some("slides")).unwrap();

        assert_eq!(setup.template.name, "slides");
        assert_eq!(setup.template.title, "Slides");
        assert_eq!(
            fs::read_to_string(project.join("film.tsx")).unwrap(),
            "// slides\n"
        );
        assert!(project.join("props.json").is_file());
        assert!(project.join("assets/a.png").is_file());
        assert!(!project.join("template.json").exists());
        assert!(project.join("package.json").is_file());
        assert!(project.join(".gitignore").is_file());
    }

    #[test]
    fn template_refuses_every_conflict_and_writes_nothing() {
        let scratch = Scratch::new("tpl-conflicts");
        let cli = runtime(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(&project.join("film.tsx"), "// mine\n");
        write(&project.join("assets/a.png"), "mine");

        let error = initialize_project(&cli, &project, Some("slides")).unwrap_err();

        let TemplateError::Conflicts { root, paths } = &error else {
            panic!("expected conflicts, got {error:?}");
        };
        let project = std::path::absolute(&project).unwrap();
        assert!(root.is_absolute());
        assert_eq!(root, &project);
        assert_eq!(
            paths,
            &[PathBuf::from("assets/a.png"), PathBuf::from("film.tsx")]
        );
        assert_eq!(
            error.to_string(),
            format!(
                "{} already has files from this template: assets/a.png, film.tsx",
                project.display()
            )
        );
        assert!(!project.join("props.json").exists());
        assert!(!project.join(PROJECT_TYPES_DIR).exists());
        assert!(!project.join("package.json").exists());
        assert_eq!(
            fs::read_to_string(project.join("film.tsx")).unwrap(),
            "// mine\n"
        );
    }

    #[test]
    fn junk_files_are_neither_copied_nor_conflicts() {
        let scratch = Scratch::new("tpl-junk");
        let cli = runtime(&scratch.0, "v1");
        let slides = scratch.0.join("runtime/dist/templates/slides");
        for junk in [
            ".DS_Store",
            "assets/.DS_Store",
            "assets/._a.png",
            "Thumbs.db",
            "desktop.ini",
            "film.tsx~",
            ".film.tsx.swp",
        ] {
            write(&slides.join(junk), "junk");
        }
        let project = scratch.0.join("video");
        write(&project.join(".DS_Store"), "mine");

        initialize_project(&cli, &project, Some("slides")).unwrap();

        assert_eq!(
            fs::read_to_string(project.join(".DS_Store")).unwrap(),
            "mine"
        );
        assert!(project.join("assets/a.png").is_file());
        for junk in [
            "assets/.DS_Store",
            "assets/._a.png",
            "Thumbs.db",
            "desktop.ini",
            "film.tsx~",
            ".film.tsx.swp",
        ] {
            assert!(!project.join(junk).exists(), "{junk}");
        }
    }

    #[test]
    fn junk_file_names() {
        for junk in [
            ".DS_Store",
            "._film.tsx",
            "Thumbs.db",
            "desktop.ini",
            "film.tsx~",
            ".film.tsx.swp",
            ".film.tsx.swo",
        ] {
            assert!(is_junk_file(OsStr::new(junk)), "{junk}");
        }
        for kept in [
            "film.tsx",
            ".gitkeep",
            ".swatch",
            "swap.swp",
            "props.json",
            "assets",
        ] {
            assert!(!is_junk_file(OsStr::new(kept)), "{kept}");
        }
    }

    #[test]
    fn template_ignores_unrelated_files() {
        let scratch = Scratch::new("tpl-unrelated");
        let cli = runtime(&scratch.0, "v1");
        let project = scratch.0.join("video");
        write(&project.join(".git/HEAD"), "ref: refs/heads/main\n");
        write(&project.join("notes.txt"), "notes");
        write(&project.join("assets/other.png"), "other");

        initialize_project(&cli, &project, Some("slides")).unwrap();

        assert!(project.join("assets/a.png").is_file());
        assert!(project.join("assets/other.png").is_file());
        assert!(project.join("notes.txt").is_file());
    }

    #[test]
    fn plain_init_after_template_adds_nothing() {
        let scratch = Scratch::new("tpl-plain-after");
        let cli = runtime(&scratch.0, "v1");
        let project = scratch.0.join("video");
        initialize_project(&cli, &project, Some("slides")).unwrap();
        write(&project.join("props.json"), "{\"title\":\"edited\"}");
        let snapshot = |root: &Path| {
            let mut names: Vec<_> = fs::read_dir(root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let before = snapshot(&project);

        initialize_project(&cli, &project, None).unwrap();

        assert_eq!(
            fs::read_to_string(project.join("props.json")).unwrap(),
            "{\"title\":\"edited\"}"
        );
        assert_eq!(
            fs::read_to_string(project.join("film.tsx")).unwrap(),
            "// slides\n"
        );
        assert_eq!(snapshot(&project), before);
    }

    #[test]
    fn unknown_template_lists_the_available_ones() {
        let scratch = Scratch::new("tpl-unknown");
        let cli = runtime(&scratch.0, "v1");
        let project = scratch.0.join("video");

        let error = initialize_project(&cli, &project, Some("nope")).unwrap_err();

        assert!(matches!(
            &error,
            TemplateError::Unknown { name, available }
                if name == "nope" && available == &["blank", "slides"]
        ));
        assert_eq!(
            error.to_string(),
            "Unknown template `nope`; available: blank, slides"
        );
        assert!(!project.exists());
    }

    #[test]
    fn missing_templates_dir_is_reported_before_anything_is_created() {
        let scratch = Scratch::new("tpl-missing");
        let cli = runtime(&scratch.0, "v1");
        fs::remove_dir_all(project_templates_dir(&cli)).unwrap();
        let project = scratch.0.join("video");

        let error = initialize_project(&cli, &project, None).unwrap_err();

        assert!(matches!(error, TemplateError::Missing { .. }));
        assert!(error.to_string().starts_with(&format!(
            "Template files are missing from the Celesta runtime ({}): ",
            project_templates_dir(&cli).display()
        )));
        assert!(!project.exists());
    }
}
