use crate::ValidationErrors;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("could not read project: {0}")]
    Io(#[source] std::io::Error),
    #[error("invalid project JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("invalid project: {0}")]
    Validation(#[source] ValidationErrors),
}
