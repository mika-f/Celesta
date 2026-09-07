//! Editor-owned project state and renderer-facing preview evaluation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mikan_composition::{
    Animatable, AnimatablePoint, AudioGraph, Keyframe, KeyframeAnimation, KeyframeAnimationType,
    Paint, Rational, Scene, Stroke, TextAlign, TextStyle, Time, TimeError, TimeRange, Transform,
    evaluate_f64,
};
use mikan_evaluator::{EvaluationError, Evaluator};
use mikan_project::{
    Asset, AssetKind, AssetSource, Character, LipSyncCue, LipSyncDefinition, LoadError, MouthShape,
    PortraitDefinition, Project, ProjectSettings, ProjectVersion, SubtitleDefinition,
    TimelineContent, TimelineItem, Track, TrackKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSummary {
    pub id: String,
    pub kind: AssetKind,
    pub path: Option<PathBuf>,
    pub missing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSummary {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub item_count: usize,
    pub clips: Vec<ClipSummary>,
    pub enabled: bool,
    pub locked: bool,
    pub muted: bool,
    pub solo: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipSummary {
    pub id: String,
    pub name: String,
    pub start: Time,
    pub duration: Time,
    pub kind: ClipKind,
    pub enabled: bool,
    pub volume: Option<Animatable<f64>>,
    /// The registered component name and its currently configured props,
    /// present only for `ClipKind::Component` clips.
    pub component: Option<ComponentClipSummary>,
    /// Dialogue-specific authoring fields, present only for dialogue clips.
    pub dialogue: Option<DialogueClipSummary>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentClipSummary {
    pub name: String,
    pub props: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueClipSummary {
    pub character: String,
    pub text: String,
    pub audio: Option<String>,
    pub expression: Option<String>,
    pub lip_sync_cue_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterSummary {
    pub id: String,
    pub name: String,
    pub default_expression: Option<String>,
    pub expressions: Vec<String>,
    pub lip_sync: Option<LipSyncSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LipSyncSummary {
    pub a: String,
    pub i: String,
    pub u: String,
    pub e: String,
    pub o: String,
    pub closed: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    Video,
    Audio,
    Image,
    Text,
    Dialogue,
    Component,
}

/// Frame-accurate playhead state shared by editor controls and preview evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineClock {
    frame: i64,
    end_frame: i64,
    frame_rate: Rational,
}

impl TimelineClock {
    pub fn new(duration: Time, frame_rate: Rational) -> Result<Self, TimeError> {
        if !duration.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }
        if !frame_rate.is_valid() {
            return Err(TimeError::InvalidFrameRate);
        }
        let numerator = i128::from(duration.value.max(0)) * i128::from(frame_rate.numerator);
        let denominator = i128::from(duration.timescale) * i128::from(frame_rate.denominator);
        let end_frame = numerator
            .checked_add(denominator - 1)
            .and_then(|value| i64::try_from(value / denominator).ok())
            .ok_or(TimeError::Overflow)?;
        Ok(Self {
            frame: 0,
            end_frame,
            frame_rate,
        })
    }

    pub const fn frame(self) -> i64 {
        self.frame
    }

    pub const fn end_frame(self) -> i64 {
        self.end_frame
    }

    pub const fn is_at_end(self) -> bool {
        self.frame >= self.end_frame
    }

    pub fn time(self) -> Result<Time, TimeError> {
        Time::frames(self.frame, self.frame_rate)
    }

    pub fn frame_for_time(self, time: Time) -> Result<i64, TimeError> {
        if !time.is_valid() {
            return Err(TimeError::ZeroTimescale);
        }
        let numerator = i128::from(time.value) * i128::from(self.frame_rate.numerator);
        let denominator = i128::from(time.timescale) * i128::from(self.frame_rate.denominator);
        let rounded = if numerator >= 0 {
            numerator.checked_add(denominator / 2)
        } else {
            numerator.checked_sub(denominator / 2)
        }
        .ok_or(TimeError::Overflow)?;
        i64::try_from(rounded / denominator).map_err(|_| TimeError::Overflow)
    }

    pub fn seek(&mut self, frame: i64) {
        self.frame = frame.clamp(0, self.end_frame);
    }

    pub fn frame_at_fraction(self, fraction: f32) -> i64 {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        (fraction * self.end_frame as f32).round() as i64
    }

    pub fn seek_fraction(&mut self, fraction: f32) {
        self.frame = self.frame_at_fraction(fraction);
    }

    pub fn step(&mut self, delta: i64) {
        self.seek(self.frame.saturating_add(delta));
    }
}

/// Converts a clip-local peak envelope into a compact, frame-aligned mouth
/// animation. A relative noise gate adapts to quiet recordings, while
/// hysteresis prevents rapid open/closed chatter around the threshold.
pub fn lip_sync_cues_from_waveform(
    waveform: &[f32],
    text: &str,
    duration: Time,
    frame_rate: Rational,
) -> Result<Vec<LipSyncCue>, TimeError> {
    let clock = TimelineClock::new(duration, frame_rate)?;
    let frame_count = clock.end_frame().max(1);
    let peak = waveform
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0_f32, f32::max);
    let open_threshold = (peak * 0.18).max(0.015);
    let close_threshold = open_threshold * 0.6;
    let mut voiced = false;
    let mut voiced_frames = Vec::with_capacity(frame_count as usize);

    for frame in 0..frame_count {
        let amplitude = if waveform.is_empty() {
            0.0
        } else {
            let index = ((frame as usize).saturating_mul(waveform.len()) / frame_count as usize)
                .min(waveform.len() - 1);
            waveform[index]
        };
        voiced = match voiced {
            false if amplitude >= open_threshold => true,
            true if amplitude <= close_threshold => false,
            current => current,
        };
        voiced_frames.push(voiced);
    }

    let vowels = vowel_shapes(text);
    let voiced_count = voiced_frames.iter().filter(|voiced| **voiced).count();
    let mut voiced_index = 0;
    let mut current = None;
    let mut cues = Vec::new();
    for (frame, voiced) in voiced_frames.into_iter().enumerate() {
        let next = if voiced {
            let shape = if vowels.is_empty() {
                MouthShape::A
            } else {
                vowels[(voiced_index * vowels.len() / voiced_count).min(vowels.len() - 1)]
            };
            voiced_index += 1;
            shape
        } else {
            MouthShape::Closed
        };
        if frame == 0 || Some(next) != current {
            cues.push(LipSyncCue {
                time: Time::frames(frame as i64, frame_rate)?,
                shape: next,
            });
        }
        current = Some(next);
    }

    Ok(cues)
}

fn vowel_shapes(text: &str) -> Vec<MouthShape> {
    let mut shapes = Vec::new();
    for character in text.chars() {
        let shape = if "aAあぁゃかがさざただなはばぱまやらわアァャカガサザタダナハバパマヤラワ"
            .contains(character)
        {
            Some(MouthShape::A)
        } else if "iIいぃきぎしじちぢにひびぴみりイィキギシジチヂニヒビピミリ".contains(character)
        {
            Some(MouthShape::I)
        } else if "uUうぅゅくぐすずつづぬふぶぷむゆるゔウゥュクグスズツヅヌフブプムユルヴ"
            .contains(character)
        {
            Some(MouthShape::U)
        } else if "eEえぇけげせぜてでねへべぺめれゑエェケゲセゼテデネヘベペメレヱ"
            .contains(character)
        {
            Some(MouthShape::E)
        } else if "oOおぉょこごそぞとどのほぼぽもよろをオォョコゴソゾトドノホボポモヨロヲ"
            .contains(character)
        {
            Some(MouthShape::O)
        } else {
            None
        };
        let Some(shape) = shape else {
            if character == 'ー'
                && let Some(previous) = shapes.last().copied()
            {
                shapes.push(previous);
            }
            continue;
        };
        if "ぁぃぅぇぉゃゅょァィゥェォャュョ".contains(character)
            && let Some(previous) = shapes.last_mut()
        {
            *previous = shape;
        } else {
            shapes.push(shape);
        }
    }
    shapes
}

/// A loaded project plus the editor-only context that must not be serialized.
pub struct EditorDocument {
    project: Project,
    path: Option<PathBuf>,
    asset_root: PathBuf,
    duration: Time,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    history_group: Option<HistoryState>,
    current_revision: u64,
    clean_revision: u64,
    next_revision: u64,
}

#[derive(Clone)]
struct HistoryState {
    project: Project,
    revision: u64,
}

struct HistoryEntry {
    before: HistoryState,
    after: HistoryState,
}

impl EditorDocument {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, EditorDocumentError> {
        let path = path.as_ref();
        let project = Project::load(path).map_err(EditorDocumentError::Load)?;
        // Canonicalize so paths serialized relative to the root
        // (`serialized_asset_path`) resolve back to exactly the same
        // absolute paths — on macOS, `/var` is a symlink to `/private/var`,
        // and mixing canonicalized input paths with a non-canonicalized
        // root would break that round trip.
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let asset_root = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Self::new(project, Some(path), asset_root)
    }

    pub fn from_json(
        input: &str,
        asset_root: impl Into<PathBuf>,
    ) -> Result<Self, EditorDocumentError> {
        let project = Project::from_json(input).map_err(EditorDocumentError::Load)?;
        Self::new(project, None, asset_root.into())
    }

    /// Builds a preview-only document backing a standalone React composition
    /// entry (`.tsx`). No `project.json` exists: the synthetic [`Project`]
    /// only carries the composition's dimensions, frame rate, sample rate,
    /// and total duration (read from the entry's `<Composition>` via the
    /// React bridge handshake) plus `react_entry` pointing back at the file.
    /// It has no tracks, so nothing evaluates it into a scene — the editor
    /// renders each frame straight from the bridge instead — and it is never
    /// saved or mutated.
    pub fn react_preview(
        entry: &Path,
        width: u32,
        height: u32,
        frame_rate: Rational,
        sample_rate: u32,
        duration_in_frames: u64,
    ) -> Result<Self, EditorDocumentError> {
        let entry = fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());
        let asset_root = entry
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let duration = Time::frames(
            i64::try_from(duration_in_frames).unwrap_or(i64::MAX),
            frame_rate,
        )
        .map_err(EditorDocumentError::Duration)?;
        let project = Project {
            version: ProjectVersion::V0,
            settings: ProjectSettings {
                width,
                height,
                frame_rate,
                sample_rate,
                master_volume: None,
                duration: Some(duration),
                react_entry: Some(
                    entry
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                ),
            },
            assets: BTreeMap::new(),
            characters: BTreeMap::new(),
            tracks: Vec::new(),
            properties: BTreeMap::new(),
        };
        Self::new(project, Some(entry), asset_root)
    }

    fn new(
        project: Project,
        path: Option<PathBuf>,
        asset_root: PathBuf,
    ) -> Result<Self, EditorDocumentError> {
        let duration = project
            .effective_duration()
            .map_err(EditorDocumentError::Duration)?;
        Ok(Self {
            project,
            path,
            asset_root,
            duration,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            history_group: None,
            current_revision: 0,
            clean_revision: 0,
            next_revision: 1,
        })
    }

    pub const fn project(&self) -> &Project {
        &self.project
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn asset_root(&self) -> &Path {
        self.asset_root.as_path()
    }

    pub const fn duration(&self) -> Time {
        self.duration
    }

    pub fn master_volume(&self) -> f64 {
        self.project.settings.master_volume.unwrap_or(1.0)
    }

    pub fn is_dirty(&self) -> bool {
        self.current_revision != self.clean_revision || self.history_group_changed()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() || self.history_group_changed()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty() && self.history_group.is_none()
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_owned()
    }

    pub fn scene_at(&self, time: Time) -> Result<Scene, EvaluationError> {
        Evaluator::new(&self.project)?.scene_at(time)
    }

    pub fn audio_graph(&self) -> Result<AudioGraph, EvaluationError> {
        Evaluator::new(&self.project)?.audio_graph()
    }

    pub fn assets(&self) -> Vec<AssetSummary> {
        self.project
            .assets
            .iter()
            .map(|(id, asset)| {
                let path = local_asset_path(asset, &self.asset_root);
                let missing = path.as_ref().is_some_and(|path| !path.is_file());
                AssetSummary {
                    id: id.clone(),
                    kind: asset.kind(),
                    path,
                    missing,
                }
            })
            .collect()
    }

    pub fn characters(&self) -> Vec<CharacterSummary> {
        self.project
            .characters
            .iter()
            .map(|(id, character)| CharacterSummary {
                id: id.clone(),
                name: character.name.clone(),
                default_expression: character
                    .portrait
                    .as_ref()
                    .map(|portrait| portrait.default_expression.clone()),
                expressions: character
                    .portrait
                    .as_ref()
                    .map(|portrait| portrait.expressions.keys().cloned().collect())
                    .unwrap_or_default(),
                lip_sync: character.portrait.as_ref().and_then(|portrait| {
                    portrait.lip_sync.as_ref().map(|lip_sync| LipSyncSummary {
                        a: lip_sync.a.clone(),
                        i: lip_sync.i.clone(),
                        u: lip_sync.u.clone(),
                        e: lip_sync.e.clone(),
                        o: lip_sync.o.clone(),
                        closed: lip_sync.closed.clone(),
                    })
                }),
            })
            .collect()
    }

    pub fn create_character_from_image(
        &mut self,
        asset_id: &str,
    ) -> Result<CharacterSummary, EditorDocumentError> {
        let asset = self
            .project
            .assets
            .get(asset_id)
            .ok_or_else(|| EditorDocumentError::MissingAsset(asset_id.to_owned()))?;
        if asset.kind() != AssetKind::Image {
            return Err(EditorDocumentError::AssetKindMismatch {
                asset: asset_id.to_owned(),
                expected: AssetKind::Image,
                actual: asset.kind(),
            });
        }
        let name = asset_name(asset)
            .and_then(|name| Path::new(name).file_stem())
            .and_then(|name| name.to_str())
            .unwrap_or(asset_id)
            .to_owned();
        let id = unique_id(
            &slugify(&name),
            self.project.characters.keys().map(String::as_str),
        );
        let width = f64::from(self.project.settings.width);
        let height = f64::from(self.project.settings.height);
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.characters.insert(
            id.clone(),
            Character {
                name: name.clone(),
                portrait: Some(PortraitDefinition {
                    default_expression: "default".to_owned(),
                    expressions: BTreeMap::from([("default".to_owned(), asset_id.to_owned())]),
                    transform: Some(Transform {
                        position: Some(AnimatablePoint {
                            x: Some(Animatable::Static(width * 0.82)),
                            y: Some(Animatable::Static(height * 0.72)),
                        }),
                        ..Transform::default()
                    }),
                    lip_sync: None,
                }),
                subtitle: Some(SubtitleDefinition {
                    style: Some(TextStyle {
                        font_size: Some((height * 0.06).max(24.0)),
                        fill: Some(Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        }),
                        stroke: Some(Stroke {
                            paint: Paint::Solid {
                                color: "#000000FF".to_owned(),
                            },
                            width: (height * 0.004).max(2.0),
                        }),
                        align: Some(TextAlign::Center),
                        line_height: Some(1.15),
                        ..TextStyle::default()
                    }),
                    transform: Some(Transform {
                        position: Some(AnimatablePoint {
                            x: Some(Animatable::Static(width / 2.0)),
                            y: Some(Animatable::Static(height * 0.9)),
                        }),
                        ..Transform::default()
                    }),
                    max_width: Some(width * 0.85),
                }),
            },
        );
        self.record_mutation(before, before_revision);
        Ok(CharacterSummary {
            id,
            name,
            default_expression: Some("default".to_owned()),
            expressions: vec!["default".to_owned()],
            lip_sync: None,
        })
    }

    pub fn add_character_expression(
        &mut self,
        character_id: &str,
        asset_id: &str,
    ) -> Result<String, EditorDocumentError> {
        let asset = self
            .project
            .assets
            .get(asset_id)
            .ok_or_else(|| EditorDocumentError::MissingAsset(asset_id.to_owned()))?;
        if asset.kind() != AssetKind::Image {
            return Err(EditorDocumentError::AssetKindMismatch {
                asset: asset_id.to_owned(),
                expected: AssetKind::Image,
                actual: asset.kind(),
            });
        }
        let expression_base = asset_name(asset)
            .and_then(|name| Path::new(name).file_stem())
            .and_then(|name| name.to_str())
            .unwrap_or(asset_id);
        let character = self
            .project
            .characters
            .get(character_id)
            .ok_or_else(|| EditorDocumentError::MissingCharacter(character_id.to_owned()))?;
        if let Some((expression, _)) = character.portrait.as_ref().and_then(|portrait| {
            portrait
                .expressions
                .iter()
                .find(|(_, existing_asset)| existing_asset.as_str() == asset_id)
        }) {
            return Ok(expression.clone());
        }
        let expression = unique_id(
            &slugify(expression_base),
            character
                .portrait
                .iter()
                .flat_map(|portrait| portrait.expressions.keys().map(String::as_str)),
        );

        let before = self.project.clone();
        let before_revision = self.current_revision;
        let character = self
            .project
            .characters
            .get_mut(character_id)
            .expect("character existence was checked");
        if let Some(portrait) = &mut character.portrait {
            portrait
                .expressions
                .insert(expression.clone(), asset_id.to_owned());
        } else {
            let width = f64::from(self.project.settings.width);
            let height = f64::from(self.project.settings.height);
            character.portrait = Some(PortraitDefinition {
                default_expression: expression.clone(),
                expressions: BTreeMap::from([(expression.clone(), asset_id.to_owned())]),
                transform: Some(Transform {
                    position: Some(AnimatablePoint {
                        x: Some(Animatable::Static(width * 0.82)),
                        y: Some(Animatable::Static(height * 0.72)),
                    }),
                    ..Transform::default()
                }),
                lip_sync: None,
            });
        }
        self.record_mutation(before, before_revision);
        Ok(expression)
    }

    /// Assigns one transparent mouth overlay to a character. The first
    /// vowel assignment initializes every vowel to the same image, keeping the
    /// project valid while the remaining images are selected. Closed is optional.
    pub fn set_character_lip_sync_asset(
        &mut self,
        character_id: &str,
        shape: MouthShape,
        asset_id: &str,
    ) -> Result<(), EditorDocumentError> {
        let asset = self
            .project
            .assets
            .get(asset_id)
            .ok_or_else(|| EditorDocumentError::MissingAsset(asset_id.to_owned()))?;
        if asset.kind() != AssetKind::Image {
            return Err(EditorDocumentError::AssetKindMismatch {
                asset: asset_id.to_owned(),
                expected: AssetKind::Image,
                actual: asset.kind(),
            });
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let character = self
            .project
            .characters
            .get_mut(character_id)
            .ok_or_else(|| EditorDocumentError::MissingCharacter(character_id.to_owned()))?;
        let portrait = character.portrait.as_mut().ok_or_else(|| {
            EditorDocumentError::MissingCharacterPortrait(character_id.to_owned())
        })?;
        let lip_sync = portrait.lip_sync.get_or_insert_with(|| LipSyncDefinition {
            a: asset_id.to_owned(),
            i: asset_id.to_owned(),
            u: asset_id.to_owned(),
            e: asset_id.to_owned(),
            o: asset_id.to_owned(),
            closed: None,
            transform: None,
        });
        match shape {
            MouthShape::Closed => lip_sync.closed = Some(asset_id.to_owned()),
            MouthShape::A => lip_sync.a = asset_id.to_owned(),
            MouthShape::I => lip_sync.i = asset_id.to_owned(),
            MouthShape::U => lip_sync.u = asset_id.to_owned(),
            MouthShape::E => lip_sync.e = asset_id.to_owned(),
            MouthShape::O => lip_sync.o = asset_id.to_owned(),
        }
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn clear_character_lip_sync(
        &mut self,
        character_id: &str,
    ) -> Result<(), EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let character = self
            .project
            .characters
            .get_mut(character_id)
            .ok_or_else(|| EditorDocumentError::MissingCharacter(character_id.to_owned()))?;
        if let Some(portrait) = &mut character.portrait {
            portrait.lip_sync = None;
        }
        for item in self
            .project
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.items)
        {
            if let TimelineContent::Dialogue {
                character,
                lip_sync,
                ..
            } = &mut item.content
                && character == character_id
            {
                lip_sync.clear();
            }
        }
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn clear_character_closed_mouth(
        &mut self,
        character_id: &str,
    ) -> Result<(), EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let character = self
            .project
            .characters
            .get_mut(character_id)
            .ok_or_else(|| EditorDocumentError::MissingCharacter(character_id.to_owned()))?;
        if let Some(lip_sync) = character
            .portrait
            .as_mut()
            .and_then(|portrait| portrait.lip_sync.as_mut())
        {
            lip_sync.closed = None;
        }
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn rename_character(
        &mut self,
        character_id: &str,
        name: &str,
    ) -> Result<(), EditorDocumentError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(EditorDocumentError::InvalidCharacterName);
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let character = self
            .project
            .characters
            .get_mut(character_id)
            .ok_or_else(|| EditorDocumentError::MissingCharacter(character_id.to_owned()))?;
        character.name = name.to_owned();
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn character_references(
        &self,
        character_id: &str,
    ) -> Result<Vec<String>, EditorDocumentError> {
        if !self.project.characters.contains_key(character_id) {
            return Err(EditorDocumentError::MissingCharacter(
                character_id.to_owned(),
            ));
        }
        Ok(self
            .project
            .tracks
            .iter()
            .flat_map(|track| {
                track.items.iter().filter_map(|item| match &item.content {
                    TimelineContent::Dialogue { character, .. } if character == character_id => {
                        Some(format!("track `{}` / clip `{}`", track.id, item.id))
                    }
                    _ => None,
                })
            })
            .collect())
    }

    pub fn delete_character(
        &mut self,
        character_id: &str,
        remove_dialogue_items: bool,
    ) -> Result<(), EditorDocumentError> {
        let references = self.character_references(character_id)?;
        if !references.is_empty() && !remove_dialogue_items {
            return Err(EditorDocumentError::CharacterInUse {
                character: character_id.to_owned(),
                references,
            });
        }
        if remove_dialogue_items
            && let Some(track) = self.project.tracks.iter().find(|track| {
                track.locked == Some(true)
                    && track.items.iter().any(|item| {
                        matches!(
                            &item.content,
                            TimelineContent::Dialogue { character, .. }
                                if character == character_id
                        )
                    })
            })
        {
            return Err(EditorDocumentError::LockedTrack(track.id.clone()));
        }

        let before = self.project.clone();
        let before_revision = self.current_revision;
        if remove_dialogue_items {
            for track in &mut self.project.tracks {
                track.items.retain(|item| {
                    !matches!(
                        &item.content,
                        TimelineContent::Dialogue { character, .. }
                            if character == character_id
                    )
                });
            }
        }
        self.project.characters.remove(character_id);
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(())
    }

    pub fn tracks(&self) -> Vec<TrackSummary> {
        self.project
            .tracks
            .iter()
            .map(|track| TrackSummary {
                id: track.id.clone(),
                name: track.name.clone(),
                kind: track.kind,
                item_count: track.items.len(),
                clips: track
                    .items
                    .iter()
                    .map(|item| ClipSummary {
                        id: item.id.clone(),
                        name: item.name.clone().unwrap_or_else(|| item.id.clone()),
                        start: item.range.start,
                        duration: item.range.duration,
                        kind: match item.content {
                            TimelineContent::Video { .. } => ClipKind::Video,
                            TimelineContent::Audio { .. } => ClipKind::Audio,
                            TimelineContent::Image { .. } => ClipKind::Image,
                            TimelineContent::Text { .. } => ClipKind::Text,
                            TimelineContent::Dialogue { .. } => ClipKind::Dialogue,
                            TimelineContent::Component { .. } => ClipKind::Component,
                        },
                        enabled: item.enabled != Some(false),
                        volume: match &item.content {
                            TimelineContent::Video { volume, .. }
                            | TimelineContent::Audio { volume, .. } => {
                                Some(volume.clone().unwrap_or(Animatable::Static(1.0)))
                            }
                            TimelineContent::Dialogue {
                                audio: Some(_),
                                volume,
                                ..
                            } => Some(volume.clone().unwrap_or(Animatable::Static(1.0))),
                            _ => None,
                        },
                        component: match &item.content {
                            TimelineContent::Component { component, props } => {
                                Some(ComponentClipSummary {
                                    name: component.clone(),
                                    props: props.clone().unwrap_or_default(),
                                })
                            }
                            _ => None,
                        },
                        dialogue: match &item.content {
                            TimelineContent::Dialogue {
                                character,
                                text,
                                audio,
                                expression,
                                lip_sync,
                                ..
                            } => Some(DialogueClipSummary {
                                character: character.clone(),
                                text: text.clone(),
                                audio: audio.clone(),
                                expression: expression.clone(),
                                lip_sync_cue_count: lip_sync.len(),
                            }),
                            _ => None,
                        },
                    })
                    .collect(),
                enabled: track.enabled != Some(false),
                locked: track.locked == Some(true),
                muted: track.muted == Some(true),
                solo: track.solo == Some(true),
            })
            .collect()
    }

    pub fn toggle_track_muted(&mut self, track_id: &str) -> Result<bool, EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        let muted = track.muted != Some(true);
        track.muted = muted.then_some(true);
        self.record_mutation(before, before_revision);
        Ok(muted)
    }

    pub fn add_track(&mut self, kind: TrackKind) -> String {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (base, name) = track_defaults(kind);
        let track_id = unique_id(
            base,
            self.project.tracks.iter().map(|track| track.id.as_str()),
        );
        self.project.tracks.push(Track {
            id: track_id.clone(),
            name: name.to_owned(),
            kind,
            enabled: None,
            locked: None,
            muted: None,
            solo: None,
            items: Vec::new(),
        });
        self.record_mutation(before, before_revision);
        track_id
    }

    pub fn move_track(
        &mut self,
        track_id: &str,
        offset: isize,
    ) -> Result<bool, EditorDocumentError> {
        let index = self
            .project
            .tracks
            .iter()
            .position(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        if self.project.tracks[index].locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
        }
        let target = index
            .saturating_add_signed(offset)
            .min(self.project.tracks.len() - 1);
        if target == index {
            return Ok(false);
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track = self.project.tracks.remove(index);
        self.project.tracks.insert(target, track);
        self.record_mutation(before, before_revision);
        Ok(true)
    }

    pub fn rename_track(&mut self, track_id: &str, name: &str) -> Result<(), EditorDocumentError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(EditorDocumentError::InvalidTrackName);
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        if track.locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
        }
        track.name = name.to_owned();
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn toggle_track_enabled(&mut self, track_id: &str) -> Result<bool, EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        if track.locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
        }
        let enabled = track.enabled == Some(false);
        track.enabled = (!enabled).then_some(false);
        self.record_mutation(before, before_revision);
        Ok(enabled)
    }

    pub fn toggle_track_locked(&mut self, track_id: &str) -> Result<bool, EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        let locked = track.locked != Some(true);
        track.locked = locked.then_some(true);
        self.record_mutation(before, before_revision);
        Ok(locked)
    }

    pub fn delete_track(
        &mut self,
        track_id: &str,
        delete_non_empty: bool,
    ) -> Result<(), EditorDocumentError> {
        let index = self
            .project
            .tracks
            .iter()
            .position(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        let track = &self.project.tracks[index];
        if track.locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
        }
        if !delete_non_empty && !track.items.is_empty() {
            return Err(EditorDocumentError::NonEmptyTrack {
                track: track_id.to_owned(),
                item_count: track.items.len(),
            });
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.tracks.remove(index);
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(())
    }

    pub fn move_clip_to_track(
        &mut self,
        clip_id: &str,
        target_track_id: &str,
    ) -> Result<bool, EditorDocumentError> {
        let source_index = self
            .project
            .tracks
            .iter()
            .position(|track| track.items.iter().any(|item| item.id == clip_id))
            .ok_or_else(|| EditorDocumentError::MissingClip(clip_id.to_owned()))?;
        let target_index = self
            .project
            .tracks
            .iter()
            .position(|track| track.id == target_track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(target_track_id.to_owned()))?;
        if source_index == target_index {
            return Ok(false);
        }
        if self.project.tracks[source_index].locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(
                self.project.tracks[source_index].id.clone(),
            ));
        }
        if self.project.tracks[target_index].locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(target_track_id.to_owned()));
        }
        let item_index = self.project.tracks[source_index]
            .items
            .iter()
            .position(|item| item.id == clip_id)
            .expect("source track contains the clip");
        let expected = timeline_content_track_kind(
            &self.project.tracks[source_index].items[item_index].content,
        );
        let actual = self.project.tracks[target_index].kind;
        if expected != actual {
            return Err(EditorDocumentError::IncompatibleClipTrack {
                clip: clip_id.to_owned(),
                track: target_track_id.to_owned(),
                expected,
                actual,
            });
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let item = self.project.tracks[source_index].items.remove(item_index);
        self.project.tracks[target_index].items.push(item);
        self.record_mutation(before, before_revision);
        Ok(true)
    }

    pub fn toggle_track_solo(&mut self, track_id: &str) -> Result<bool, EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        let solo = track.solo != Some(true);
        track.solo = solo.then_some(true);
        self.record_mutation(before, before_revision);
        Ok(solo)
    }

    pub fn set_master_volume(&mut self, volume: f64) -> Result<(), EditorDocumentError> {
        if !volume.is_finite() || volume < 0.0 {
            return Err(EditorDocumentError::InvalidMasterVolume(volume));
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.settings.master_volume = (volume != 1.0).then_some(volume);
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    /// The `.tsx` entry `TimelineContent::Component` items resolve against,
    /// as stored in `project.json` (relative to the project file, like an
    /// asset path) — see `ProjectSettings::react_entry`.
    pub fn react_entry(&self) -> Option<&str> {
        self.project.settings.react_entry.as_deref()
    }

    /// Resolves the stored `react_entry` to an absolute path, the same way
    /// `local_asset_path` resolves a relative asset path, so a caller can
    /// hand it straight to `ReactBridge::spawn`.
    pub fn react_entry_absolute_path(&self) -> Option<PathBuf> {
        let entry = self.project.settings.react_entry.as_deref()?;
        let path = Path::new(entry);
        Some(if path.is_absolute() {
            path.to_owned()
        } else {
            self.asset_root.join(path)
        })
    }

    /// Records `path` as the project's React entry, storing it relative to
    /// the project file the same way an imported asset's path is stored.
    pub fn set_react_entry(&mut self, path: impl AsRef<Path>) -> Result<(), EditorDocumentError> {
        let path = path.as_ref();
        let canonical =
            fs::canonicalize(path).map_err(|source| EditorDocumentError::ImportAsset {
                path: path.to_owned(),
                source,
            })?;
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.settings.react_entry = Some(self.serialized_asset_path(&canonical));
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn clear_react_entry(&mut self) {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.settings.react_entry = None;
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
    }

    /// Sets one prop on a `TimelineContent::Component` clip's `props` map,
    /// creating the map if this is the clip's first configured prop. Errors
    /// if `clip_id` is not a Component clip — there is nothing else on a
    /// Video/Text/etc. clip a "component prop" could mean.
    pub fn set_component_prop(
        &mut self,
        clip_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let TimelineContent::Component { props, .. } =
            &mut self.project.tracks[track_index].items[item_index].content
        else {
            return Err(EditorDocumentError::UnsupportedComponentProp(
                clip_id.to_owned(),
            ));
        };
        props
            .get_or_insert_with(BTreeMap::new)
            .insert(key.to_owned(), value);
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    /// The project's GUI-editable `properties` map. React entries read these
    /// values through `useProjectProperty(key, defaultValue)`; which fields
    /// exist and how the Inspector presents them comes from the entry's
    /// `defineProjectProperties()` schema, which is advisory Inspector
    /// metadata — this map itself stays schema-free.
    pub fn project_properties(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.project.properties
    }

    /// Upserts one project property value, undoable like every other
    /// document mutation. There is no declared-key or type whitelist here:
    /// a schema only describes how to present fields, so writing an
    /// unlisted key stays legal.
    pub fn set_project_property(&mut self, key: &str, value: serde_json::Value) {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.properties.insert(key.to_owned(), value);
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
    }

    /// Removes one project property value. Removing an absent key changes
    /// nothing and records no undo entry.
    pub fn remove_project_property(&mut self, key: &str) {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.properties.remove(key);
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
    }

    pub fn set_clip_volume_static(
        &mut self,
        clip_id: &str,
        volume: f64,
    ) -> Result<(), EditorDocumentError> {
        validate_clip_volume(volume)?;
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let field = clip_volume_field_mut(
            clip_id,
            &mut self.project.tracks[track_index].items[item_index].content,
        )?;
        *field = (volume != 1.0).then_some(Animatable::Static(volume));
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn set_clip_volume_keyframe(
        &mut self,
        clip_id: &str,
        local_time: Time,
        volume: f64,
    ) -> Result<(), EditorDocumentError> {
        validate_clip_volume(volume)?;
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let duration = self.project.tracks[track_index].items[item_index]
            .range
            .duration;
        validate_clip_local_time(clip_id, local_time, duration)?;
        let field = clip_volume_field_mut(
            clip_id,
            &mut self.project.tracks[track_index].items[item_index].content,
        )?;
        let existing = field.clone().unwrap_or(Animatable::Static(1.0));
        let mut keyframes = match existing {
            Animatable::Static(value) => {
                let mut keyframes = vec![Keyframe {
                    time: Time::ZERO,
                    value,
                    easing: None,
                }];
                if local_time != Time::ZERO {
                    keyframes.push(Keyframe {
                        time: local_time,
                        value: volume,
                        easing: None,
                    });
                } else {
                    keyframes[0].value = volume;
                }
                keyframes
            }
            Animatable::Keyframes(animation) => animation.keyframes,
        };
        if let Some(keyframe) = keyframes.iter_mut().find(|keyframe| {
            keyframe
                .time
                .cmp_exact(local_time)
                .is_ok_and(|ordering| ordering.is_eq())
        }) {
            keyframe.value = volume;
        } else {
            keyframes.push(Keyframe {
                time: local_time,
                value: volume,
                easing: None,
            });
            keyframes.sort_by(|left, right| {
                left.time
                    .cmp_exact(right.time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        *field = Some(Animatable::Keyframes(KeyframeAnimation {
            kind: KeyframeAnimationType::Keyframes,
            keyframes,
        }));
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn remove_clip_volume_keyframe(
        &mut self,
        clip_id: &str,
        local_time: Time,
    ) -> Result<bool, EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let duration = self.project.tracks[track_index].items[item_index]
            .range
            .duration;
        validate_clip_local_time(clip_id, local_time, duration)?;
        let field = clip_volume_field_mut(
            clip_id,
            &mut self.project.tracks[track_index].items[item_index].content,
        )?;
        let Some(Animatable::Keyframes(animation)) = field else {
            return Ok(false);
        };
        let Some(index) = animation.keyframes.iter().position(|keyframe| {
            keyframe
                .time
                .cmp_exact(local_time)
                .is_ok_and(|ordering| ordering.is_eq())
        }) else {
            return Ok(false);
        };
        animation.keyframes.remove(index);
        if animation.keyframes.len() == 1 {
            *field = Some(Animatable::Static(animation.keyframes[0].value));
        } else if animation.keyframes.is_empty() {
            *field = None;
        }
        self.record_mutation(before, before_revision);
        Ok(true)
    }

    pub fn flatten_clip_volume(
        &mut self,
        clip_id: &str,
        local_time: Time,
    ) -> Result<(), EditorDocumentError> {
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        let duration = self.project.tracks[track_index].items[item_index]
            .range
            .duration;
        validate_clip_local_time(clip_id, local_time, duration)?;
        let volume = match &self.project.tracks[track_index].items[item_index].content {
            TimelineContent::Video { volume, .. }
            | TimelineContent::Audio { volume, .. }
            | TimelineContent::Dialogue {
                audio: Some(_),
                volume,
                ..
            } => match volume {
                Some(volume) => evaluate_f64(volume, local_time).map_err(|_| {
                    EditorDocumentError::InvalidClipVolumeTime {
                        clip: clip_id.to_owned(),
                        time: local_time,
                    }
                })?,
                None => 1.0,
            },
            _ => {
                return Err(EditorDocumentError::UnsupportedClipVolume(
                    clip_id.to_owned(),
                ));
            }
        };
        self.set_clip_volume_static(clip_id, volume)
    }

    pub fn import_assets(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Vec<AssetSummary>, EditorDocumentError> {
        let mut prepared = Vec::new();
        let mut reserved_ids = self.project.assets.keys().cloned().collect::<Vec<_>>();
        for path in paths {
            let kind = asset_kind_for_path(&path)?;
            let canonical =
                fs::canonicalize(&path).map_err(|source| EditorDocumentError::ImportAsset {
                    path: path.clone(),
                    source,
                })?;
            if !canonical.is_file() {
                return Err(EditorDocumentError::UnsupportedAsset(path));
            }
            let stem = canonical
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("asset");
            let id = unique_id(&slugify(stem), reserved_ids.iter().map(String::as_str));
            reserved_ids.push(id.clone());
            let name = canonical
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned);
            let source = AssetSource::File {
                path: self.serialized_asset_path(&canonical),
            };
            let asset = match kind {
                AssetKind::Video => Asset::Video { name, source },
                AssetKind::Audio => Asset::Audio { name, source },
                AssetKind::Image => Asset::Image { name, source },
                AssetKind::Font => Asset::Font { name, source },
            };
            prepared.push((id, kind, asset));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }

        let before = self.project.clone();
        let before_revision = self.current_revision;
        let summaries = prepared
            .into_iter()
            .map(|(id, kind, asset)| {
                let path = local_asset_path(&asset, &self.asset_root);
                self.project.assets.insert(id.clone(), asset);
                AssetSummary {
                    id,
                    kind,
                    path,
                    missing: false,
                }
            })
            .collect();
        self.record_mutation(before, before_revision);
        Ok(summaries)
    }

    pub fn insert_asset_clip(
        &mut self,
        asset_id: &str,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<String, EditorDocumentError> {
        self.insert_asset_clip_on_track(asset_id, None, start_frame, duration_frames)
    }

    pub fn insert_asset_clip_on_track(
        &mut self,
        asset_id: &str,
        target_track_id: Option<&str>,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<String, EditorDocumentError> {
        let asset = self
            .project
            .assets
            .get(asset_id)
            .ok_or_else(|| EditorDocumentError::MissingAsset(asset_id.to_owned()))?;
        let kind = asset.kind();
        if kind == AssetKind::Font {
            return Err(EditorDocumentError::UnsupportedTimelineAsset(kind));
        }
        let range = self.new_clip_range(start_frame, duration_frames)?;

        let before = self.project.clone();
        let before_revision = self.current_revision;
        let track_kind = match kind {
            AssetKind::Video => TrackKind::Video,
            AssetKind::Audio => TrackKind::Audio,
            AssetKind::Image => TrackKind::Overlay,
            AssetKind::Font => unreachable!("font assets were rejected"),
        };
        if let Some(track_id) = target_track_id {
            let track = self
                .project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
            if track.locked == Some(true) {
                return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
            }
            if track.kind != track_kind {
                return Err(EditorDocumentError::IncompatibleTrack {
                    asset: asset_id.to_owned(),
                    track: track_id.to_owned(),
                    expected: track_kind,
                    actual: track.kind,
                });
            }
        }
        let item_id = unique_id(
            asset_id,
            self.project
                .tracks
                .iter()
                .flat_map(|track| &track.items)
                .map(|item| item.id.as_str()),
        );
        let name = asset_name(asset).unwrap_or(asset_id).to_owned();
        let transform = matches!(kind, AssetKind::Video | AssetKind::Image).then(|| Transform {
            position: Some(AnimatablePoint {
                x: Some(Animatable::Static(
                    f64::from(self.project.settings.width) / 2.0,
                )),
                y: Some(Animatable::Static(
                    f64::from(self.project.settings.height) / 2.0,
                )),
            }),
            ..Transform::default()
        });
        let content = match kind {
            AssetKind::Video => TimelineContent::Video {
                asset: asset_id.to_owned(),
                source_range: None,
                playback_rate: None,
                volume: None,
                muted: None,
            },
            AssetKind::Audio => TimelineContent::Audio {
                asset: asset_id.to_owned(),
                source_range: None,
                playback_rate: None,
                volume: None,
                muted: None,
            },
            AssetKind::Image => TimelineContent::Image {
                asset: asset_id.to_owned(),
            },
            AssetKind::Font => unreachable!("font assets were rejected"),
        };
        let item = TimelineItem {
            id: item_id.clone(),
            name: Some(name),
            range,
            content,
            enabled: None,
            transform,
            opacity: None,
        };
        if let Some(track_id) = target_track_id {
            self.project
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
                .expect("target track was validated")
                .items
                .push(item);
        } else if let Some(track) = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.kind == track_kind && track.locked != Some(true))
        {
            track.items.push(item);
        } else {
            let base = match track_kind {
                TrackKind::Video => "video",
                TrackKind::Audio => "audio",
                TrackKind::Overlay => "overlays",
                TrackKind::Dialogue => unreachable!(),
            };
            let track_id = unique_id(
                base,
                self.project.tracks.iter().map(|track| track.id.as_str()),
            );
            let name = match track_kind {
                TrackKind::Video => "Video",
                TrackKind::Audio => "Audio",
                TrackKind::Overlay => "Overlays",
                TrackKind::Dialogue => unreachable!(),
            };
            self.project.tracks.push(Track {
                id: track_id,
                name: name.to_owned(),
                kind: track_kind,
                enabled: None,
                locked: None,
                muted: None,
                solo: None,
                items: vec![item],
            });
        }
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(item_id)
    }

    pub fn insert_dialogue_clip(
        &mut self,
        audio_asset_id: &str,
        character_id: &str,
        target_track_id: Option<&str>,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<String, EditorDocumentError> {
        let asset = self
            .project
            .assets
            .get(audio_asset_id)
            .ok_or_else(|| EditorDocumentError::MissingAsset(audio_asset_id.to_owned()))?;
        if asset.kind() != AssetKind::Audio {
            return Err(EditorDocumentError::AssetKindMismatch {
                asset: audio_asset_id.to_owned(),
                expected: AssetKind::Audio,
                actual: asset.kind(),
            });
        }
        if !self.project.characters.contains_key(character_id) {
            return Err(EditorDocumentError::MissingCharacter(
                character_id.to_owned(),
            ));
        }
        let range = self.new_clip_range(start_frame, duration_frames)?;
        if let Some(track_id) = target_track_id {
            let track = self
                .project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
            if track.locked == Some(true) {
                return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
            }
            if track.kind != TrackKind::Dialogue {
                return Err(EditorDocumentError::IncompatibleTrack {
                    asset: audio_asset_id.to_owned(),
                    track: track_id.to_owned(),
                    expected: TrackKind::Dialogue,
                    actual: track.kind,
                });
            }
        }

        let before = self.project.clone();
        let before_revision = self.current_revision;
        let item_id = unique_id(
            "dialogue",
            self.project
                .tracks
                .iter()
                .flat_map(|track| &track.items)
                .map(|item| item.id.as_str()),
        );
        let name = asset_name(asset).unwrap_or(audio_asset_id).to_owned();
        let item = TimelineItem {
            id: item_id.clone(),
            name: Some(name.clone()),
            range,
            content: TimelineContent::Dialogue {
                character: character_id.to_owned(),
                text: name,
                audio: Some(audio_asset_id.to_owned()),
                volume: None,
                expression: None,
                lip_sync: Vec::new(),
            },
            enabled: None,
            transform: None,
            opacity: None,
        };
        if let Some(track_id) = target_track_id {
            if let Some(track) = self
                .project
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
            {
                track.items.push(item);
            }
        } else if let Some(track) = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.kind == TrackKind::Dialogue && track.locked != Some(true))
        {
            track.items.push(item);
        } else {
            let track_id = unique_id(
                "dialogue",
                self.project.tracks.iter().map(|track| track.id.as_str()),
            );
            self.project.tracks.push(Track {
                id: track_id,
                name: "Dialogue".to_owned(),
                kind: TrackKind::Dialogue,
                enabled: None,
                locked: None,
                muted: None,
                solo: None,
                items: vec![item],
            });
        }
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(item_id)
    }

    /// Inserts a `TimelineContent::Component` clip for a registered React
    /// component. Component clips render onto an overlay track; with no
    /// `target_track_id` the first unlocked overlay track is used, or a new
    /// `Overlays` track is created. `props` is left unset — the Inspector
    /// configures it against the entry's schema afterwards.
    pub fn insert_component_clip_on_track(
        &mut self,
        component: &str,
        target_track_id: Option<&str>,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<String, EditorDocumentError> {
        let range = self.new_clip_range(start_frame, duration_frames)?;
        if let Some(track_id) = target_track_id {
            let track = self
                .project
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
            if track.locked == Some(true) {
                return Err(EditorDocumentError::LockedTrack(track_id.to_owned()));
            }
            if track.kind != TrackKind::Overlay {
                return Err(EditorDocumentError::IncompatibleTrack {
                    asset: component.to_owned(),
                    track: track_id.to_owned(),
                    expected: TrackKind::Overlay,
                    actual: track.kind,
                });
            }
        }

        let before = self.project.clone();
        let before_revision = self.current_revision;
        let item_id = unique_id(
            &slugify(component),
            self.project
                .tracks
                .iter()
                .flat_map(|track| &track.items)
                .map(|item| item.id.as_str()),
        );
        let item = TimelineItem {
            id: item_id.clone(),
            name: Some(component.to_owned()),
            range,
            content: TimelineContent::Component {
                component: component.to_owned(),
                props: None,
            },
            enabled: None,
            transform: None,
            opacity: None,
        };
        if let Some(track_id) = target_track_id {
            self.project
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
                .expect("target track was validated")
                .items
                .push(item);
        } else if let Some(track) = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.kind == TrackKind::Overlay && track.locked != Some(true))
        {
            track.items.push(item);
        } else {
            let track_id = unique_id(
                "overlays",
                self.project.tracks.iter().map(|track| track.id.as_str()),
            );
            self.project.tracks.push(Track {
                id: track_id,
                name: "Overlays".to_owned(),
                kind: TrackKind::Overlay,
                enabled: None,
                locked: None,
                muted: None,
                solo: None,
                items: vec![item],
            });
        }
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(item_id)
    }

    pub fn set_dialogue_text(
        &mut self,
        clip_id: &str,
        text: &str,
    ) -> Result<(), EditorDocumentError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(EditorDocumentError::InvalidDialogueText);
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let TimelineContent::Dialogue {
            text: current_text, ..
        } = &mut self.project.tracks[track_index].items[item_index].content
        else {
            return Err(EditorDocumentError::UnsupportedDialogueClip(
                clip_id.to_owned(),
            ));
        };
        *current_text = text.to_owned();
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn set_dialogue_character(
        &mut self,
        clip_id: &str,
        character_id: &str,
    ) -> Result<(), EditorDocumentError> {
        if !self.project.characters.contains_key(character_id) {
            return Err(EditorDocumentError::MissingCharacter(
                character_id.to_owned(),
            ));
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let TimelineContent::Dialogue {
            character,
            expression,
            ..
        } = &mut self.project.tracks[track_index].items[item_index].content
        else {
            return Err(EditorDocumentError::UnsupportedDialogueClip(
                clip_id.to_owned(),
            ));
        };
        if character == character_id {
            return Ok(());
        }
        *character = character_id.to_owned();
        *expression = None;
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn set_dialogue_expression(
        &mut self,
        clip_id: &str,
        expression_id: Option<&str>,
    ) -> Result<(), EditorDocumentError> {
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let TimelineContent::Dialogue { character, .. } =
            &self.project.tracks[track_index].items[item_index].content
        else {
            return Err(EditorDocumentError::UnsupportedDialogueClip(
                clip_id.to_owned(),
            ));
        };
        if let Some(expression_id) = expression_id {
            let expression_exists = self
                .project
                .characters
                .get(character)
                .and_then(|character| character.portrait.as_ref())
                .is_some_and(|portrait| portrait.expressions.contains_key(expression_id));
            if !expression_exists {
                return Err(EditorDocumentError::MissingCharacterExpression {
                    character: character.clone(),
                    expression: expression_id.to_owned(),
                });
            }
        }

        let before = self.project.clone();
        let before_revision = self.current_revision;
        let TimelineContent::Dialogue { expression, .. } =
            &mut self.project.tracks[track_index].items[item_index].content
        else {
            unreachable!("dialogue content was checked");
        };
        *expression = expression_id.map(str::to_owned);
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn generate_dialogue_lip_sync(
        &mut self,
        clip_id: &str,
        waveform: &[f32],
    ) -> Result<usize, EditorDocumentError> {
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let item = &self.project.tracks[track_index].items[item_index];
        let TimelineContent::Dialogue { audio, text, .. } = &item.content else {
            return Err(EditorDocumentError::UnsupportedDialogueClip(
                clip_id.to_owned(),
            ));
        };
        if audio.is_none() {
            return Err(EditorDocumentError::DialogueLipSyncRequiresAudio(
                clip_id.to_owned(),
            ));
        }
        let cues = lip_sync_cues_from_waveform(
            waveform,
            text,
            item.range.duration,
            self.project.settings.frame_rate,
        )
        .map_err(EditorDocumentError::Duration)?;
        let cue_count = cues.len();
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let TimelineContent::Dialogue { lip_sync, .. } =
            &mut self.project.tracks[track_index].items[item_index].content
        else {
            unreachable!("dialogue content was checked")
        };
        *lip_sync = cues;
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(cue_count)
    }

    pub fn clear_dialogue_lip_sync(&mut self, clip_id: &str) -> Result<(), EditorDocumentError> {
        let (track_index, item_index) = self.clip_indices(clip_id)?;
        self.ensure_track_unlocked(track_index)?;
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let TimelineContent::Dialogue { lip_sync, .. } =
            &mut self.project.tracks[track_index].items[item_index].content
        else {
            return Err(EditorDocumentError::UnsupportedDialogueClip(
                clip_id.to_owned(),
            ));
        };
        lip_sync.clear();
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    fn new_clip_range(
        &self,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<TimeRange, EditorDocumentError> {
        if start_frame < 0 || duration_frames < 1 {
            return Err(EditorDocumentError::InvalidNewClipFrameRange {
                start_frame,
                duration_frames,
            });
        }
        let end_frame = start_frame.checked_add(duration_frames).ok_or(
            EditorDocumentError::InvalidNewClipFrameRange {
                start_frame,
                duration_frames,
            },
        )?;
        if let Some(duration) = self.project.settings.duration {
            let clock = TimelineClock::new(duration, self.project.settings.frame_rate)
                .map_err(EditorDocumentError::Duration)?;
            if end_frame > clock.end_frame() {
                return Err(EditorDocumentError::InvalidNewClipFrameRange {
                    start_frame,
                    duration_frames,
                });
            }
        }
        Ok(TimeRange {
            start: Time::frames(start_frame, self.project.settings.frame_rate)
                .map_err(EditorDocumentError::Duration)?,
            duration: Time::frames(duration_frames, self.project.settings.frame_rate)
                .map_err(EditorDocumentError::Duration)?,
        })
    }

    pub fn asset_references(&self, asset_id: &str) -> Result<Vec<String>, EditorDocumentError> {
        if !self.project.assets.contains_key(asset_id) {
            return Err(EditorDocumentError::MissingAsset(asset_id.to_owned()));
        }
        let mut references = Vec::new();
        for track in &self.project.tracks {
            for item in &track.items {
                let referenced = match &item.content {
                    TimelineContent::Video { asset, .. }
                    | TimelineContent::Audio { asset, .. }
                    | TimelineContent::Image { asset } => asset == asset_id,
                    TimelineContent::Dialogue {
                        audio: Some(asset), ..
                    } => asset == asset_id,
                    _ => false,
                };
                if referenced {
                    references.push(format!("track `{}` / clip `{}`", track.id, item.id));
                }
            }
        }
        for (character_id, character) in &self.project.characters {
            if let Some(portrait) = &character.portrait {
                for (expression, asset) in &portrait.expressions {
                    if asset == asset_id {
                        references.push(format!(
                            "character `{character_id}` / expression `{expression}`"
                        ));
                    }
                }
                if let Some(lip_sync) = &portrait.lip_sync {
                    for (shape, asset) in [
                        ("a", &lip_sync.a),
                        ("i", &lip_sync.i),
                        ("u", &lip_sync.u),
                        ("e", &lip_sync.e),
                        ("o", &lip_sync.o),
                    ] {
                        if asset == asset_id {
                            references.push(format!(
                                "character `{character_id}` / lip sync {shape} mouth"
                            ));
                        }
                    }
                    if lip_sync.closed.as_deref() == Some(asset_id) {
                        references.push(format!(
                            "character `{character_id}` / lip sync closed mouth"
                        ));
                    }
                }
            }
        }
        Ok(references)
    }

    pub fn relink_asset(
        &mut self,
        asset_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), EditorDocumentError> {
        let path = path.as_ref();
        let expected = self
            .project
            .assets
            .get(asset_id)
            .ok_or_else(|| EditorDocumentError::MissingAsset(asset_id.to_owned()))?
            .kind();
        let actual = asset_kind_for_path(path)?;
        if actual != expected {
            return Err(EditorDocumentError::AssetKindMismatch {
                asset: asset_id.to_owned(),
                expected,
                actual,
            });
        }
        let canonical =
            fs::canonicalize(path).map_err(|source| EditorDocumentError::ImportAsset {
                path: path.to_owned(),
                source,
            })?;
        if !canonical.is_file() {
            return Err(EditorDocumentError::UnsupportedAsset(path.to_owned()));
        }
        let serialized_path = self.serialized_asset_path(&canonical);
        let before = self.project.clone();
        let before_revision = self.current_revision;
        let asset = self
            .project
            .assets
            .get_mut(asset_id)
            .expect("asset existence was checked");
        set_asset_source(
            asset,
            AssetSource::File {
                path: serialized_path,
            },
        );
        if self.project != before {
            self.record_mutation(before, before_revision);
        }
        Ok(())
    }

    pub fn remove_asset(
        &mut self,
        asset_id: &str,
        remove_references: bool,
    ) -> Result<(), EditorDocumentError> {
        let references = self.asset_references(asset_id)?;
        if !references.is_empty() && !remove_references {
            return Err(EditorDocumentError::AssetInUse {
                asset: asset_id.to_owned(),
                references,
            });
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        if remove_references {
            for track in &mut self.project.tracks {
                track.items.retain_mut(|item| match &mut item.content {
                    TimelineContent::Video { asset, .. }
                    | TimelineContent::Audio { asset, .. }
                    | TimelineContent::Image { asset } => asset != asset_id,
                    TimelineContent::Dialogue {
                        audio, lip_sync, ..
                    } => {
                        if audio.as_deref() == Some(asset_id) {
                            *audio = None;
                            lip_sync.clear();
                        }
                        true
                    }
                    _ => true,
                });
            }
            let mut cleared_lip_sync_characters = Vec::new();
            for (character_id, character) in &mut self.project.characters {
                let Some(portrait) = &mut character.portrait else {
                    continue;
                };
                portrait.expressions.retain(|_, asset| asset != asset_id);
                let removes_required_mouth = portrait.lip_sync.as_ref().is_some_and(|lip_sync| {
                    [
                        &lip_sync.a,
                        &lip_sync.i,
                        &lip_sync.u,
                        &lip_sync.e,
                        &lip_sync.o,
                    ]
                    .into_iter()
                    .any(|asset| asset == asset_id)
                });
                if removes_required_mouth {
                    portrait.lip_sync = None;
                    cleared_lip_sync_characters.push(character_id.clone());
                } else if let Some(lip_sync) = &mut portrait.lip_sync
                    && lip_sync.closed.as_deref() == Some(asset_id)
                {
                    lip_sync.closed = None;
                }
                if portrait.expressions.is_empty() {
                    character.portrait = None;
                } else if !portrait
                    .expressions
                    .contains_key(&portrait.default_expression)
                {
                    portrait.default_expression = portrait
                        .expressions
                        .keys()
                        .next()
                        .expect("portrait still has an expression")
                        .clone();
                }
            }
            let characters = &self.project.characters;
            for item in self
                .project
                .tracks
                .iter_mut()
                .flat_map(|track| &mut track.items)
            {
                let TimelineContent::Dialogue {
                    character,
                    expression,
                    lip_sync,
                    ..
                } = &mut item.content
                else {
                    continue;
                };
                let expression_is_valid = expression.as_ref().is_none_or(|expression| {
                    characters
                        .get(character)
                        .and_then(|character| character.portrait.as_ref())
                        .is_some_and(|portrait| portrait.expressions.contains_key(expression))
                });
                if !expression_is_valid {
                    *expression = None;
                }
                if cleared_lip_sync_characters.contains(character) {
                    lip_sync.clear();
                }
            }
        }
        self.project.assets.remove(asset_id);
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(())
    }

    pub fn delete_clip(&mut self, clip_id: &str) -> Result<(), EditorDocumentError> {
        let track_index = self
            .project
            .tracks
            .iter()
            .position(|track| track.items.iter().any(|item| item.id == clip_id))
            .ok_or_else(|| EditorDocumentError::MissingClip(clip_id.to_owned()))?;
        if self.project.tracks[track_index].locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(
                self.project.tracks[track_index].id.clone(),
            ));
        }
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.project.tracks[track_index]
            .items
            .retain(|item| item.id != clip_id);
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        self.record_mutation(before, before_revision);
        Ok(())
    }

    fn serialized_asset_path(&self, path: &Path) -> String {
        let root = fs::canonicalize(&self.asset_root).unwrap_or_else(|_| self.asset_root.clone());
        if self.path.is_some()
            && let Ok(relative) = path.strip_prefix(root)
        {
            format!("./{}", relative.to_string_lossy().replace('\\', "/"))
        } else {
            path.to_string_lossy().replace('\\', "/")
        }
    }

    pub fn edit_clip_frames(
        &mut self,
        clip_id: &str,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<(), EditorDocumentError> {
        let before = self.project.clone();
        let before_revision = self.current_revision;
        self.apply_clip_frames(clip_id, start_frame, duration_frames)?;
        if self.project == before {
            return Ok(());
        }
        if self.history_group.is_none() {
            self.record_history(HistoryState {
                project: before,
                revision: before_revision,
            });
        }
        Ok(())
    }

    fn clip_indices(&self, clip_id: &str) -> Result<(usize, usize), EditorDocumentError> {
        self.project
            .tracks
            .iter()
            .enumerate()
            .find_map(|(track_index, track)| {
                track
                    .items
                    .iter()
                    .position(|item| item.id == clip_id)
                    .map(|item_index| (track_index, item_index))
            })
            .ok_or_else(|| EditorDocumentError::MissingClip(clip_id.to_owned()))
    }

    fn ensure_track_unlocked(&self, track_index: usize) -> Result<(), EditorDocumentError> {
        let track = &self.project.tracks[track_index];
        if track.locked == Some(true) {
            Err(EditorDocumentError::LockedTrack(track.id.clone()))
        } else {
            Ok(())
        }
    }

    fn apply_clip_frames(
        &mut self,
        clip_id: &str,
        start_frame: i64,
        duration_frames: i64,
    ) -> Result<(), EditorDocumentError> {
        let frame_rate = self.project.settings.frame_rate;
        let clock =
            TimelineClock::new(self.duration, frame_rate).map_err(EditorDocumentError::Duration)?;
        let end_frame = start_frame.checked_add(duration_frames).ok_or(
            EditorDocumentError::InvalidClipFrameRange {
                start_frame,
                duration_frames,
                timeline_end_frame: clock.end_frame(),
            },
        )?;
        if start_frame < 0 || duration_frames < 1 || end_frame > clock.end_frame() {
            return Err(EditorDocumentError::InvalidClipFrameRange {
                start_frame,
                duration_frames,
                timeline_end_frame: clock.end_frame(),
            });
        }
        let (track_index, item_index) = self
            .project
            .tracks
            .iter()
            .enumerate()
            .find_map(|(track_index, track)| {
                track
                    .items
                    .iter()
                    .position(|item| item.id == clip_id)
                    .map(|item_index| (track_index, item_index))
            })
            .ok_or_else(|| EditorDocumentError::MissingClip(clip_id.to_owned()))?;
        if self.project.tracks[track_index].locked == Some(true) {
            return Err(EditorDocumentError::LockedTrack(
                self.project.tracks[track_index].id.clone(),
            ));
        }
        let item = &mut self.project.tracks[track_index].items[item_index];
        item.range.start =
            Time::frames(start_frame, frame_rate).map_err(EditorDocumentError::Duration)?;
        item.range.duration =
            Time::frames(duration_frames, frame_rate).map_err(EditorDocumentError::Duration)?;
        if self.project.settings.duration.is_none() {
            self.duration = self
                .project
                .effective_duration()
                .map_err(EditorDocumentError::Duration)?;
        }
        Ok(())
    }

    /// Starts a group of live mutations that will become one undo entry.
    pub fn begin_history_group(&mut self) {
        if self.history_group.is_none() {
            self.history_group = Some(HistoryState {
                project: self.project.clone(),
                revision: self.current_revision,
            });
        }
    }

    /// Commits the active history group, returning whether it changed the project.
    pub fn commit_history_group(&mut self) -> bool {
        let Some(before) = self.history_group.take() else {
            return false;
        };
        if before.project == self.project {
            return false;
        }
        self.record_history(before);
        true
    }

    pub fn undo(&mut self) -> Result<bool, EditorDocumentError> {
        self.commit_history_group();
        let Some(entry) = self.undo_stack.pop() else {
            return Ok(false);
        };
        self.restore_history_state(&entry.before)?;
        self.redo_stack.push(entry);
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditorDocumentError> {
        self.commit_history_group();
        let Some(entry) = self.redo_stack.pop() else {
            return Ok(false);
        };
        self.restore_history_state(&entry.after)?;
        self.undo_stack.push(entry);
        Ok(true)
    }

    /// Atomically saves the document to its current path.
    pub fn save(&mut self) -> Result<(), EditorDocumentError> {
        self.commit_history_group();
        let path = self
            .path
            .as_deref()
            .ok_or(EditorDocumentError::MissingSavePath)?;
        let json = self
            .project
            .to_json()
            .map_err(EditorDocumentError::Serialize)?;
        atomic_write(path, json.as_bytes()).map_err(EditorDocumentError::Save)?;
        self.clean_revision = self.current_revision;
        Ok(())
    }

    /// Atomically saves to a new path and adopts it as the document path.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), EditorDocumentError> {
        self.commit_history_group();
        let path = path.as_ref();
        let json = self
            .project
            .to_json()
            .map_err(EditorDocumentError::Serialize)?;
        atomic_write(path, json.as_bytes()).map_err(EditorDocumentError::Save)?;
        self.path = Some(path.to_path_buf());
        self.asset_root = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        self.clean_revision = self.current_revision;
        Ok(())
    }

    fn history_group_changed(&self) -> bool {
        self.history_group
            .as_ref()
            .is_some_and(|state| state.project != self.project)
    }

    fn record_history(&mut self, before: HistoryState) {
        let revision = self.next_revision;
        self.next_revision = self.next_revision.saturating_add(1);
        self.current_revision = revision;
        self.undo_stack.push(HistoryEntry {
            before,
            after: HistoryState {
                project: self.project.clone(),
                revision,
            },
        });
        self.redo_stack.clear();
    }

    fn record_mutation(&mut self, project: Project, revision: u64) {
        if self.history_group.is_none() {
            self.record_history(HistoryState { project, revision });
        }
    }

    fn restore_history_state(&mut self, state: &HistoryState) -> Result<(), EditorDocumentError> {
        let duration = state
            .project
            .effective_duration()
            .map_err(EditorDocumentError::Duration)?;
        self.project = state.project.clone();
        self.duration = duration;
        self.current_revision = state.revision;
        Ok(())
    }
}

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn asset_kind_for_path(path: &Path) -> Result<AssetKind, EditorDocumentError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| EditorDocumentError::UnsupportedAsset(path.to_owned()))?;
    match extension.as_str() {
        "mp4" | "mov" | "mkv" | "webm" | "m4v" | "avi" => Ok(AssetKind::Video),
        "wav" | "mp3" | "aac" | "m4a" | "flac" | "ogg" | "opus" => Ok(AssetKind::Audio),
        "png" | "jpg" | "jpeg" | "webp" | "pnm" | "ppm" | "pgm" | "pbm" => Ok(AssetKind::Image),
        "ttf" | "otf" | "ttc" => Ok(AssetKind::Font),
        _ => Err(EditorDocumentError::UnsupportedAsset(path.to_owned())),
    }
}

fn asset_name(asset: &Asset) -> Option<&str> {
    match asset {
        Asset::Video { name, .. }
        | Asset::Audio { name, .. }
        | Asset::Image { name, .. }
        | Asset::Font { name, .. } => name.as_deref(),
    }
}

fn track_defaults(kind: TrackKind) -> (&'static str, &'static str) {
    match kind {
        TrackKind::Video => ("video", "Video"),
        TrackKind::Audio => ("audio", "Audio"),
        TrackKind::Overlay => ("overlays", "Overlays"),
        TrackKind::Dialogue => ("dialogue", "Dialogue"),
    }
}

fn timeline_content_track_kind(content: &TimelineContent) -> TrackKind {
    match content {
        TimelineContent::Video { .. } => TrackKind::Video,
        TimelineContent::Audio { .. } => TrackKind::Audio,
        TimelineContent::Image { .. }
        | TimelineContent::Text { .. }
        | TimelineContent::Component { .. } => TrackKind::Overlay,
        TimelineContent::Dialogue { .. } => TrackKind::Dialogue,
    }
}

fn clip_volume_field_mut<'a>(
    clip_id: &str,
    content: &'a mut TimelineContent,
) -> Result<&'a mut Option<Animatable<f64>>, EditorDocumentError> {
    match content {
        TimelineContent::Video { volume, .. }
        | TimelineContent::Audio { volume, .. }
        | TimelineContent::Dialogue {
            audio: Some(_),
            volume,
            ..
        } => Ok(volume),
        _ => Err(EditorDocumentError::UnsupportedClipVolume(
            clip_id.to_owned(),
        )),
    }
}

fn validate_clip_volume(volume: f64) -> Result<(), EditorDocumentError> {
    if volume.is_finite() && volume >= 0.0 {
        Ok(())
    } else {
        Err(EditorDocumentError::InvalidClipVolume(volume))
    }
}

fn validate_clip_local_time(
    clip_id: &str,
    time: Time,
    duration: Time,
) -> Result<(), EditorDocumentError> {
    let within_range = time
        .cmp_exact(Time::ZERO)
        .is_ok_and(|ordering| !ordering.is_lt())
        && time
            .cmp_exact(duration)
            .is_ok_and(|ordering| !ordering.is_gt());
    if within_range {
        Ok(())
    } else {
        Err(EditorDocumentError::InvalidClipVolumeTime {
            clip: clip_id.to_owned(),
            time,
        })
    }
}

fn set_asset_source(asset: &mut Asset, new_source: AssetSource) {
    match asset {
        Asset::Video { source, .. }
        | Asset::Audio { source, .. }
        | Asset::Image { source, .. }
        | Asset::Font { source, .. } => *source = new_source,
    }
}

fn local_asset_path(asset: &Asset, asset_root: &Path) -> Option<PathBuf> {
    let source = match asset {
        Asset::Video { source, .. }
        | Asset::Audio { source, .. }
        | Asset::Image { source, .. }
        | Asset::Font { source, .. } => source,
    };
    match source {
        AssetSource::File { path } => {
            let path = Path::new(path);
            Some(if path.is_absolute() {
                path.to_owned()
            } else {
                asset_root.join(path)
            })
        }
        AssetSource::Url { .. } => None,
    }
}

fn slugify(input: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !output.is_empty() {
                output.push('-');
            }
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if output.is_empty() {
        "asset".to_owned()
    } else {
        output
    }
}

fn unique_id<'a>(base: &str, existing: impl IntoIterator<Item = &'a str>) -> String {
    let existing = existing.into_iter().collect::<Vec<_>>();
    if !existing.contains(&base) {
        return base.to_owned();
    }
    for suffix in 2_u64.. {
        let candidate = format!("{base}-{suffix}");
        if !existing.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("u64 ID suffixes were exhausted")
}

fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

#[derive(Debug)]
pub enum EditorDocumentError {
    Load(LoadError),
    Serialize(serde_json::Error),
    Save(std::io::Error),
    MissingSavePath,
    Duration(mikan_composition::TimeError),
    MissingClip(String),
    MissingTrack(String),
    MissingAsset(String),
    MissingCharacter(String),
    MissingCharacterPortrait(String),
    MissingCharacterExpression {
        character: String,
        expression: String,
    },
    LockedTrack(String),
    UnsupportedAsset(PathBuf),
    ImportAsset {
        path: PathBuf,
        source: std::io::Error,
    },
    AssetKindMismatch {
        asset: String,
        expected: AssetKind,
        actual: AssetKind,
    },
    IncompatibleTrack {
        asset: String,
        track: String,
        expected: TrackKind,
        actual: TrackKind,
    },
    IncompatibleClipTrack {
        clip: String,
        track: String,
        expected: TrackKind,
        actual: TrackKind,
    },
    AssetInUse {
        asset: String,
        references: Vec<String>,
    },
    CharacterInUse {
        character: String,
        references: Vec<String>,
    },
    UnsupportedTimelineAsset(AssetKind),
    InvalidNewClipFrameRange {
        start_frame: i64,
        duration_frames: i64,
    },
    InvalidMasterVolume(f64),
    InvalidTrackName,
    InvalidCharacterName,
    InvalidDialogueText,
    NonEmptyTrack {
        track: String,
        item_count: usize,
    },
    InvalidClipVolume(f64),
    InvalidClipVolumeTime {
        clip: String,
        time: Time,
    },
    UnsupportedClipVolume(String),
    InvalidClipFrameRange {
        start_frame: i64,
        duration_frames: i64,
        timeline_end_frame: i64,
    },
    UnsupportedComponentProp(String),
    UnsupportedDialogueClip(String),
    DialogueLipSyncRequiresAudio(String),
}

impl fmt::Display for EditorDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(formatter, "could not load project: {error}"),
            Self::Serialize(error) => write!(formatter, "could not serialize project: {error}"),
            Self::Save(error) => write!(formatter, "could not save project: {error}"),
            Self::MissingSavePath => formatter.write_str("project has no save path"),
            Self::Duration(error) => write!(formatter, "could not calculate duration: {error}"),
            Self::MissingClip(clip_id) => write!(formatter, "clip `{clip_id}` does not exist"),
            Self::MissingTrack(track_id) => write!(formatter, "track `{track_id}` does not exist"),
            Self::MissingAsset(asset_id) => write!(formatter, "asset `{asset_id}` does not exist"),
            Self::MissingCharacter(character_id) => {
                write!(formatter, "character `{character_id}` does not exist")
            }
            Self::MissingCharacterPortrait(character_id) => {
                write!(formatter, "character `{character_id}` has no portrait")
            }
            Self::MissingCharacterExpression {
                character,
                expression,
            } => write!(
                formatter,
                "character `{character}` has no expression `{expression}`"
            ),
            Self::LockedTrack(track_id) => write!(formatter, "track `{track_id}` is locked"),
            Self::UnsupportedAsset(path) => {
                write!(formatter, "unsupported asset file `{}`", path.display())
            }
            Self::ImportAsset { path, source } => {
                write!(formatter, "could not import `{}`: {source}", path.display())
            }
            Self::AssetKindMismatch {
                asset,
                expected,
                actual,
            } => write!(
                formatter,
                "asset `{asset}` is {expected}, but the replacement file is {actual}"
            ),
            Self::IncompatibleTrack {
                asset,
                track,
                expected,
                actual,
            } => write!(
                formatter,
                "asset `{asset}` requires a {expected:?} track, but `{track}` is {actual:?}"
            ),
            Self::IncompatibleClipTrack {
                clip,
                track,
                expected,
                actual,
            } => write!(
                formatter,
                "clip `{clip}` requires a {expected:?} track, but `{track}` is {actual:?}"
            ),
            Self::AssetInUse { asset, references } => write!(
                formatter,
                "asset `{asset}` is still used by {} reference(s)",
                references.len()
            ),
            Self::CharacterInUse {
                character,
                references,
            } => write!(
                formatter,
                "character `{character}` is still used by {} dialogue clip(s)",
                references.len()
            ),
            Self::UnsupportedTimelineAsset(kind) => {
                write!(formatter, "{kind} assets cannot be placed on the timeline")
            }
            Self::InvalidNewClipFrameRange {
                start_frame,
                duration_frames,
            } => write!(
                formatter,
                "new clip frame range {start_frame}+{duration_frames} is outside the timeline"
            ),
            Self::InvalidMasterVolume(volume) => {
                write!(
                    formatter,
                    "master volume {volume} must be finite and non-negative"
                )
            }
            Self::InvalidTrackName => formatter.write_str("track name must not be empty"),
            Self::InvalidCharacterName => formatter.write_str("character name must not be empty"),
            Self::InvalidDialogueText => formatter.write_str("dialogue text must not be empty"),
            Self::NonEmptyTrack { track, item_count } => write!(
                formatter,
                "track `{track}` still contains {item_count} clip(s)"
            ),
            Self::InvalidClipVolume(volume) => {
                write!(
                    formatter,
                    "clip volume {volume} must be finite and non-negative"
                )
            }
            Self::InvalidClipVolumeTime { clip, time } => write!(
                formatter,
                "volume keyframe time {time:?} is outside clip `{clip}`"
            ),
            Self::UnsupportedClipVolume(clip) => {
                write!(formatter, "clip `{clip}` does not have an audio volume")
            }
            Self::InvalidClipFrameRange {
                start_frame,
                duration_frames,
                timeline_end_frame,
            } => write!(
                formatter,
                "clip frame range {start_frame}+{duration_frames} is outside 0..{timeline_end_frame}"
            ),
            Self::UnsupportedComponentProp(clip) => {
                write!(formatter, "clip `{clip}` is not a registered component")
            }
            Self::UnsupportedDialogueClip(clip) => {
                write!(formatter, "clip `{clip}` is not dialogue")
            }
            Self::DialogueLipSyncRequiresAudio(clip) => {
                write!(formatter, "dialogue clip `{clip}` has no voice asset")
            }
        }
    }
}

impl Error for EditorDocumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::Serialize(error) => Some(error),
            Self::Save(error) => Some(error),
            Self::ImportAsset { source, .. } => Some(source),
            Self::Duration(error) => Some(error),
            Self::MissingSavePath
            | Self::MissingClip(_)
            | Self::MissingTrack(_)
            | Self::MissingAsset(_)
            | Self::MissingCharacter(_)
            | Self::MissingCharacterPortrait(_)
            | Self::MissingCharacterExpression { .. }
            | Self::LockedTrack(_)
            | Self::UnsupportedAsset(_)
            | Self::AssetKindMismatch { .. }
            | Self::IncompatibleTrack { .. }
            | Self::IncompatibleClipTrack { .. }
            | Self::AssetInUse { .. }
            | Self::CharacterInUse { .. }
            | Self::UnsupportedTimelineAsset(_)
            | Self::InvalidNewClipFrameRange { .. }
            | Self::InvalidMasterVolume(_)
            | Self::InvalidTrackName
            | Self::InvalidCharacterName
            | Self::InvalidDialogueText
            | Self::NonEmptyTrack { .. }
            | Self::InvalidClipVolume(_)
            | Self::InvalidClipVolumeTime { .. }
            | Self::UnsupportedClipVolume(_)
            | Self::InvalidClipFrameRange { .. }
            | Self::UnsupportedComponentProp(_)
            | Self::UnsupportedDialogueClip(_)
            | Self::DialogueLipSyncRequiresAudio(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mikan_composition::LayerContent;

    const MINIMAL: &str = include_str!("../../../examples/minimal.mikan.json");
    const EDITOR_DEMO: &str = include_str!("../../../examples/editor-demo.mikan.json");
    const VOICEROID: &str = include_str!("../../../examples/voiceroid.mikan.json");

    #[test]
    fn loads_an_empty_document() {
        let document = EditorDocument::from_json(MINIMAL, ".").unwrap();

        assert_eq!(document.display_name(), "Untitled");
        assert!(document.assets().is_empty());
        assert!(document.tracks().is_empty());
        assert_eq!(document.scene_at(Time::ZERO).unwrap().layers.len(), 0);
    }

    #[test]
    fn exposes_editor_summaries_without_changing_the_project() {
        let document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        assert_eq!(document.assets().len(), 2);
        assert_eq!(document.tracks()[0].kind, TrackKind::Dialogue);
        assert_eq!(document.tracks()[0].item_count, 1);
        assert_eq!(document.tracks()[0].clips[0].start, Time::new(5, 1));
        assert_eq!(document.tracks()[0].clips[0].duration, Time::new(3, 1));
        assert_eq!(document.tracks()[0].clips[0].kind, ClipKind::Dialogue);
        assert_eq!(document.duration(), Time::new(8, 1));
    }

    #[test]
    fn timeline_clock_uses_exact_project_frames_and_clamps() {
        let mut clock = TimelineClock::new(Time::new(3, 2), Rational::new(60, 1)).unwrap();

        assert_eq!(clock.end_frame(), 90);
        clock.seek(75);
        assert_eq!(clock.time().unwrap(), Time::new(75, 60));
        clock.step(30);
        assert_eq!(clock.frame(), 90);
        assert!(clock.is_at_end());
        clock.step(-100);
        assert_eq!(clock.frame(), 0);
        assert_eq!(clock.frame_for_time(Time::new(5, 4)).unwrap(), 75);
    }

    #[test]
    fn timeline_clock_rounds_partial_final_frames_up() {
        let clock = TimelineClock::new(Time::new(1, 100), Rational::new(60, 1)).unwrap();

        assert_eq!(clock.end_frame(), 1);
    }

    #[test]
    fn timeline_clock_maps_scrub_positions_to_clamped_frames() {
        let mut clock = TimelineClock::new(Time::new(10, 1), Rational::new(60, 1)).unwrap();

        assert_eq!(clock.frame_at_fraction(0.25), 150);
        clock.seek_fraction(0.75);
        assert_eq!(clock.frame(), 450);
        clock.seek_fraction(2.0);
        assert_eq!(clock.frame(), 600);
        clock.seek_fraction(f32::NAN);
        assert_eq!(clock.frame(), 0);
    }

    #[test]
    fn lip_sync_waveform_generation_is_frame_aligned_and_uses_hysteresis() {
        let cues = lip_sync_cues_from_waveform(
            &[0.0, 1.0, 1.0, 0.0],
            "あい",
            Time::new(4, 1),
            Rational::new(1, 1),
        )
        .unwrap();

        assert_eq!(
            cues,
            vec![
                LipSyncCue {
                    time: Time::ZERO,
                    shape: MouthShape::Closed,
                },
                LipSyncCue {
                    time: Time::new(1, 1),
                    shape: MouthShape::A,
                },
                LipSyncCue {
                    time: Time::new(2, 1),
                    shape: MouthShape::I,
                },
                LipSyncCue {
                    time: Time::new(3, 1),
                    shape: MouthShape::Closed,
                },
            ]
        );
    }

    #[test]
    fn lip_sync_text_mapping_handles_small_kana_and_long_vowels() {
        assert_eq!(
            vowel_shapes("キャットーA"),
            vec![MouthShape::A, MouthShape::O, MouthShape::O, MouthShape::A]
        );
    }

    #[test]
    fn character_mouth_assets_and_dialogue_cues_are_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        document
            .set_character_lip_sync_asset("akane", MouthShape::A, "akane-default")
            .unwrap();
        document
            .set_character_lip_sync_asset("akane", MouthShape::Closed, "akane-default")
            .unwrap();
        document.clear_character_closed_mouth("akane").unwrap();
        assert!(
            document.characters()[0]
                .lip_sync
                .as_ref()
                .unwrap()
                .closed
                .is_none()
        );
        document
            .set_character_lip_sync_asset("akane", MouthShape::Closed, "akane-default")
            .unwrap();
        let cue_count = document
            .generate_dialogue_lip_sync("dialogue-001", &[0.0, 0.0, 1.0, 1.0, 0.0, 0.0])
            .unwrap();

        assert!(document.characters()[0].lip_sync.is_some());
        assert!(cue_count >= 3);
        assert_eq!(
            document.tracks()[0].clips[0]
                .dialogue
                .as_ref()
                .unwrap()
                .lip_sync_cue_count,
            cue_count
        );
        let scene = document.scene_at(Time::new(13, 2)).unwrap();
        let LayerContent::Group { layers } = &scene.layers[0].content else {
            panic!("dialogue must render as a group")
        };
        assert_eq!(layers.len(), 3);

        assert!(document.undo().unwrap());
        assert_eq!(
            document.tracks()[0].clips[0]
                .dialogue
                .as_ref()
                .unwrap()
                .lip_sync_cue_count,
            0
        );
    }

    #[test]
    fn loads_the_editor_demo_timeline() {
        let document = EditorDocument::from_json(
            include_str!("../../../examples/editor-demo.mikan.json"),
            "examples",
        )
        .unwrap();

        assert_eq!(document.duration(), Time::new(10, 1));
        assert_eq!(document.tracks()[0].clips[0].start, Time::ZERO);
        assert_eq!(document.tracks()[0].clips[0].duration, Time::new(10, 1));
    }

    #[test]
    fn voiceroid_example_renders_during_dialogue_when_a_gpu_is_available() {
        let asset_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let document = EditorDocument::from_json(VOICEROID, asset_root).unwrap();
        let Ok(mut renderer) =
            mikan_gpu_renderer::GpuRenderer::new(mikan_gpu_renderer::GpuRenderOptions::default())
        else {
            return;
        };
        renderer = renderer.with_asset_root(document.asset_root());

        let scene = document.scene_at(Time::new(11, 2)).unwrap();
        let frame = renderer.render(&scene).unwrap();

        assert_eq!(frame.width(), 1920);
        assert_eq!(frame.height(), 1080);
    }

    #[test]
    fn edits_clip_ranges_in_exact_project_frames() {
        let mut document = EditorDocument::from_json(
            include_str!("../../../examples/editor-demo.mikan.json"),
            "examples",
        )
        .unwrap();

        document.edit_clip_frames("welcome", 60, 480).unwrap();

        let clip = &document.tracks()[0].clips[0];
        assert_eq!(clip.start, Time::new(60, 60));
        assert_eq!(clip.duration, Time::new(480, 60));
        assert!(document.edit_clip_frames("welcome", -1, 480).is_err());
        assert!(document.edit_clip_frames("welcome", 60, 0).is_err());
        assert!(document.edit_clip_frames("welcome", 60, 541).is_err());
    }

    #[test]
    fn undo_redo_tracks_revisions_and_discards_redo_branches() {
        let mut document = EditorDocument::from_json(
            include_str!("../../../examples/editor-demo.mikan.json"),
            "examples",
        )
        .unwrap();

        document.edit_clip_frames("welcome", 30, 540).unwrap();
        assert!(document.is_dirty());
        assert!(document.can_undo());
        assert_eq!(document.tracks()[0].clips[0].start, Time::new(30, 60));

        assert!(document.undo().unwrap());
        assert!(!document.is_dirty());
        assert!(document.can_redo());
        assert_eq!(document.tracks()[0].clips[0].start, Time::ZERO);

        assert!(document.redo().unwrap());
        assert!(document.is_dirty());
        assert_eq!(document.tracks()[0].clips[0].start, Time::new(30, 60));

        assert!(document.undo().unwrap());
        document.edit_clip_frames("welcome", 60, 480).unwrap();
        assert!(!document.can_redo());
        assert_eq!(document.tracks()[0].clips[0].start, Time::new(60, 60));
    }

    #[test]
    fn coalesces_a_live_clip_drag_into_one_history_entry() {
        let mut document = EditorDocument::from_json(
            include_str!("../../../examples/editor-demo.mikan.json"),
            "examples",
        )
        .unwrap();

        document.begin_history_group();
        document.edit_clip_frames("welcome", 10, 580).unwrap();
        document.edit_clip_frames("welcome", 20, 560).unwrap();
        document.edit_clip_frames("welcome", 30, 540).unwrap();
        assert!(document.is_dirty());
        assert!(document.commit_history_group());

        assert!(document.undo().unwrap());
        assert_eq!(document.tracks()[0].clips[0].start, Time::ZERO);
        assert!(!document.undo().unwrap());
    }

    #[test]
    fn creates_reorders_and_moves_clips_between_compatible_tracks() {
        let mut document = EditorDocument::from_json(EDITOR_DEMO, ".").unwrap();
        let second_overlay = document.add_track(TrackKind::Overlay);
        let audio = document.add_track(TrackKind::Audio);
        assert_eq!(second_overlay, "overlays");
        assert_eq!(audio, "audio");

        assert!(
            document
                .move_clip_to_track("welcome", &second_overlay)
                .unwrap()
        );
        assert!(
            document
                .tracks()
                .iter()
                .find(|track| track.id == second_overlay)
                .unwrap()
                .clips
                .iter()
                .any(|clip| clip.id == "welcome")
        );
        assert!(matches!(
            document.move_clip_to_track("welcome", &audio),
            Err(EditorDocumentError::IncompatibleClipTrack { .. })
        ));

        assert!(document.move_track(&second_overlay, -1).unwrap());
        assert_eq!(document.tracks()[0].id, second_overlay);
        assert!(document.undo().unwrap());
        assert_eq!(document.tracks()[1].id, second_overlay);
        assert!(document.undo().unwrap());
        assert!(
            document.tracks()[0]
                .clips
                .iter()
                .any(|clip| clip.id == "welcome")
        );
    }

    #[test]
    fn renames_toggles_and_safely_deletes_tracks() {
        let mut document = EditorDocument::from_json(EDITOR_DEMO, ".").unwrap();

        document.rename_track("titles", "  Main titles  ").unwrap();
        assert_eq!(document.tracks()[0].name, "Main titles");
        assert!(!document.toggle_track_enabled("titles").unwrap());
        assert!(!document.tracks()[0].enabled);
        assert!(matches!(
            document.delete_track("titles", false),
            Err(EditorDocumentError::NonEmptyTrack { item_count: 1, .. })
        ));

        assert!(document.toggle_track_locked("titles").unwrap());
        assert!(matches!(
            document.rename_track("titles", "Locked"),
            Err(EditorDocumentError::LockedTrack(_))
        ));
        assert!(!document.toggle_track_locked("titles").unwrap());
        document.delete_track("titles", true).unwrap();
        assert!(document.tracks().is_empty());

        assert!(document.undo().unwrap());
        assert_eq!(document.tracks()[0].name, "Main titles");
        assert!(!document.tracks()[0].enabled);
    }

    #[test]
    fn track_audio_controls_and_master_volume_are_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        assert!(document.toggle_track_muted("dialogue").unwrap());
        assert!(document.tracks()[0].muted);
        assert!(document.audio_graph().unwrap().clips.is_empty());

        assert!(document.toggle_track_solo("dialogue").unwrap());
        assert!(document.tracks()[0].solo);
        document.set_master_volume(0.65).unwrap();
        assert_eq!(document.master_volume(), 0.65);
        assert_eq!(document.audio_graph().unwrap().master_volume, 0.65);

        assert!(document.undo().unwrap());
        assert_eq!(document.master_volume(), 1.0);
        assert!(document.undo().unwrap());
        assert!(!document.tracks()[0].solo);
        assert!(document.undo().unwrap());
        assert!(!document.tracks()[0].muted);
    }

    #[test]
    fn react_entry_and_component_props_are_undoable() {
        const WITH_COMPONENT: &str = r#"{
            "version": 0,
            "settings": {
                "width": 1920,
                "height": 1080,
                "frameRate": {"numerator": 30, "denominator": 1},
                "sampleRate": 48000
            },
            "assets": {},
            "characters": {},
            "tracks": [
                {
                    "id": "overlays",
                    "name": "Overlays",
                    "kind": "overlay",
                    "items": [
                        {
                            "id": "boss-intro",
                            "range": {"start": {"value": 0, "timescale": 1}, "duration": {"value": 1, "timescale": 1}},
                            "content": {
                                "type": "component",
                                "component": "BossIntroduction",
                                "props": {"bossName": "Golem"}
                            }
                        }
                    ]
                }
            ],
            "properties": {}
        }"#;
        let mut document = EditorDocument::from_json(WITH_COMPONENT, "examples").unwrap();

        assert!(document.react_entry().is_none());
        assert!(
            document
                .set_component_prop("boss-intro", "level", serde_json::json!(42))
                .is_ok()
        );
        let component = document.tracks()[0].clips[0].component.clone().unwrap();
        assert_eq!(component.name, "BossIntroduction");
        assert_eq!(
            component.props.get("bossName"),
            Some(&serde_json::json!("Golem"))
        );
        assert_eq!(component.props.get("level"), Some(&serde_json::json!(42)));

        assert!(document.undo().unwrap());
        let component = document.tracks()[0].clips[0].component.clone().unwrap();
        assert!(!component.props.contains_key("level"));

        assert!(matches!(
            document.set_component_prop("does-not-exist", "level", serde_json::json!(1)),
            Err(EditorDocumentError::MissingClip(_))
        ));
    }

    #[test]
    fn project_properties_are_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        assert_eq!(
            document.project_properties().get("title"),
            Some(&serde_json::json!("Mikan example"))
        );
        document.set_project_property("title", serde_json::json!("Chapter 1"));

        // Setting an identical value and removing an absent key record no
        // undo entry, so exactly two undos restore the original state.
        document.set_project_property("episode", serde_json::json!(3));
        document.set_project_property("episode", serde_json::json!(3));
        document.remove_project_property("does-not-exist");
        assert_eq!(
            document.project_properties().get("title"),
            Some(&serde_json::json!("Chapter 1"))
        );
        assert_eq!(
            document.project_properties().get("episode"),
            Some(&serde_json::json!(3))
        );

        assert!(document.undo().unwrap());
        assert!(!document.project_properties().contains_key("episode"));
        assert!(document.undo().unwrap());
        assert_eq!(
            document.project_properties().get("title"),
            Some(&serde_json::json!("Mikan example"))
        );
    }

    #[test]
    fn authors_and_flattens_clip_volume_keyframes() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        document
            .set_clip_volume_static("dialogue-001", 0.5)
            .unwrap();
        document
            .set_clip_volume_keyframe("dialogue-001", Time::new(1, 1), 1.0)
            .unwrap();
        let volume = document.tracks()[0].clips[0].volume.clone().unwrap();
        assert_eq!(evaluate_f64(&volume, Time::new(1, 2)).unwrap(), 0.75);
        assert_eq!(
            evaluate_f64(
                &document.audio_graph().unwrap().clips[0].volume,
                Time::new(1, 2)
            )
            .unwrap(),
            0.75
        );

        document
            .set_clip_volume_keyframe("dialogue-001", Time::new(1, 1), 0.8)
            .unwrap();
        let Animatable::Keyframes(animation) =
            document.tracks()[0].clips[0].volume.clone().unwrap()
        else {
            panic!("volume should be keyframed");
        };
        assert_eq!(animation.keyframes.len(), 2);
        assert_eq!(animation.keyframes[1].value, 0.8);

        document
            .flatten_clip_volume("dialogue-001", Time::new(1, 2))
            .unwrap();
        assert_eq!(
            document.tracks()[0].clips[0].volume,
            Some(Animatable::Static(0.65))
        );
        assert!(document.undo().unwrap());
        assert!(matches!(
            document.tracks()[0].clips[0].volume,
            Some(Animatable::Keyframes(_))
        ));
        assert!(
            document
                .remove_clip_volume_keyframe("dialogue-001", Time::new(1, 1))
                .unwrap()
        );
        assert_eq!(
            document.tracks()[0].clips[0].volume,
            Some(Animatable::Static(0.5))
        );
    }

    #[test]
    fn react_entry_path_is_stored_relative_and_resolves_back_to_absolute() {
        let root = std::env::temp_dir().join(format!(
            "mikan-editor-react-entry-{}-{}",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("react")).unwrap();
        let entry_path = root.join("react/entry.tsx");
        fs::write(
            &entry_path,
            b"export default function Root() { return null; }",
        )
        .unwrap();
        let project_path = root.join("project.mikan.json");
        fs::write(&project_path, MINIMAL).unwrap();
        let mut document = EditorDocument::load(&project_path).unwrap();

        document.set_react_entry(&entry_path).unwrap();
        assert_eq!(document.react_entry(), Some("./react/entry.tsx"));
        assert_eq!(
            document.react_entry_absolute_path().unwrap(),
            fs::canonicalize(&entry_path).unwrap()
        );

        document.clear_react_entry();
        assert!(document.react_entry().is_none());
        assert!(document.undo().unwrap());
        assert_eq!(document.react_entry(), Some("./react/entry.tsx"));
    }

    #[test]
    fn react_preview_builds_a_synthetic_document_from_composition_facts() {
        let root = std::env::temp_dir().join(format!(
            "mikan-editor-react-preview-{}-{}",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let entry_path = root.join("title.tsx");
        fs::write(
            &entry_path,
            b"export default function Root() { return null; }",
        )
        .unwrap();

        let document = EditorDocument::react_preview(
            &entry_path,
            1920,
            1080,
            Rational::new(30, 1),
            48_000,
            150,
        )
        .unwrap();

        assert_eq!(document.project().settings.width, 1920);
        assert_eq!(document.project().settings.height, 1080);
        assert!(document.project().tracks.is_empty());
        assert_eq!(document.react_entry(), Some("title.tsx"));
        assert_eq!(
            document.react_entry_absolute_path().unwrap(),
            fs::canonicalize(&entry_path).unwrap()
        );
        // 150 frames at 30fps is exactly five seconds.
        assert_eq!(document.duration().as_seconds().unwrap(), 5.0);
        assert!(!document.is_dirty());
    }

    #[test]
    fn imports_inserts_and_deletes_assets_through_history() {
        let root = std::env::temp_dir().join(format!(
            "mikan-editor-import-{}-{}",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("media")).unwrap();
        let audio_path = root.join("media/Voice Take.wav");
        fs::write(&audio_path, b"fixture").unwrap();
        let project_path = root.join("project.mikan.json");
        fs::write(
            &project_path,
            include_str!("../../../examples/editor-demo.mikan.json"),
        )
        .unwrap();
        let mut document = EditorDocument::load(&project_path).unwrap();

        let imported = document.import_assets([audio_path]).unwrap();
        assert_eq!(imported[0].id, "voice-take");
        assert_eq!(imported[0].kind, AssetKind::Audio);
        let Asset::Audio {
            source: AssetSource::File { path },
            ..
        } = &document.project().assets["voice-take"]
        else {
            panic!("imported WAV must be an audio asset");
        };
        assert_eq!(path, "./media/Voice Take.wav");

        let clip_id = document.insert_asset_clip("voice-take", 60, 120).unwrap();
        let audio_track = document
            .tracks()
            .into_iter()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap();
        assert_eq!(audio_track.clips[0].id, clip_id);
        assert_eq!(audio_track.clips[0].start, Time::new(60, 60));
        assert_eq!(audio_track.clips[0].duration, Time::new(120, 60));

        document.delete_clip(&clip_id).unwrap();
        assert!(
            document
                .tracks()
                .into_iter()
                .find(|track| track.kind == TrackKind::Audio)
                .unwrap()
                .clips
                .is_empty()
        );
        assert!(document.undo().unwrap());
        assert_eq!(
            document
                .tracks()
                .into_iter()
                .find(|track| track.kind == TrackKind::Audio)
                .unwrap()
                .clips
                .len(),
            1
        );
        assert!(document.undo().unwrap());
        assert!(
            document
                .tracks()
                .into_iter()
                .all(|track| track.kind != TrackKind::Audio)
        );
        assert!(document.undo().unwrap());
        assert!(document.assets().is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creates_and_edits_dialogue_from_an_audio_asset() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        let mut aoi = document.project.characters["akane"].clone();
        aoi.name = "琴葉葵".to_owned();
        document.project.characters.insert("aoi".to_owned(), aoi);

        let clip_id = document
            .insert_dialogue_clip("voice-001", "akane", None, 0, 60)
            .unwrap();
        document
            .set_dialogue_text(&clip_id, "ゲームを始めるで")
            .unwrap();
        document.set_dialogue_character(&clip_id, "aoi").unwrap();

        let dialogue = document
            .tracks()
            .into_iter()
            .flat_map(|track| track.clips)
            .find(|clip| clip.id == clip_id)
            .and_then(|clip| clip.dialogue)
            .unwrap();
        assert_eq!(
            dialogue,
            DialogueClipSummary {
                character: "aoi".to_owned(),
                text: "ゲームを始めるで".to_owned(),
                audio: Some("voice-001".to_owned()),
                expression: None,
                lip_sync_cue_count: 0,
            }
        );
    }

    #[test]
    fn creates_a_ready_to_use_character_from_an_image_asset() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        let character = document
            .create_character_from_image("akane-default")
            .unwrap();
        let created = &document.project.characters[&character.id];
        assert_eq!(
            (
                character.id.as_str(),
                created.name.as_str(),
                created
                    .portrait
                    .as_ref()
                    .and_then(|portrait| portrait.expressions.get("default"))
                    .map(String::as_str),
                created.subtitle.is_some(),
            ),
            ("akane-2", "Akane", Some("akane-default"), true)
        );
        document
            .insert_dialogue_clip("voice-001", &character.id, None, 0, 60)
            .unwrap();
        assert_eq!(document.scene_at(Time::ZERO).unwrap().layers.len(), 1);
    }

    #[test]
    fn character_expression_creation_is_named_and_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        document.project.assets.insert(
            "akane-happy".to_owned(),
            Asset::Image {
                name: Some("Akane Happy.png".to_owned()),
                source: AssetSource::File {
                    path: "assets/akane-happy.png".to_owned(),
                },
            },
        );

        let expression = document
            .add_character_expression("akane", "akane-happy")
            .unwrap();
        assert_eq!(expression, "akane-happy");
        assert_eq!(
            document.project.characters["akane"]
                .portrait
                .as_ref()
                .unwrap()
                .expressions["akane-happy"],
            "akane-happy"
        );

        assert!(document.undo().unwrap());
        assert_eq!(document.characters()[0].expressions, vec!["default"]);
    }

    #[test]
    fn character_creation_is_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        let character = document
            .create_character_from_image("akane-default")
            .unwrap();
        document.undo().unwrap();

        assert!(!document.project.characters.contains_key(&character.id));
    }

    #[test]
    fn character_rename_is_trimmed_and_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        document.rename_character("akane", "  茜ちゃん  ").unwrap();
        let renamed = document.project.characters["akane"].name.clone();
        document.undo().unwrap();

        assert_eq!(
            (
                renamed.as_str(),
                document.project.characters["akane"].name.as_str()
            ),
            ("茜ちゃん", "琴葉茜")
        );
    }

    #[test]
    fn character_rename_rejects_an_empty_name() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        let error = document.rename_character("akane", "  ").unwrap_err();

        assert!(matches!(error, EditorDocumentError::InvalidCharacterName));
    }

    #[test]
    fn character_deletion_requires_confirmation_and_is_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        let references = document.character_references("akane").unwrap();

        assert_eq!(references, vec!["track `dialogue` / clip `dialogue-001`"]);
        assert!(matches!(
            document.delete_character("akane", false),
            Err(EditorDocumentError::CharacterInUse { .. })
        ));

        document.delete_character("akane", true).unwrap();
        assert!(!document.project.characters.contains_key("akane"));
        assert!(document.tracks()[0].clips.is_empty());
        document.project.validate().unwrap();

        assert!(document.undo().unwrap());
        assert!(document.project.characters.contains_key("akane"));
        assert_eq!(document.tracks()[0].clips.len(), 1);
    }

    #[test]
    fn character_deletion_preserves_dialogue_on_locked_tracks() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        document.project.tracks[0].locked = Some(true);

        assert!(matches!(
            document.delete_character("akane", true),
            Err(EditorDocumentError::LockedTrack(_))
        ));
        assert!(document.project.characters.contains_key("akane"));
        assert_eq!(document.tracks()[0].clips.len(), 1);
        assert!(!document.is_dirty());
    }

    #[test]
    fn rejects_non_audio_assets_for_dialogue() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        let error = document
            .insert_dialogue_clip("akane-default", "akane", None, 0, 60)
            .unwrap_err();

        assert!(matches!(
            error,
            EditorDocumentError::AssetKindMismatch { .. }
        ));
    }

    #[test]
    fn keeping_the_same_dialogue_character_preserves_its_expression() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        document
            .set_dialogue_character("dialogue-001", "akane")
            .unwrap();

        assert_eq!(
            document.tracks()[0].clips[0]
                .dialogue
                .as_ref()
                .and_then(|dialogue| dialogue.expression.as_deref()),
            Some("default")
        );
    }

    #[test]
    fn dialogue_expression_selection_validates_and_is_undoable() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        document.project.assets.insert(
            "akane-happy".to_owned(),
            Asset::Image {
                name: Some("Akane Happy.png".to_owned()),
                source: AssetSource::File {
                    path: "assets/akane-happy.png".to_owned(),
                },
            },
        );
        document
            .add_character_expression("akane", "akane-happy")
            .unwrap();

        assert!(matches!(
            document.set_dialogue_expression("dialogue-001", Some("missing")),
            Err(EditorDocumentError::MissingCharacterExpression { .. })
        ));
        document
            .set_dialogue_expression("dialogue-001", Some("akane-happy"))
            .unwrap();
        assert_eq!(
            document.tracks()[0].clips[0]
                .dialogue
                .as_ref()
                .and_then(|dialogue| dialogue.expression.as_deref()),
            Some("akane-happy")
        );

        assert!(document.undo().unwrap());
        assert_eq!(
            document.tracks()[0].clips[0]
                .dialogue
                .as_ref()
                .and_then(|dialogue| dialogue.expression.as_deref()),
            Some("default")
        );
    }

    #[test]
    fn inserts_assets_only_on_explicit_compatible_unlocked_tracks() {
        let mut document = EditorDocument::from_json(EDITOR_DEMO, ".").unwrap();
        document.project.assets.insert(
            "voice".to_owned(),
            Asset::Audio {
                name: Some("Voice".to_owned()),
                source: AssetSource::File {
                    path: "voice.wav".to_owned(),
                },
            },
        );

        assert!(matches!(
            document.insert_asset_clip_on_track("voice", Some("titles"), 0, 60),
            Err(EditorDocumentError::IncompatibleTrack { .. })
        ));
        assert!(document.project.tracks[0].items.len() == 1);

        document.insert_asset_clip("voice", 0, 60).unwrap();
        let audio_track_id = document
            .project
            .tracks
            .iter()
            .find(|track| track.kind == TrackKind::Audio)
            .unwrap()
            .id
            .clone();
        let clip_id = document
            .insert_asset_clip_on_track("voice", Some(&audio_track_id), 60, 60)
            .unwrap();
        let audio_track = document
            .project
            .tracks
            .iter()
            .find(|track| track.id == audio_track_id)
            .unwrap();
        assert_eq!(audio_track.items.last().unwrap().id, clip_id);

        document
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == audio_track_id)
            .unwrap()
            .locked = Some(true);
        assert!(matches!(
            document.insert_asset_clip_on_track("voice", Some(&audio_track_id), 120, 60),
            Err(EditorDocumentError::LockedTrack(_))
        ));
    }

    #[test]
    fn rejects_an_import_batch_without_partial_changes() {
        let root = std::env::temp_dir().join(format!(
            "mikan-editor-import-atomic-{}-{}",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let image = root.join("image.png");
        let unsupported = root.join("notes.txt");
        fs::write(&image, b"fixture").unwrap();
        fs::write(&unsupported, b"fixture").unwrap();
        let mut document = EditorDocument::from_json(MINIMAL, &root).unwrap();

        assert!(document.import_assets([image, unsupported]).is_err());
        assert!(document.assets().is_empty());
        assert!(!document.is_dirty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn relinks_and_safely_removes_referenced_assets() {
        let root = std::env::temp_dir().join(format!(
            "mikan-editor-relink-{}-{}",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let first = root.join("first.wav");
        let replacement = root.join("replacement.wav");
        fs::write(&first, b"first").unwrap();
        fs::write(&replacement, b"replacement").unwrap();
        let mut document = EditorDocument::from_json(MINIMAL, &root).unwrap();
        let asset = document.import_assets([first]).unwrap().remove(0);
        let clip = document.insert_asset_clip(&asset.id, 0, 60).unwrap();

        document.relink_asset(&asset.id, &replacement).unwrap();
        let canonical_replacement = fs::canonicalize(&replacement).unwrap();
        assert_eq!(
            document.assets()[0].path.as_deref(),
            Some(canonical_replacement.as_path())
        );
        let references = document.asset_references(&asset.id).unwrap();
        assert_eq!(references, vec![format!("track `audio` / clip `{clip}`")]);
        assert!(matches!(
            document.remove_asset(&asset.id, false),
            Err(EditorDocumentError::AssetInUse { .. })
        ));

        document.remove_asset(&asset.id, true).unwrap();
        assert!(document.assets().is_empty());
        assert!(document.tracks()[0].clips.is_empty());
        assert!(document.undo().unwrap());
        assert_eq!(
            document.assets()[0].path.as_deref(),
            Some(canonical_replacement.as_path())
        );
        assert_eq!(document.tracks()[0].clips.len(), 1);
        assert!(document.undo().unwrap());
        let canonical_first = fs::canonicalize(root.join("first.wav")).unwrap();
        assert_eq!(
            document.assets()[0].path.as_deref(),
            Some(canonical_first.as_path())
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cascading_character_and_dialogue_asset_removal_keeps_project_valid() {
        let mut portrait_document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        portrait_document
            .remove_asset("akane-default", true)
            .unwrap();
        assert!(
            portrait_document.project().characters["akane"]
                .portrait
                .is_none()
        );
        assert!(portrait_document.scene_at(Time::new(6, 1)).is_ok());

        let mut voice_document = EditorDocument::from_json(VOICEROID, "examples").unwrap();
        voice_document.remove_asset("voice-001", true).unwrap();
        assert_eq!(voice_document.tracks()[0].clips.len(), 1);
        assert!(voice_document.audio_graph().unwrap().clips.is_empty());
        assert!(voice_document.scene_at(Time::new(6, 1)).is_ok());
    }

    #[test]
    fn saves_pretty_json_and_marks_the_saved_revision_clean() {
        let unique = format!(
            "mikan-editor-save-{}-{}.mikan.json",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        fs::write(
            &path,
            include_str!("../../../examples/editor-demo.mikan.json"),
        )
        .unwrap();
        let mut document = EditorDocument::load(&path).unwrap();
        document.edit_clip_frames("welcome", 60, 480).unwrap();

        document.save().unwrap();

        assert!(!document.is_dirty());
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.ends_with('\n'));
        assert_eq!(
            Project::from_json(&saved).unwrap(),
            document.project().clone()
        );
        document.undo().unwrap();
        assert!(document.is_dirty());
        document.redo().unwrap();
        assert!(!document.is_dirty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unsaved_documents_report_a_recoverable_save_error() {
        let mut document = EditorDocument::from_json(MINIMAL, ".").unwrap();

        assert!(matches!(
            document.save(),
            Err(EditorDocumentError::MissingSavePath)
        ));
    }

    #[test]
    fn save_as_assigns_a_path_only_after_a_successful_atomic_write() {
        let unique = format!(
            "mikan-editor-save-as-{}-{}.mikan.json",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        let mut document = EditorDocument::from_json(
            include_str!("../../../examples/editor-demo.mikan.json"),
            "examples",
        )
        .unwrap();
        document.edit_clip_frames("welcome", 60, 480).unwrap();

        document.save_as(&path).unwrap();

        assert_eq!(document.path(), Some(path.as_path()));
        assert_eq!(document.asset_root(), path.parent().unwrap());
        assert_eq!(
            document.display_name(),
            path.file_stem().unwrap().to_str().unwrap()
        );
        assert!(!document.is_dirty());
        assert_eq!(Project::load(&path).unwrap(), document.project().clone());
        fs::remove_file(path).unwrap();

        let invalid_path = std::env::temp_dir()
            .join("missing-mikan-save-as-directory")
            .join("project.mikan.json");
        let mut document = EditorDocument::from_json(MINIMAL, ".").unwrap();
        assert!(document.save_as(invalid_path).is_err());
        assert_eq!(document.path(), None);
    }
}
