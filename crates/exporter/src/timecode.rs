use celesta_composition::Time;
use std::error::Error;
use std::fmt;

/// Parses a `HH:MM:SS(.mmm)` / `MM:SS(.mmm)` / `SS(.mmm)` timecode into an
/// exact [`Time`] (millisecond timescale). Every component must be a
/// non-negative integer; the fractional part is at most three digits.
pub fn parse_timecode(input: &str) -> Result<Time, TimecodeError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TimecodeError::Empty);
    }
    let malformed = || TimecodeError::Malformed(trimmed.to_owned());

    let components: Vec<&str> = trimmed.split(':').collect();
    if components.len() > 3 {
        return Err(malformed());
    }
    let (seconds_component, leading) = components.split_last().expect("split is never empty");

    let (whole_seconds, millis) = match seconds_component.split_once('.') {
        Some((whole, fraction)) => {
            if fraction.is_empty() || fraction.len() > 3 || !is_ascii_digits(fraction) {
                return Err(malformed());
            }
            let mut padded = fraction.to_owned();
            while padded.len() < 3 {
                padded.push('0');
            }
            (whole, padded.parse::<i64>().map_err(|_| malformed())?)
        }
        None => (*seconds_component, 0),
    };

    let mut total_ms = parse_component(whole_seconds, &malformed)?
        .checked_mul(1_000)
        .and_then(|value| value.checked_add(millis))
        .ok_or_else(|| TimecodeError::Overflow(trimmed.to_owned()))?;

    let mut unit_ms: i64 = 60_000;
    for component in leading.iter().rev() {
        let value = parse_component(component, &malformed)?;
        total_ms = value
            .checked_mul(unit_ms)
            .and_then(|scaled| total_ms.checked_add(scaled))
            .ok_or_else(|| TimecodeError::Overflow(trimmed.to_owned()))?;
        unit_ms = unit_ms
            .checked_mul(60)
            .ok_or_else(|| TimecodeError::Overflow(trimmed.to_owned()))?;
    }

    Ok(Time::new(total_ms, 1_000))
}

pub(crate) fn is_ascii_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn parse_component(
    text: &str,
    malformed: &impl Fn() -> TimecodeError,
) -> Result<i64, TimecodeError> {
    if !is_ascii_digits(text) {
        return Err(malformed());
    }
    text.parse::<i64>().map_err(|_| malformed())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimecodeError {
    Empty,
    Malformed(String),
    Overflow(String),
}

impl fmt::Display for TimecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("timecode is empty"),
            Self::Malformed(input) => write!(
                formatter,
                "'{input}' is not a HH:MM:SS(.mmm), MM:SS(.mmm) or SS(.mmm) timecode"
            ),
            Self::Overflow(input) => write!(formatter, "timecode '{input}' is out of range"),
        }
    }
}

impl Error for TimecodeError {}
