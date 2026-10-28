use celesta_composition::{AnimationError, DuckError, Time, TimeError};
use celesta_project::ValidationErrors;

#[derive(Debug, thiserror::Error)]
pub enum EvaluationError {
    #[error("invalid project: {0}")]
    InvalidProject(ValidationErrors),
    #[error("invalid evaluation time {0:?}")]
    InvalidTime(Time),
    #[error("time evaluation failed: {0}")]
    Time(#[from] TimeError),
    #[error("animation evaluation failed: {0}")]
    Animation(#[from] AnimationError),
    #[error("invalid effect color `{0}`")]
    InvalidEffectColor(String),
    #[error("missing asset `{0}`")]
    MissingAsset(String),
    #[error("missing character `{0}`")]
    MissingCharacter(String),
    #[error("character `{character}` has no expression `{expression}`")]
    MissingExpression {
        character: String,
        expression: String,
    },
    #[error("item `{item}` has an invalid duck: {error}")]
    InvalidDuck { item: String, error: DuckError },
}
