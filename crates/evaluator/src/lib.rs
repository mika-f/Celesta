//! Evaluation of editor-owned project data into renderer-owned composition data.

use celesta_composition::{
    Animatable, AssetLocation, AudioClip, AudioGraph, BlendMode, EvaluatedTransform, Layer,
    LayerContent, LayerEffects, MediaTiming, ResolvedAsset, Scene, TextStyle, Time,
};
use celesta_project::{
    Asset, AssetSource, LipSyncCue, MouthShape, Project, TimelineContent, TimelineItem, Track,
};

mod error;
mod evaluate;
#[cfg(test)]
mod tests;
mod timeline;

pub use error::EvaluationError;
use evaluate::{evaluate_effects, evaluate_optional, evaluate_transform, integrate_optional};
use timeline::{is_active, item_has_audio, mouth_shape_at, source_duration, source_start};

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
            layers.extend(self.active_track_layers(track, time)?);
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
            shaders: Vec::new(),
        })
    }

    /// Evaluates one track's active visual layers at an exact project time,
    /// independent of every other track — the counterpart to `scene_at`
    /// evaluating all of them together. An unknown `track_id`, or a track
    /// disabled at the project level, evaluates to no layers rather than an
    /// error: from the caller's perspective these are the same "nothing to
    /// show" outcome, not a distinct failure.
    pub fn layers_for_track(
        &self,
        track_id: &str,
        time: Time,
    ) -> Result<Vec<Layer>, EvaluationError> {
        if !time.is_valid() || time.value < 0 {
            return Err(EvaluationError::InvalidTime(time));
        }
        let Some(track) = self
            .project
            .tracks
            .iter()
            .find(|track| track.id == track_id)
        else {
            return Ok(Vec::new());
        };
        self.active_track_layers(track, time)
    }

    fn active_track_layers(
        &self,
        track: &Track,
        time: Time,
    ) -> Result<Vec<Layer>, EvaluationError> {
        if track.enabled == Some(false) {
            return Ok(Vec::new());
        }
        let mut layers = Vec::new();
        for item in &track.items {
            if item.enabled == Some(false) || !is_active(item, time)? {
                continue;
            }
            if let Some(layer) = self.visual_layer(item, time)? {
                layers.push(layer);
            }
        }
        Ok(layers)
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
            TimelineContent::Image {
                asset,
                width,
                height,
                fit,
            } => LayerContent::Image {
                asset: self.asset(asset)?,
                width: *width,
                height: *height,
                fit: *fit,
            },
            TimelineContent::Text { text, style } => LayerContent::Text {
                text: text.clone(),
                style: style.clone().unwrap_or_default(),
                max_width: None,
                baseline_anchor: false,
            },
            TimelineContent::Dialogue {
                character,
                text,
                expression,
                lip_sync,
                ..
            } => self.dialogue(character, text, expression.as_deref(), lip_sync, local_time)?,
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
            blend_mode: item.blend_mode.unwrap_or_default(),
            effects: evaluate_effects(item, local_time)?,
            content,
        }))
    }

    fn dialogue(
        &self,
        character_id: &str,
        text: &str,
        expression: Option<&str>,
        lip_sync_cues: &[LipSyncCue],
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
                blend_mode: BlendMode::Normal,
                effects: LayerEffects::default(),
                content: LayerContent::Image {
                    width: None,
                    height: None,
                    fit: None,
                    asset: self.asset(asset)?,
                },
            });

            if let (Some(lip_sync), Some(shape)) = (
                portrait.lip_sync.as_ref(),
                mouth_shape_at(lip_sync_cues, local_time)?,
            ) {
                let asset = match shape {
                    MouthShape::Closed => lip_sync.closed.as_ref(),
                    MouthShape::A => Some(&lip_sync.a),
                    MouthShape::I => Some(&lip_sync.i),
                    MouthShape::U => Some(&lip_sync.u),
                    MouthShape::E => Some(&lip_sync.e),
                    MouthShape::O => Some(&lip_sync.o),
                };
                if let Some(asset) = asset {
                    layers.push(Layer {
                        id: "mouth".to_owned(),
                        transform: evaluate_transform(
                            lip_sync.transform.as_ref().or(portrait.transform.as_ref()),
                            local_time,
                        )?,
                        opacity: 1.0,
                        blend_mode: BlendMode::Normal,
                        effects: LayerEffects::default(),
                        content: LayerContent::Image {
                            width: None,
                            height: None,
                            fit: None,
                            asset: self.asset(asset)?,
                        },
                    });
                }
            }
        }

        if let Some(subtitle) = &character.subtitle {
            layers.push(Layer {
                id: "subtitle".to_owned(),
                transform: evaluate_transform(subtitle.transform.as_ref(), local_time)?,
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: LayerEffects::default(),
                content: LayerContent::Text {
                    text: text.to_owned(),
                    style: subtitle.style.clone().unwrap_or_default(),
                    max_width: subtitle.max_width,
                    baseline_anchor: false,
                },
            });
        } else {
            layers.push(Layer {
                id: "subtitle".to_owned(),
                transform: EvaluatedTransform::default(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: LayerEffects::default(),
                content: LayerContent::Text {
                    text: text.to_owned(),
                    style: TextStyle::default(),
                    max_width: None,
                    baseline_anchor: false,
                },
            });
        }

        Ok(LayerContent::Group {
            layers,
            clip: None,
            mask: None,
        })
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
