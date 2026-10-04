use crate::ValidationErrors;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Validation(ValidationErrors),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read project: {error}"),
            Self::Json(error) => write!(formatter, "invalid project JSON: {error}"),
            Self::Validation(errors) => write!(formatter, "invalid project: {errors}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Validation(errors) => Some(errors),
        }
    }
}
