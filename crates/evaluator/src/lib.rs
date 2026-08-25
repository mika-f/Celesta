//! Evaluation of editor-owned project data into renderer-owned composition data.

use std::error::Error;
use std::fmt;

use mikan_composition::{
    Animatable, AnimationError, AssetLocation, AudioClip, AudioGraph, EvaluatedTransform, Layer,
    LayerContent, MediaTiming, Point, ResolvedAsset, Scene, TextStyle, Time, TimeError, Transform,
    evaluate_f64, integrate_f64,
};
use mikan_project::{
    Asset, AssetSource, Project, SourceRange, TimelineContent, TimelineItem, ValidationErrors,
};

pub struct Evaluator<'project> {
    project: &'project Project,
}

impl<'project> Evaluator<'project> {
    pub fn new(project: &'project Project) -> Result<Self, EvaluationError> {
        project
            .validate()
            .map_err(EvaluationError::InvalidProject)?;
        Ok(Self { project })
    }

    /// Evaluates active visual layers at an exact project time.
    pub fn scene_at(&self, time: Time) -> Result<Scene, EvaluationError> {
        if !time.is_valid() || time.value < 0 {
            return Err(EvaluationError::InvalidTime(time));
        }

        let mut layers = Vec::new();
        for track in &self.project.tracks {
            if track.enabled == Some(false) {
                continue;
            }
            for item in &track.items {
                if item.enabled == Some(false) || !is_active(item, time)? {
                    continue;
                }
                if let Some(layer) = self.visual_layer(item, time)? {
                    layers.push(layer);
                }
            }
        }

        Ok(Scene {
            width: self.project.settings.width,
            height: self.project.settings.height,
            frame_rate: self.project.settings.frame_rate,
            time,
            fonts: self
                .project
                .assets
                .iter()
                .filter(|(_, asset)| matches!(asset, Asset::Font { .. }))
                .map(|(id, _)| self.asset(id))
                .collect::<Result<_, _>>()?,
            layers,
        })
    }

    /// Builds the complete audio timeline. Mixing and decoding remain backend concerns.
    pub fn audio_graph(&self) -> Result<AudioGraph, EvaluationError> {
        let mut clips = Vec::new();
        let has_solo = self.project.tracks.iter().any(|track| {
            track.enabled != Some(false)
                && track.solo == Some(true)
                && track.items.iter().any(item_has_audio)
        });
        for track in &self.project.tracks {
            if track.enabled == Some(false)
                || track.muted == Some(true)
                || (has_solo && track.solo != Some(true))
            {
                continue;
            }
            for item in &track.items {
                if item.enabled == Some(false) {
                    continue;
                }
                match &item.content {
                    TimelineContent::Video {
                        asset,
                        source_range,
                        playback_rate,
                        volume,
                        muted,
                    }
                    | TimelineContent::Audio {
                        asset,
                        source_range,
                        playback_rate,
                        volume,
                        muted,
                    } => clips.push(AudioClip {
                        id: item.id.clone(),
                        asset: self.asset(asset)?,
                        range: item.range,
                        source_start: source_start(*source_range),
                        source_duration: source_duration(*source_range),
                        playback_rate: playback_rate.clone().unwrap_or(Animatable::Static(1.0)),
                        volume: volume.clone().unwrap_or(Animatable::Static(1.0)),
                        muted: muted.unwrap_or(false),
                    }),
                    TimelineContent::Dialogue {
                        audio: Some(asset),
                        volume,
                        ..
                    } => clips.push(AudioClip {
                        id: format!("{}:voice", item.id),
                        asset: self.asset(asset)?,
                        range: item.range,
                        source_start: Time::ZERO,
                        source_duration: None,
                        playback_rate: Animatable::Static(1.0),
                        volume: volume.clone().unwrap_or(Animatable::Static(1.0)),
                        muted: false,
                    }),
                    _ => {}
                }
            }
        }

        Ok(AudioGraph {
            sample_rate: self.project.settings.sample_rate,
            master_volume: self.project.settings.master_volume.unwrap_or(1.0),
            clips,
        })
    }

