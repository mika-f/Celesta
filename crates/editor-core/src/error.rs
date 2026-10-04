use celesta_project::LoadError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum EditorDocumentError {
    Load(LoadError),
    Duration(celesta_composition::TimeError),
    MissingTrack(String),
}

impl fmt::Display for EditorDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(formatter, "could not load project: {error}"),
            Self::Duration(error) => write!(formatter, "could not calculate duration: {error}"),
            Self::MissingTrack(track_id) => write!(formatter, "track `{track_id}` does not exist"),
        }
    }
}

impl Error for EditorDocumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::MissingTrack(_) => None,
        }
    }
}
