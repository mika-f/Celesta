//! Writes one property into a JSON props file, leaving the rest of the file's
//! top-level keys in their original order.
//!
//! The editor's Properties section edits the file the user (or another
//! editor) also touches, so every edit re-reads the file, changes a single
//! key and writes the whole file back through a temporary file in the same
//! folder. A file that is not a JSON object is never written.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A JSON object's entries in the order they appear in the text.
///
/// `serde_json::Map` sorts keys unless its `preserve_order` feature is on,
/// which would reshuffle a hand-written props file on the first edit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderedObject(pub Vec<(String, Value)>);

impl<'de> Deserialize<'de> for OrderedObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;

        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = OrderedObject;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(entry) = map.next_entry::<String, Value>()? {
                    entries.push(entry);
                }
                Ok(OrderedObject(entries))
            }
        }

        deserializer.deserialize_map(ObjectVisitor)
    }
}

impl Serialize for OrderedObject {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PropsEditError {
    /// The file could not be read or written.
    #[error("Couldn’t write {file}: {source}")]
    Io { file: PathBuf, source: io::Error },
    /// The file is not a JSON object, so it was left untouched.
    #[error("Fix {} before editing: {message}", file_name(.file))]
    Invalid { file: PathBuf, message: String },
}

/// `file`'s name, or the whole path when it has none.
fn file_name(file: &Path) -> String {
    file.file_name().map_or_else(
        || file.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Sets `key` to `value`, or removes it for `None`. A missing file counts as
/// `{}`, so the first edit creates it. An existing key keeps its position and
/// a new key is appended; every other top-level key keeps its order.
pub fn set_property(file: &Path, key: &str, value: Option<&Value>) -> Result<(), PropsEditError> {
    let io_error = |source| PropsEditError::Io {
        file: file.to_owned(),
        source,
    };

    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(io_error(error)),
    };
    let text = set_property_text(file, &text, key, value)?;

    // Renaming over a read-only file would succeed, so refuse it explicitly.
    if fs::metadata(file).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(io_error(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the file is read-only",
        )));
    }
    replace_file(file, &text).map_err(io_error)
}

/// The props file text `text` (the content of `file`) becomes once `key` is set
/// to `value`, or removed for `None`. Empty text counts as `{}`. `file` only
/// names the file in an error.
pub fn set_property_text(
    file: &Path,
    text: &str,
    key: &str,
    value: Option<&Value>,
) -> Result<String, PropsEditError> {
    let invalid = |message: String| PropsEditError::Invalid {
        file: file.to_owned(),
        message,
    };

    let mut object = if text.is_empty() {
        OrderedObject::default()
    } else {
        serde_json::from_str::<OrderedObject>(text).map_err(|error| invalid(error.to_string()))?
    };

    let position = object.0.iter().position(|(name, _)| name == key);
    match (position, value) {
        (Some(index), Some(value)) => object.0[index].1 = value.clone(),
        (None, Some(value)) => object.0.push((key.to_owned(), value.clone())),
        (Some(index), None) => {
            object.0.remove(index);
        }
        (None, None) => {}
    }

    let mut text =
        serde_json::to_string_pretty(&object).map_err(|error| invalid(error.to_string()))?;
    text.push('\n');
    Ok(text)
}