    fn visual_layer(
        &self,
        item: &TimelineItem,
        project_time: Time,
    ) -> Result<Option<Layer>, EvaluationError> {
        let local_time = project_time.checked_sub(item.range.start)?;
        let transform = evaluate_transform(item.transform.as_ref(), local_time)?;
        let opacity = evaluate_optional(&item.opacity, local_time, 1.0)?;
        let content = match &item.content {
            TimelineContent::Video {
                asset,
                source_range,
                playback_rate,
                ..
            } => LayerContent::Video {
                asset: self.asset(asset)?,
                timing: MediaTiming {
                    local_time,
                    source_start: source_start(*source_range),
                    source_time_seconds: source_start(*source_range).as_seconds()?
                        + integrate_optional(playback_rate, local_time, 1.0)?,
                    playback_rate: evaluate_optional(playback_rate, local_time, 1.0)?,
                },
            },
            TimelineContent::Image { asset } => LayerContent::Image {
                asset: self.asset(asset)?,
            },
            TimelineContent::Text { text, style } => LayerContent::Text {
                text: text.clone(),
                style: style.clone().unwrap_or_default(),
                max_width: None,
            },
            TimelineContent::Dialogue {
                character,
                text,
                expression,
                ..
            } => self.dialogue(character, text, expression.as_deref(), local_time)?,
            TimelineContent::Component { component, props } => LayerContent::MissingComponent {
                component: component.clone(),
                props: props.clone().unwrap_or_default(),
            },
            TimelineContent::Audio { .. } => return Ok(None),
        };

        Ok(Some(Layer {
            id: item.id.clone(),
            transform,
            opacity,
            content,
        }))
    }

    fn dialogue(
        &self,
        character_id: &str,
        text: &str,
        expression: Option<&str>,
        local_time: Time,
    ) -> Result<LayerContent, EvaluationError> {
        let character = self
            .project
            .characters
            .get(character_id)
            .ok_or_else(|| EvaluationError::MissingCharacter(character_id.to_owned()))?;
        let mut layers = Vec::new();

        if let Some(portrait) = &character.portrait {
            let expression = expression.unwrap_or(&portrait.default_expression);
            let asset = portrait.expressions.get(expression).ok_or_else(|| {
                EvaluationError::MissingExpression {
                    character: character_id.to_owned(),
                    expression: expression.to_owned(),
                }
            })?;
            layers.push(Layer {
                id: "portrait".to_owned(),
                transform: evaluate_transform(portrait.transform.as_ref(), local_time)?,
                opacity: 1.0,
                content: LayerContent::Image {
                    asset: self.asset(asset)?,
                },
            });
        }

        if let Some(subtitle) = &character.subtitle {
            layers.push(Layer {
                id: "subtitle".to_owned(),
                transform: evaluate_transform(subtitle.transform.as_ref(), local_time)?,
                opacity: 1.0,
                content: LayerContent::Text {
                    text: text.to_owned(),
                    style: subtitle.style.clone().unwrap_or_default(),
                    max_width: subtitle.max_width,
                },
            });
        } else {
            layers.push(Layer {
                id: "subtitle".to_owned(),
                transform: EvaluatedTransform::default(),
                opacity: 1.0,
                content: LayerContent::Text {
                    text: text.to_owned(),
                    style: TextStyle::default(),
                    max_width: None,
                },
            });
        }

        Ok(LayerContent::Group { layers })
    }

    fn asset(&self, id: &str) -> Result<ResolvedAsset, EvaluationError> {
        let asset = self
            .project
            .assets
            .get(id)
            .ok_or_else(|| EvaluationError::MissingAsset(id.to_owned()))?;
        let source = match asset {
            Asset::Video { source, .. }
            | Asset::Audio { source, .. }
            | Asset::Image { source, .. }
            | Asset::Font { source, .. } => source,
        };
        let location = match source {
            AssetSource::File { path } => AssetLocation::File { path: path.clone() },
            AssetSource::Url { url } => AssetLocation::Url { url: url.clone() },
        };
        Ok(ResolvedAsset {
            id: id.to_owned(),
            location,
        })
    }
}

fn item_has_audio(item: &TimelineItem) -> bool {
    matches!(
        &item.content,
        TimelineContent::Video { .. }
            | TimelineContent::Audio { .. }
            | TimelineContent::Dialogue { audio: Some(_), .. }
    )
}

fn is_active(item: &TimelineItem, time: Time) -> Result<bool, TimeError> {
    Ok(time.cmp_exact(item.range.start)?.is_ge() && time.cmp_exact(item.range.end()?)?.is_lt())
}

fn source_start(range: Option<SourceRange>) -> Time {
    range.map_or(Time::ZERO, |range| range.start)
}

fn source_duration(range: Option<SourceRange>) -> Option<Time> {
    range.and_then(|range| range.duration)
}

fn evaluate_optional(
    value: &Option<Animatable<f64>>,
    time: Time,
    default: f64,
) -> Result<f64, AnimationError> {
    value
        .as_ref()
        .map_or(Ok(default), |value| evaluate_f64(value, time))
}

fn integrate_optional(
    value: &Option<Animatable<f64>>,
    time: Time,
    default: f64,
) -> Result<f64, AnimationError> {
    value.as_ref().map_or_else(
        || Ok(default * time.as_seconds()?),
        |value| integrate_f64(value, time),
    )
}

