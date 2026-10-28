//! Ducking: a clip turned down while clips of other groups play.
//!
//! `@celesta/media-utils`' `duckEnvelope` ports [`AudioDuck::resolve`] and
//! [`duck_curve`]; `tests/fixtures/duck-curves.json` pins the two.

use serde::{Deserialize, Serialize};

use crate::{
    Animatable, AudioClip, Easing, Keyframe, KeyframeAnimation, KeyframeAnimationType, Time,
    TimeError,
};

/// The groups a clip ducks under, as authored: one name or several.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(untagged)]
pub enum DuckGroups {
    One(String),
    Many(Vec<String>),
}

/// How far a ducked clip goes down, as authored: a gain multiplier, or
/// decibels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(untagged)]
pub enum DuckLevel {
    Gain(f64),
    Db { db: f64 },
}

/// A clip's ducking, resolved: its gain dips to `to` while a clip of a group
/// in `under` plays. See [`duck_curve`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AudioDuck {
    pub under: Vec<String>,
    /// The gain multiplier under the voices, from 0 to 1.
    pub to: f64,
    /// How long the gain takes to come down, ending as a voice starts.
    pub attack: Time,
    /// How long the gain takes to come back up, starting as a voice ends.
    pub release: Time,
    /// Silences shorter than this stay ducked.
    pub hold_gap: Time,
}

/// Which authored ducking value breaks its rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DuckError {
    #[error("must be a non-empty name")]
    Group,
    #[error("must be a non-empty name")]
    Under,
    #[error("must be a multiplier from 0 to 1, or {{ db }} at or below 0")]
    To,
    #[error("must be finite and not negative")]
    Attack,
    #[error("must be finite and not negative")]
    Release,
    #[error("must be finite and not negative")]
    HoldGap,
}

impl DuckError {
    /// The authored field the error is about: `duckGroup`, or a field of
    /// `duck`.
    pub fn field(self) -> &'static str {
        match self {
            Self::Group => "duckGroup",
            Self::Under => "under",
            Self::To => "to",
            Self::Attack => "attack",
            Self::Release => "release",
            Self::HoldGap => "holdGap",
        }
    }
}

impl AudioDuck {
    pub const DEFAULT_TO: f64 = 0.3;
    pub const DEFAULT_ATTACK: Time = Time::new(3, 20);
    pub const DEFAULT_RELEASE: Time = Time::new(2, 5);
    pub const DEFAULT_HOLD_GAP: Time = Time::new(3, 5);

    /// Applies the defaults to authored values, converts decibels to a gain,
    /// and checks every rule.
    pub fn resolve(
        under: &DuckGroups,
        to: Option<DuckLevel>,
        attack: Option<Time>,
        release: Option<Time>,
        hold_gap: Option<Time>,
    ) -> Result<Self, DuckError> {
        Ok(Self {
            under: under.names()?,
            to: to.map_or(Ok(Self::DEFAULT_TO), DuckLevel::gain)?,
            attack: duck_duration(attack, Self::DEFAULT_ATTACK, DuckError::Attack)?,
            release: duck_duration(release, Self::DEFAULT_RELEASE, DuckError::Release)?,
            hold_gap: duck_duration(hold_gap, Self::DEFAULT_HOLD_GAP, DuckError::HoldGap)?,
        })
    }
}

impl DuckGroups {
    /// The group names, each non-empty, at least one.
    pub fn names(&self) -> Result<Vec<String>, DuckError> {
        let names = match self {
            Self::One(group) => vec![group.clone()],
            Self::Many(groups) => groups.clone(),
        };
        if names.is_empty() || names.iter().any(String::is_empty) {
            return Err(DuckError::Under);
        }
        Ok(names)
    }
}

impl DuckLevel {
    /// The gain multiplier, from 0 to 1.
    pub fn gain(self) -> Result<f64, DuckError> {
        match self {
            Self::Gain(gain) if gain.is_finite() && (0.0..=1.0).contains(&gain) => Ok(gain),
            Self::Db { db } if db.is_finite() && db <= 0.0 => Ok(10_f64.powf(db / 20.0)),
            _ => Err(DuckError::To),
        }
    }
}