/// Writes `text` to a temporary file beside `file` and renames it over `file`,
/// so a reader never sees a half-written props file.
pub fn replace_file(file: &Path, text: &str) -> io::Result<()> {
    let temporary = write_temp(file, text)?;
    let result = fs::rename(&temporary, file);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Writes `text` to a temporary file beside `file`, named
/// `.<file name>.tmp-<process id>`, and returns its path for the caller to
/// rename over `file`. Removes the temporary file again when the write fails.
pub fn write_temp(file: &Path, text: &str) -> io::Result<PathBuf> {
    let name = file
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?
        .to_string_lossy();
    let temporary = file.with_file_name(format!(".{name}.tmp-{}", std::process::id()));

    let written = fs::File::create(&temporary).and_then(|mut handle| {
        handle.write_all(text.as_bytes())?;
        handle.sync_all()
    });
    match written {
        Ok(()) => Ok(temporary),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// The value to store for `chosen` in a props file in `props_dir`. A file
/// under `project_root` is referenced in place; any other file is copied
/// into `<project_root>/assets/` first. Always `/`-separated, starting with
/// `./` or `../`.
pub fn import_path(project_root: &Path, props_dir: &Path, chosen: &Path) -> io::Result<String> {
    // Resolve `..` segments and symlinks first, so the three paths share one
    // frame of reference whichever way each of them was spelled.
    let project_root = resolve(project_root)?;
    let props_dir = resolve(props_dir)?;
    let chosen = resolve(chosen)?;

    if !chosen.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a file", chosen.display()),
        ));
    }

    let target = if chosen.strip_prefix(&project_root).is_ok() {
        chosen
    } else {
        copy_into_assets(&project_root.join("assets"), &chosen)?
    };

    relative_path(&props_dir, &target)
}

/// `path` made absolute with `..` segments and symlinks resolved. A folder that
/// does not exist yet (such as a props folder about to be created) keeps its
/// missing tail, appended to its nearest existing ancestor.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    if let Ok(canonical) = fs::canonicalize(&absolute) {
        return Ok(canonical);
    }

    let mut missing = Vec::new();
    let mut existing = absolute.as_path();
    while let Some(parent) = existing.parent() {
        if let Some(name) = existing.file_name() {
            missing.push(name);
        }
        existing = parent;
        if let Ok(mut canonical) = fs::canonicalize(existing) {
            canonical.extend(missing.iter().rev());
            return Ok(canonical);
        }
    }
    Ok(absolute)
}

/// Copies `chosen` into `assets` and returns the copy's path. A file of the
/// same name with the same bytes is reused; one with different bytes makes the
/// copy take the next free `name-2.ext`, `name-3.ext`, ... name.
fn copy_into_assets(assets: &Path, chosen: &Path) -> io::Result<PathBuf> {
    let name_error = || io::Error::new(io::ErrorKind::InvalidInput, "the file has no name");
    let stem = chosen.file_stem().ok_or_else(name_error)?.to_string_lossy();
    let extension = chosen
        .extension()
        .map(|extension| extension.to_string_lossy());
    let bytes = fs::read(chosen)?;

    fs::create_dir_all(assets)?;
    for number in 1_u32.. {
        let name = match (number, &extension) {
            (1, Some(extension)) => format!("{stem}.{extension}"),
            (1, None) => stem.to_string(),
            (number, Some(extension)) => format!("{stem}-{number}.{extension}"),
            (number, None) => format!("{stem}-{number}"),
        };
        let candidate = assets.join(name);

        if candidate.symlink_metadata().is_err() {
            fs::write(&candidate, &bytes)?;
            return Ok(candidate);
        }
        if fs::read(&candidate).is_ok_and(|existing| existing == bytes) {
            return Ok(candidate);
        }
    }
    unreachable!("the candidate numbers are unbounded")
}