fn evaluate_transform(
    transform: Option<&Transform>,
    time: Time,
) -> Result<EvaluatedTransform, AnimationError> {
    let Some(transform) = transform else {
        return Ok(EvaluatedTransform::default());
    };
    let defaults = EvaluatedTransform::default();

    Ok(EvaluatedTransform {
        position: evaluate_point(transform.position.as_ref(), time, defaults.position)?,
        scale: evaluate_point(transform.scale.as_ref(), time, defaults.scale)?,
        rotation: evaluate_optional(&transform.rotation, time, defaults.rotation)?,
        anchor: evaluate_point(transform.anchor.as_ref(), time, defaults.anchor)?,
    })
}

fn evaluate_point(
    point: Option<&mikan_composition::AnimatablePoint>,
    time: Time,
    default: Point,
) -> Result<Point, AnimationError> {
    let Some(point) = point else {
        return Ok(default);
    };
    Ok(Point {
        x: evaluate_optional(&point.x, time, default.x)?,
        y: evaluate_optional(&point.y, time, default.y)?,
    })
}

#[derive(Debug)]
pub enum EvaluationError {
    InvalidProject(ValidationErrors),
    InvalidTime(Time),
    Time(TimeError),
    Animation(AnimationError),
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

#[cfg(test)]
mod tests {
    use mikan_composition::{LayerContent, Time};
    use mikan_project::{Project, SourceRange, TimelineContent};

    use super::*;

    fn example() -> Project {
        Project::from_json(include_str!("../../../examples/voiceroid.mikan.json")).unwrap()
    }

    #[test]
    fn expands_dialogue_into_visual_and_audio_composition() {
        let project = example();
        let evaluator = Evaluator::new(&project).unwrap();

        assert!(
            evaluator
                .scene_at(Time::new(499, 100))
                .unwrap()
                .layers
                .is_empty()
        );

        let scene = evaluator.scene_at(Time::new(21, 4)).unwrap();
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].opacity, 1.0);
        let LayerContent::Group { layers } = &scene.layers[0].content else {
            panic!("dialogue must expand to a group");
        };
        assert_eq!(layers.len(), 2);
        assert!(matches!(layers[0].content, LayerContent::Image { .. }));
        assert!(matches!(layers[1].content, LayerContent::Text { .. }));

        let audio = evaluator.audio_graph().unwrap();
        assert_eq!(audio.clips.len(), 1);
        assert_eq!(audio.clips[0].id, "dialogue-001:voice");
        assert_eq!(audio.master_volume, 1.0);
    }

    #[test]
    fn applies_track_mute_and_project_master_volume_to_audio_graph() {
        let mut project = example();
        project.settings.master_volume = Some(0.5);
        project.tracks[0].muted = Some(true);

        let audio = Evaluator::new(&project).unwrap().audio_graph().unwrap();

        assert!(audio.clips.is_empty());
        assert_eq!(audio.master_volume, 0.5);
    }

    #[test]
    fn audio_graph_preserves_source_range_and_playback_mapping() {
        let mut project = example();
        project.tracks[0].items[0].content = TimelineContent::Audio {
            asset: "voice-001".to_owned(),
            source_range: Some(SourceRange {
                start: Time::new(1, 2),
                duration: Some(Time::new(3, 2)),
            }),
            playback_rate: Some(Animatable::Static(2.0)),
            volume: None,
            muted: None,
        };

        let audio = Evaluator::new(&project).unwrap().audio_graph().unwrap();

        assert_eq!(audio.clips[0].source_start, Time::new(1, 2));
        assert_eq!(audio.clips[0].source_duration, Some(Time::new(3, 2)));
        assert_eq!(audio.clips[0].playback_rate, Animatable::Static(2.0));
    }

    #[test]
    fn solo_tracks_exclude_audio_from_other_tracks() {
        let mut project = example();
        let mut solo = project.tracks[0].clone();
        solo.id = "solo-dialogue".to_owned();
        solo.items[0].id = "solo-dialogue-item".to_owned();
        solo.solo = Some(true);
        project.tracks.push(solo);

        let audio = Evaluator::new(&project).unwrap().audio_graph().unwrap();

        assert_eq!(audio.clips.len(), 1);
        assert_eq!(audio.clips[0].id, "solo-dialogue-item:voice");
    }

    #[test]
    fn item_end_is_exclusive() {
        let project = example();
        let evaluator = Evaluator::new(&project).unwrap();
        assert!(
            evaluator
                .scene_at(Time::new(8, 1))
                .unwrap()
                .layers
                .is_empty()
        );
    }

    #[test]
    fn returns_missing_components_as_placeholder_layers() {
        let mut project = example();
        project.tracks[0].items[0].content = TimelineContent::Component {
            component: "BossIntroduction".to_owned(),
            props: None,
        };
        let evaluator = Evaluator::new(&project).unwrap();
        let scene = evaluator.scene_at(Time::new(6, 1)).unwrap();
        assert!(matches!(
            scene.layers[0].content,
            LayerContent::MissingComponent { .. }
        ));
    }
}