/// An authored attack, release, or hold gap: `default` when absent, `error`
/// when it is not a valid, non-negative time.
pub fn duck_duration(
    value: Option<Time>,
    default: Time,
    error: DuckError,
) -> Result<Time, DuckError> {
    match value {
        None => Ok(default),
        Some(time) if time.is_valid() && time.value >= 0 => Ok(time),
        Some(_) => Err(error),
    }
}

/// Checks a clip's `duckGroup`.
pub fn check_duck_group(group: &str) -> Result<(), DuckError> {
    if group.is_empty() {
        Err(DuckError::Group)
    } else {
        Ok(())
    }
}

fn microseconds(seconds: f64) -> Time {
    Time::new((seconds * 1_000_000.0).round() as i64, 1_000_000)
}

/// The gain multiplier of a clip ducked by `duck` under voices playing over
/// `intervals` (timeline seconds), as keyframes on the timeline.
///
/// Intervals are joined into runs where the silence between them is shorter
/// than `hold_gap`, or than `attack + release`, so ramps never overlap. Each
/// run eases down from 1 to `to` over `attack` before it starts and back up
/// over `release` after it ends. Without any run the gain is 1.
pub fn duck_curve(
    intervals: &[(f64, f64)],
    duck: &AudioDuck,
) -> Result<Animatable<f64>, TimeError> {
    let attack = duck.attack.as_seconds()?;
    let release = duck.release.as_seconds()?;
    let join = duck.hold_gap.as_seconds()?.max(attack + release);
    let mut sorted: Vec<(f64, f64)> = intervals
        .iter()
        .copied()
        .filter(|(start, end)| start.is_finite() && end.is_finite() && end > start)
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut runs: Vec<(f64, f64)> = Vec::new();
    for (start, end) in sorted {
        match runs.last_mut() {
            Some(run) if start - run.1 < join => run.1 = run.1.max(end),
            _ => runs.push((start, end)),
        }
    }
    if runs.is_empty() {
        return Ok(Animatable::Static(1.0));
    }
    let key = |seconds: f64, value: f64, easing: Option<Easing>| Keyframe {
        time: microseconds(seconds),
        value,
        easing,
    };
    let keyframes = runs
        .into_iter()
        .flat_map(|(start, end)| {
            [
                key(start - attack, 1.0, None),
                key(start, duck.to, Some(Easing::EaseInOut)),
                key(end, duck.to, None),
                key(end + release, 1.0, Some(Easing::EaseInOut)),
            ]
        })
        .collect();
    Ok(Animatable::Keyframes(KeyframeAnimation {
        kind: KeyframeAnimationType::Keyframes,
        keyframes,
    }))
}

/// The ducking curve of each clip (`None` for a clip without `duck`).
///
/// `audible_ends` holds, for each clip, the timeline second it stops
/// sounding; a marked clip counts from its range start to there. Muted clips
/// do not count, a clip never counts toward its own curve, and a marked clip
/// counts whole even when it is ducked itself.
pub fn duck_curves(
    clips: &[AudioClip],
    audible_ends: &[f64],
) -> Result<Vec<Option<Animatable<f64>>>, TimeError> {
    clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let Some(duck) = &clip.duck else {
                return Ok(None);
            };
            let mut intervals = Vec::new();
            for (other, voice) in clips.iter().enumerate() {
                let marked = voice
                    .duck_group
                    .as_ref()
                    .is_some_and(|group| duck.under.contains(group));
                if other != index && marked && !voice.muted {
                    let end = audible_ends[other].min(voice.range.end()?.as_seconds()?);
                    intervals.push((voice.range.start.as_seconds()?, end));
                }
            }
            duck_curve(&intervals, duck).map(Some)
        })
        .collect()
}