/// `target` as seen from `from`, joined with `/` and starting with `./` or
/// `../`. Both paths must be absolute.
fn relative_path(from: &Path, target: &Path) -> io::Result<String> {
    let from: Vec<_> = from.components().collect();
    let target: Vec<_> = target.components().collect();
    let common = from
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the paths have no folder in common",
        ));
    }

    let mut parts = vec![".."; from.len() - common]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    parts.extend(
        target[common..]
            .iter()
            .map(|component| component.as_os_str().to_string_lossy().into_owned()),
    );

    let joined = parts.join("/");
    Ok(if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_types::tests::{Scratch, write};
    use serde_json::json;
    use std::fs;

    fn text(path: &std::path::Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn keeps_key_order_and_appends_new_keys() {
        let scratch = Scratch::new("props-order");
        let file = scratch.0.join("props.json");
        write(&file, "{\"b\":1,\"a\":2}");

        set_property(&file, "c", Some(&json!(3))).unwrap();

        assert_eq!(text(&file), "{\n  \"b\": 1,\n  \"a\": 2,\n  \"c\": 3\n}\n");
    }

    #[test]
    fn replaces_a_value_in_place() {
        let scratch = Scratch::new("props-replace");
        let file = scratch.0.join("props.json");
        write(&file, "{\"b\":1,\"a\":2}");

        set_property(&file, "a", Some(&json!(5))).unwrap();

        assert_eq!(text(&file), "{\n  \"b\": 1,\n  \"a\": 5\n}\n");
    }

    #[test]
    fn removes_a_key_and_keeps_an_empty_object() {
        let scratch = Scratch::new("props-remove");
        let file = scratch.0.join("props.json");
        write(&file, "{\"a\":1}");

        set_property(&file, "a", None).unwrap();

        assert_eq!(text(&file), "{}\n");
        assert!(file.is_file());
    }

    #[test]
    fn creates_a_missing_file() {
        let scratch = Scratch::new("props-create");
        let folder = scratch.0.join("日本語 project");
        fs::create_dir_all(&folder).unwrap();
        let file = folder.join("props.json");

        set_property(&file, "title", Some(&json!("こんにちは"))).unwrap();

        assert_eq!(text(&file), "{\n  \"title\": \"こんにちは\"\n}\n");
    }

    #[test]
    fn keeps_keys_written_by_another_editor() {
        let scratch = Scratch::new("props-external");
        let file = scratch.0.join("props.json");
        set_property(&file, "a", Some(&json!(1))).unwrap();
        fs::write(&file, "{\"a\":1,\"z\":9}").unwrap();

        set_property(&file, "b", Some(&json!(2))).unwrap();

        assert_eq!(text(&file), "{\n  \"a\": 1,\n  \"z\": 9,\n  \"b\": 2\n}\n");
    }

    #[test]
    fn refuses_to_write_a_broken_file() {
        let scratch = Scratch::new("props-broken");
        let file = scratch.0.join("props.json");
        write(&file, "{\"a\":");

        let error = set_property(&file, "b", Some(&json!(1))).unwrap_err();

        assert!(matches!(error, PropsEditError::Invalid { .. }));
        assert!(
            error
                .to_string()
                .starts_with("Fix props.json before editing: "),
            "{error}"
        );
        assert_eq!(text(&file), "{\"a\":");
    }

    #[test]
    fn refuses_a_non_object() {
        let scratch = Scratch::new("props-array");
        let file = scratch.0.join("props.json");
        write(&file, "[1]");

        let error = set_property(&file, "b", Some(&json!(1))).unwrap_err();

        assert!(matches!(error, PropsEditError::Invalid { .. }));
        assert_eq!(text(&file), "[1]");
    }

    #[test]
    fn leaves_no_temporary_file() {
        let scratch = Scratch::new("props-temp");
        let file = scratch.0.join("props.json");
        write(&file, "{}");

        set_property(&file, "a", Some(&json!(1))).unwrap();

        let names: Vec<_> = fs::read_dir(&scratch.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("props.json")]);
    }

    #[test]
    fn reports_a_read_only_file() {
        let scratch = Scratch::new("props-readonly");
        let file = scratch.0.join("props.json");
        write(&file, "{\"a\":1}");
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();

        let result = set_property(&file, "b", Some(&json!(2)));

        let mut permissions = fs::metadata(&file).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(&file, permissions).unwrap();
        assert!(
            matches!(result, Err(PropsEditError::Io { .. })),
            "{result:?}"
        );
        assert_eq!(text(&file), "{\"a\":1}");
        let names: Vec<_> = fs::read_dir(&scratch.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("props.json")]);
    }

    fn project(name: &str) -> (Scratch, PathBuf, PathBuf) {
        let scratch = Scratch::new(name);
        let root = scratch.0.join("project");
        let other = scratch.0.join("other");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&other).unwrap();
        (scratch, root, other)
    }

    #[test]
    fn a_file_in_the_project_is_relative_to_the_props_file() {
        let (_scratch, root, _) = project("import-inside");
        let chosen = root.join("assets/a.png");
        write(&chosen, "a");

        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/a.png"
        );
        assert_eq!(
            import_path(&root, &root.join("variants"), &chosen).unwrap(),
            "../assets/a.png"
        );
    }

    #[test]
    fn a_file_in_a_nested_folder_with_spaces_and_japanese() {
        let (_scratch, root, _) = project("import-nested");
        let chosen = root.join("素材 フォルダ/b.png");
        write(&chosen, "b");

        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./素材 フォルダ/b.png"
        );
    }

    #[test]
    fn an_outside_file_is_copied_into_assets() {
        let (_scratch, root, other) = project("import-copy");
        let chosen = other.join("c.png");
        write(&chosen, "c");

        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/c.png"
        );
        assert_eq!(fs::read(root.join("assets/c.png")).unwrap(), b"c");
    }

    #[test]
    fn identical_content_is_reused() {
        let (_scratch, root, other) = project("import-reuse");
        let chosen = other.join("c.png");
        write(&chosen, "c");

        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/c.png"
        );
        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/c.png"
        );
        assert!(!root.join("assets/c-2.png").exists());
    }

    #[test]
    fn different_content_gets_a_number() {
        let (_scratch, root, other) = project("import-number");
        write(&root.join("assets/c.png"), "first");
        let chosen = other.join("c.png");
        write(&chosen, "second");

        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/c-2.png"
        );

        write(&chosen, "third");
        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/c-3.png"
        );
        assert_eq!(fs::read(root.join("assets/c.png")).unwrap(), b"first");
        assert_eq!(fs::read(root.join("assets/c-2.png")).unwrap(), b"second");
        assert_eq!(fs::read(root.join("assets/c-3.png")).unwrap(), b"third");
    }

    #[test]
    fn a_file_without_an_extension_gets_a_number() {
        let (_scratch, root, other) = project("import-no-extension");
        write(&root.join("assets/data"), "first");
        let chosen = other.join("data");
        write(&chosen, "second");

        assert_eq!(
            import_path(&root, &root, &chosen).unwrap(),
            "./assets/data-2"
        );
    }

    #[test]
    fn a_props_folder_with_a_parent_segment_is_resolved() {
        let (_scratch, root, _) = project("import-dotdot");
        let chosen = root.join("assets/a.png");
        write(&chosen, "a");
        fs::create_dir_all(root.join("variants")).unwrap();
        let props_dir = root.join("../other/../project/variants");

        assert_eq!(
            import_path(&root, &props_dir, &chosen).unwrap(),
            "../assets/a.png"
        );
        assert!(!root.join("assets/a-2.png").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_project_reached_through_a_symlink_is_not_copied() {
        let (scratch, root, _) = project("import-symlink");
        let chosen = root.join("assets/a.png");
        write(&chosen, "a");
        let link = scratch.0.join("link");
        std::os::unix::fs::symlink(&root, &link).unwrap();

        // The project root is the real folder; the other two go through the link.
        assert_eq!(
            import_path(&root, &link, &link.join("assets/a.png")).unwrap(),
            "./assets/a.png"
        );
        assert_eq!(
            import_path(&link, &root.join("variants"), &chosen).unwrap(),
            "../assets/a.png"
        );
        assert_eq!(fs::read_dir(root.join("assets")).unwrap().count(), 1);
    }

    #[test]
    fn a_folder_is_not_a_file() {
        let (_scratch, root, other) = project("import-folder");

        let error = import_path(&root, &root, &other).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!root.join("assets").exists());
    }
}
