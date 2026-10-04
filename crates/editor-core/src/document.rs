use crate::error::EditorDocumentError;
use crate::summary::{
    AssetSummary, CharacterSummary, ClipKind, ClipSummary, ComponentClipSummary,
    DialogueClipSummary, LipSyncSummary, TrackSummary,
};
use celesta_composition::{Animatable, AudioGraph, Rational, Scene, Time};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_project::{
    Asset, AssetSource, Project, ProjectSettings, ProjectVersion, TimelineContent,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A loaded project plus the viewer-only context that is never serialized.
/// The viewer does not edit projects: apart from track mute/solo, which only
/// change what the preview (and an export from it) plays, the project stays
/// exactly as loaded.
pub struct EditorDocument {
    pub(crate) project: Project,
    pub(crate) path: Option<PathBuf>,
    pub(crate) asset_root: PathBuf,
    pub(crate) duration: Time,
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

    pub(crate) fn new(
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

pub(crate) fn local_asset_path(asset: &Asset, asset_root: &Path) -> Option<PathBuf> {
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
