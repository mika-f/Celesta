//! Viewer-owned project state and renderer-facing preview evaluation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use celesta_composition::{Animatable, AudioGraph, Rational, Scene, Time, TimeError};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_project::{
    Asset, AssetKind, AssetSource, LoadError, Project, ProjectSettings, ProjectVersion,
    TimelineContent, TrackKind,
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

/// A loaded project plus the viewer-only context that is never serialized.
/// The viewer does not edit projects: apart from track mute/solo, which only
/// change what the preview (and an export from it) plays, the project stays
/// exactly as loaded.
pub struct EditorDocument {
    project: Project,
    path: Option<PathBuf>,
    asset_root: PathBuf,
    duration: Time,
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

    /// The file name without its extension, treating `.celesta.json` as one
    /// extension: `demo.celesta.json` shows as `demo`.
    pub fn display_name(&self) -> String {
        let stem = self
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");
        stem.strip_suffix(".celesta").unwrap_or(stem).to_owned()
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
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        let muted = track.muted != Some(true);
        track.muted = muted.then_some(true);
        Ok(muted)
    }

    pub fn toggle_track_solo(&mut self, track_id: &str) -> Result<bool, EditorDocumentError> {
        let track = self
            .project
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| EditorDocumentError::MissingTrack(track_id.to_owned()))?;
        let solo = track.solo != Some(true);
        track.solo = solo.then_some(true);
        Ok(solo)
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

    /// The project's GUI-editable `properties` map. React entries read these
    /// values through `useProjectProperty(key, defaultValue)`; which fields
    /// exist and how the Inspector presents them comes from the entry's
    /// `defineProjectProperties()` schema, which is advisory Inspector
    /// metadata — this map itself stays schema-free.
    pub fn project_properties(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.project.properties
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
        })
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

#[derive(Debug)]
pub enum EditorDocumentError {
    Load(LoadError),
    Duration(celesta_composition::TimeError),
    MissingTrack(String),
}

impl fmt::Display for EditorDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(formatter, "could not load project: {error}"),
            Self::Duration(error) => write!(formatter, "could not calculate duration: {error}"),
            Self::MissingTrack(track_id) => write!(formatter, "track `{track_id}` does not exist"),
        }
    }
}

impl Error for EditorDocumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::MissingTrack(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = include_str!("../../../examples/minimal.celesta.json");
    const VOICEROID: &str = include_str!("../../../examples/voiceroid.celesta.json");

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
    fn loads_the_editor_demo_timeline() {
        let document = EditorDocument::from_json(
            include_str!("../../../examples/editor-demo.celesta.json"),
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
        let Ok(mut renderer) = celesta_gpu_renderer::GpuRenderer::new(
            celesta_gpu_renderer::GpuRenderOptions::default(),
        ) else {
            return;
        };
        renderer = renderer.with_asset_root(document.asset_root());

        let scene = document.scene_at(Time::new(11, 2)).unwrap();
        let frame = renderer.render(&scene).unwrap();

        assert_eq!(frame.width(), 1920);
        assert_eq!(frame.height(), 1080);
    }

    #[test]
    fn react_preview_builds_a_synthetic_document_from_composition_facts() {
        let root = std::env::temp_dir().join(format!(
            "celesta-editor-react-preview-{}",
            std::process::id()
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
    }

    #[test]
    fn track_mute_and_solo_toggle_the_audio_graph() {
        let mut document = EditorDocument::from_json(VOICEROID, "examples").unwrap();

        assert!(document.toggle_track_muted("dialogue").unwrap());
        assert!(document.tracks()[0].muted);
        assert!(document.audio_graph().unwrap().clips.is_empty());
        assert!(!document.toggle_track_muted("dialogue").unwrap());
        assert!(!document.audio_graph().unwrap().clips.is_empty());

        assert!(document.toggle_track_solo("dialogue").unwrap());
        assert!(document.tracks()[0].solo);
        assert!(document.toggle_track_muted("missing").is_err());
    }

    #[test]
    fn react_entry_resolves_relative_to_the_project_file() {
        let root =
            std::env::temp_dir().join(format!("celesta-editor-react-entry-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let project_path = root.join("project.celesta.json");
        let mut project: serde_json::Value = serde_json::from_str(MINIMAL).unwrap();
        project["settings"]["reactEntry"] = "./react/entry.tsx".into();
        fs::write(&project_path, project.to_string()).unwrap();

        let document = EditorDocument::load(&project_path).unwrap();

        assert_eq!(document.display_name(), "project");
        assert_eq!(document.react_entry(), Some("./react/entry.tsx"));
        assert_eq!(
            document.react_entry_absolute_path().unwrap(),
            fs::canonicalize(&root).unwrap().join("./react/entry.tsx")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
