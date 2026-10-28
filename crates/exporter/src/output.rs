//! Export outputs, written beside their destination and published only once
//! whole, so a failed or cancelled export leaves no partial file.

use crate::error::ExportError;
use celesta_remote::StagedFile;
use std::io;
use std::path::Path;

/// Stages a file beside `path`, creating its directory, for an export to
/// write. `suffix` keeps the extension FFmpeg picks a container by.
pub(crate) fn stage(
    path: &Path,
    suffix: &str,
    operation: &'static str,
) -> Result<StagedFile, ExportError> {
    StagedFile::beside(path, suffix).map_err(|source| ExportError::Io { operation, source })
}

/// Publishes `file` at `path`. Without `overwrite`, a file already there
/// stays and the export fails with [`ExportError::OutputExists`].
pub(crate) fn publish(
    file: StagedFile,
    path: &Path,
    overwrite: bool,
    operation: &'static str,
) -> Result<(), ExportError> {
    file.publish(path, overwrite).map_err(|source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            ExportError::OutputExists(path.to_owned())
        } else {
            ExportError::Io { operation, source }
        }
    })
}
