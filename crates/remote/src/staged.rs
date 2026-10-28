//! Files written beside their destination and published over it by one
//! rename, so a reader never sees a half-written file and a failed write
//! leaves nothing behind.

use std::fs::{self, File};
use std::io;
use std::path::Path;

use tempfile::NamedTempFile;

/// A file being written beside its destination. [`StagedFile::publish`]
/// renames it into place; dropping it unpublished removes it.
pub struct StagedFile(NamedTempFile);

impl StagedFile {
    /// Creates `destination`'s directory if needed and an empty file in it,
    /// named `.celesta-<random><suffix>`. The suffix keeps an extension a
    /// writer reads, such as the one FFmpeg picks a container by, or `.tmp`
    /// for a file that must not match a listing of destination files.
    pub fn beside(destination: &Path, suffix: &str) -> io::Result<Self> {
        let directory = match destination.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        fs::create_dir_all(directory)?;
        let mut builder = tempfile::Builder::new();
        builder.prefix(".celesta-").suffix(suffix);
        // The published file gets the permissions `File::create` gives a new
        // file, not a temporary file's owner-only ones.
        #[cfg(unix)]
        builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
        builder.tempfile_in(directory).map(Self)
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }

    pub fn as_file_mut(&mut self) -> &mut File {
        self.0.as_file_mut()
    }

    /// Renames the file to `destination`. With `overwrite` it replaces a file
    /// already there; without it, that file stays and publishing fails with
    /// [`io::ErrorKind::AlreadyExists`]. A failed publish removes the staged
    /// file.
    pub fn publish(self, destination: &Path, overwrite: bool) -> io::Result<()> {
        let published = if overwrite {
            self.0.persist(destination)
        } else {
            self.0.persist_noclobber(destination)
        };
        published.map(drop).map_err(|error| error.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn names(directory: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn publishes_into_a_new_directory() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("nested/out.mp4");
        let mut file = StagedFile::beside(&destination, ".mp4").unwrap();
        assert_eq!(file.path().parent(), destination.parent());
        let name = file.path().file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(".celesta-") && name.ends_with(".mp4"));
        file.as_file_mut().write_all(b"video").unwrap();

        file.publish(&destination, false).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"video");
        assert_eq!(names(destination.parent().unwrap()), ["out.mp4"]);
    }

    #[test]
    fn keeps_an_existing_file_unless_told_to_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("out.txt");
        fs::write(&destination, "old").unwrap();

        let mut file = StagedFile::beside(&destination, ".tmp").unwrap();
        file.as_file_mut().write_all(b"new").unwrap();
        let error = file.publish(&destination, false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "old");
        assert_eq!(names(directory.path()), ["out.txt"]);

        let mut file = StagedFile::beside(&destination, ".tmp").unwrap();
        file.as_file_mut().write_all(b"new").unwrap();
        file.publish(&destination, true).unwrap();
        assert_eq!(fs::read_to_string(&destination).unwrap(), "new");
        assert_eq!(names(directory.path()), ["out.txt"]);
    }

    #[test]
    fn dropping_an_unpublished_file_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        let file = StagedFile::beside(&directory.path().join("out.txt"), ".tmp").unwrap();
        assert!(file.path().exists());
        drop(file);
        assert!(names(directory.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn publishes_with_the_permissions_of_a_created_file() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let created = directory.path().join("created");
        File::create(&created).unwrap();
        let destination = directory.path().join("published");
        StagedFile::beside(&destination, ".tmp")
            .unwrap()
            .publish(&destination, false)
            .unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&destination), mode(&created));
    }
}
