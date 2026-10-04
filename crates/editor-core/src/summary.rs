use celesta_composition::{Animatable, Time};
use celesta_project::{AssetKind, TrackKind};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
