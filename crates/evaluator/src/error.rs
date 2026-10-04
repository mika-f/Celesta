use celesta_composition::{AnimationError, Time, TimeError};
use celesta_project::ValidationErrors;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum EvaluationError {
    InvalidProject(ValidationErrors),
    InvalidTime(Time),
    Time(TimeError),
    Animation(AnimationError),
    InvalidEffectColor(String),
    MissingAsset(String),
    MissingCharacter(String),
    MissingExpression {
        character: String,
        expression: String,
    },
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProject(errors) => write!(formatter, "invalid project: {errors}"),
            Self::InvalidTime(time) => write!(formatter, "invalid evaluation time {time:?}"),
            Self::Time(error) => write!(formatter, "time evaluation failed: {error}"),
            Self::Animation(error) => write!(formatter, "animation evaluation failed: {error}"),
            Self::InvalidEffectColor(color) => write!(formatter, "invalid effect color `{color}`"),
            Self::MissingAsset(id) => write!(formatter, "missing asset `{id}`"),
            Self::MissingCharacter(id) => write!(formatter, "missing character `{id}`"),
            Self::MissingExpression {
                character,
                expression,
            } => write!(
                formatter,
                "character `{character}` has no expression `{expression}`"
            ),
        }
    }
}

impl Error for EvaluationError {}

impl From<TimeError> for EvaluationError {
    fn from(error: TimeError) -> Self {
        Self::Time(error)
    }
}

impl From<AnimationError> for EvaluationError {
    fn from(error: AnimationError) -> Self {
        Self::Animation(error)
    }
}
