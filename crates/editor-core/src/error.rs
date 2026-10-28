use celesta_project::LoadError;

#[derive(Debug, thiserror::Error)]
pub enum EditorDocumentError {
    #[error("could not load project: {0}")]
    Load(#[source] LoadError),
    #[error("could not calculate duration: {0}")]
    Duration(#[source] celesta_composition::TimeError),
    #[error("track `{0}` does not exist")]
    MissingTrack(String),
}
