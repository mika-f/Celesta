//! Spawns the `@celesta/react` Node.js runtime and evaluates a React
//! composition into the shared `celesta_composition::Scene` model.
//!
//! A single Node process is kept alive for the lifetime of a [`ReactBridge`]
//! and answers one JSON request per requested frame over its stdin/stdout
//! pipe, following the same "one long-lived process instead of one process
//! per frame" shape as `celesta-media`'s sequential video decoding session.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use celesta_composition::{
    Animatable, AssetLocation, AudioClip, AudioGraph, Layer, Rational, ResolvedAsset, Scene, Time,
    TimeRange,
};
use celesta_media::{AudioStream, FfmpegBackend, MediaProbe, VideoStream};
use serde::{Deserialize, Serialize};

mod runtime;
pub use runtime::runtime_paths;

/// A companion project's layers for one exact frame, evaluated up front by
/// the caller (see [`ReactBridge::scene_at_with_project`]).
#[derive(Clone, Copy, Debug)]
pub struct ProjectFrame<'a> {
    /// Every evaluated layer together, in project order — what
    /// `<ProjectTimeline />` embeds.
    pub layers: &'a [Layer],
    /// The same layers, grouped by track id — what `<ProjectTrack />` and
    /// `useProjectTrack()` embed.
    pub tracks: &'a BTreeMap<String, Vec<Layer>>,
}

/// Static composition facts read once from the entry's `<Composition>` root.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactCompositionMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub duration_in_frames: u64,
    /// Every `ComponentPropertySchema` declared via `registerComponent(name,
    /// component, schema)`, keyed by `name`. Populated at spawn time — every
    /// `registerComponent()` call runs at module scope before the entry's
    /// `Ready` handshake is sent — so this never changes for the lifetime of
    /// a `ReactBridge`. A registered component with no `schema` argument has
    /// no entry here.
    pub component_schemas: BTreeMap<String, ComponentPropertySchema>,
    /// The project property schema declared via `defineProjectProperties()`,
    /// or `None` when the entry declared none (distinct from an empty
    /// schema). Fields share `ComponentPropertyField`'s shape with component
    /// schemas; only where their values live differs (project-level vs per
    /// timeline item).
    pub project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
}

/// One audible `<Audio>` element as reported for a single rendered frame
/// (`packages/react/src/render.ts`'s `AudioClipDescriptor`, gathered during
/// the same tree walk that produces that frame's layers). Because collection
/// happens per frame, an `<Audio>` behind an ordinary React conditional or
/// nested inside `<Sequence>`s contributes on exactly the frames where it
/// actually renders; the caller merges these reports into the export's
/// `AudioGraph`.
///
/// `start`/`duration` are composition-space seconds bounded by any enclosing
/// sequences; `source_start` is where in the source file the first audible
/// moment plays, already adjusted so head-clipping by outer sequences keeps
/// the source clock continuous.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactAudioClipDescriptor {
    pub src: String,
    pub source_start: f64,
    pub playback_rate: Animatable<f64>,
    pub volume: Animatable<f64>,
    pub muted: bool,
    pub start: f64,
    pub duration: f64,
}

/// One field of a `ComponentPropertySchema` declared on the TypeScript side
/// (`packages/react/src/registry.ts`). Mirrors `ComponentPropertyField`
/// there field-for-field; kept as a real enum here (rather than opaque JSON)
/// so a GUI Inspector can match on `field_type` to choose a widget without
/// re-parsing JSON itself.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComponentPropertyField {
    String {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
    },
    Number {
        #[serde(default)]
        label: Option<String>,
        default_value: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        step: Option<f64>,
    },
    Boolean {
        #[serde(default)]
        label: Option<String>,
        default_value: bool,
    },
    Color {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
    },
    Select {
        #[serde(default)]
        label: Option<String>,
        default_value: String,
        options: Vec<String>,
    },
}

/// One registered component's declared props, keyed by prop name — the
/// Rust-side counterpart of `ComponentPropertySchema<Props>` in
/// `packages/react/src/registry.ts`.
pub type ComponentPropertySchema = BTreeMap<String, ComponentPropertyField>;

/// One component-resolution request: a `registerComponent()` name plus the
/// timeline item's configured props (see [`ReactBridge::resolve_components`]).
#[derive(Clone, Copy, Debug)]
pub struct ComponentResolutionRequest<'a> {
    pub component: &'a str,
    pub props: &'a BTreeMap<String, serde_json::Value>,
}

/// One frame's evaluation: the visual scene plus every `<Audio>` declaration
/// audible at that frame (see [`ReactAudioClipDescriptor`]).
#[derive(Clone, Debug)]
pub struct FrameEvaluation {
    pub scene: Scene,
    pub audio: Vec<ReactAudioClipDescriptor>,
}

/// A live connection to the `@celesta/react` CLI evaluating one entry module.
pub struct ReactBridge {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    metadata: ReactCompositionMetadata,
}

impl Drop for ReactBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ReactBridge {
    /// Spawns `node <cli_script> <entry>` and reads its startup configuration.
    pub fn spawn(
        node: impl AsRef<Path>,
        cli_script: impl AsRef<Path>,
        entry: impl AsRef<Path>,
    ) -> Result<Self, ReactBridgeError> {
        let node = node.as_ref();
        let mut command = Command::new(node);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .arg(cli_script.as_ref())
            .arg(entry.as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| ReactBridgeError::Executable {
                executable: node.to_owned(),
                source,
            })?;
        let mut stdin = child.stdin.take().ok_or(ReactBridgeError::MissingPipe)?;
        let stdout = child.stdout.take().ok_or(ReactBridgeError::MissingPipe)?;
        let mut stdout = BufReader::new(stdout);

        let mut media = FfmpegBackend::new();
        let metadata = loop {
            let mut line = String::new();
            let read = stdout.read_line(&mut line).map_err(ReactBridgeError::Io)?;
            if read == 0 {
                let _ = child.wait();
                return Err(ReactBridgeError::UnexpectedExit);
            }
            let message: ReadyMessage =
                serde_json::from_str(line.trim()).map_err(ReactBridgeError::Protocol)?;
            match message {
                ReadyMessage::Ready {
                    config,
                    component_schemas,
                    project_property_schema,
                } => {
                    break ReactCompositionMetadata {
                        width: config.width,
                        height: config.height,
                        frame_rate: config.frame_rate,
                        duration_in_frames: config.duration_in_frames,
                        component_schemas,
                        project_property_schema,
                    };
                }
                ReadyMessage::ProbeMedia { probe_media } => {
                    let response = match media.probe(&probe_media.path) {
                        Ok(probe) => MediaProbeResponse {
                            media: Some(media_probe_payload(probe)),
                            error: None,
                        },
                        Err(error) => MediaProbeResponse {
                            media: None,
                            error: Some(format!(
                                "could not probe {}: {error}",
                                probe_media.path.display()
                            )),
                        },
                    };
                    let payload =
                        serde_json::to_string(&response).map_err(ReactBridgeError::Protocol)?;
                    writeln!(stdin, "{payload}").map_err(ReactBridgeError::Io)?;
                    stdin.flush().map_err(ReactBridgeError::Io)?;
                }
                ReadyMessage::Error { error } => {
                    return Err(ReactBridgeError::EntryFailed(error));
                }
            }
        };

        Ok(Self {
            child,
            stdin,
            stdout,
            metadata,
        })
    }

    pub const fn metadata(&self) -> &ReactCompositionMetadata {
        &self.metadata
    }

    /// Requests the evaluated `Scene` at an exact composition time.
    pub fn scene_at(&mut self, time: Time) -> Result<Scene, ReactBridgeError> {
        Ok(self.evaluate_at(time, None)?.scene)
    }

    /// Same as [`Self::scene_at`], but also hands the entry's
    /// `<ProjectTimeline />`/`<ProjectTrack />`/`useProjectTrack()` a
    /// project's layers already evaluated for this exact time. The Node
    /// side cannot ask Rust to evaluate a project mid-render: this process
    /// is synchronously blocked on the response to this very request, so a
    /// request travelling the other way would deadlock. Evaluating up front
    /// and embedding the result avoids that.
    pub fn scene_at_with_project(
        &mut self,
        time: Time,
        project: Option<ProjectFrame<'_>>,
    ) -> Result<Scene, ReactBridgeError> {
        Ok(self.evaluate_at(time, project)?.scene)
    }

    /// The full evaluation behind [`Self::scene_at_with_project`]: the
    /// scene plus every `<Audio>` clip this frame's tree declares (see
    /// [`FrameEvaluation::audio`]).
    pub fn evaluate_at(
        &mut self,
        time: Time,
        project: Option<ProjectFrame<'_>>,
    ) -> Result<FrameEvaluation, ReactBridgeError> {
        let request = Request {
            time: Some(time),
            project: project.map(|frame| ProjectPayload {
                layers: frame.layers,
                tracks: frame.tracks,
            }),
            runtime: None,
            components: None,
        };
        match self.request_response(request)? {
            Response::Ok { scene, audio } => Ok(FrameEvaluation { scene, audio }),
            Response::Components { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
        }
    }

    /// Resolves individual `registerComponent()` names against the entry's
    /// registry without rendering the whole composition — one rendered layer
    /// list per request, in order, with `None` for names nothing registered.
    /// Used by the GPUI editor preview to place resolved component content at
    /// `TimelineContent::Component` items it has already evaluated. The
    /// components' hooks see this composition's real static facts (from the
    /// same `<Composition>` the handshake metadata was read from) and
    /// `time` — normally the preview playhead, so `useCurrentFrame()`
    /// matches what an export would render at that frame. Hook state still
    /// persists across calls on the resolution-only root.
    pub fn resolve_components(
        &mut self,
        requests: &[ComponentResolutionRequest<'_>],
        time: Time,
    ) -> Result<Vec<Option<Vec<Layer>>>, ReactBridgeError> {
        let request = Request {
            time: None,
            project: None,
            runtime: Some(ResolutionRuntime {
                width: self.metadata.width,
                height: self.metadata.height,
                fps: f64::from(self.metadata.frame_rate.numerator)
                    / f64::from(self.metadata.frame_rate.denominator),
                duration_in_frames: self.metadata.duration_in_frames,
                time,
                preview: true,
            }),
            components: Some(
                requests
                    .iter()
                    .map(|request| ComponentRequest {
                        component: request.component,
                        props: request.props,
                    })
                    .collect(),
            ),
        };
        match self.request_response(request)? {
            Response::Components { components } => Ok(components),
            Response::Ok { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
        }
    }

    /// Sweeps every frame of the composition and builds the complete
    /// `AudioGraph` its `<Audio>` declarations imply — the standalone
    /// counterpart of what `celesta-exporter` accumulates during its render
    /// loop. Each frame's `evaluate_at` report lists the `<Audio>` elements
    /// audible *that* frame (so conditional / sequence-shifted audio is
    /// captured); [`merge_react_audio_clips`] then collapses the per-frame
    /// duplicates. Relative `src` paths resolve against `entry_dir` (the
    /// entry file's own directory, like the renderer's asset root).
    pub fn collect_audio_graph(
        &mut self,
        sample_rate: u32,
        master_volume: f64,
        entry_dir: &Path,
    ) -> Result<AudioGraph, ReactBridgeError> {
        let frame_rate = self.metadata.frame_rate;
        let mut reports = Vec::new();
        for frame in 0..self.metadata.duration_in_frames {
            let time = Time::frames(frame as i64, frame_rate).map_err(ReactBridgeError::Time)?;
            reports.extend(self.evaluate_at(time, None)?.audio);
        }
        Ok(AudioGraph {
            sample_rate,
            master_volume,
            clips: react_audio_clips(&reports, entry_dir),
        })
    }

    fn request_response<R: Serialize>(&mut self, request: R) -> Result<Response, ReactBridgeError> {
        let payload = serde_json::to_string(&request).map_err(ReactBridgeError::Protocol)?;
        writeln!(self.stdin, "{payload}").map_err(ReactBridgeError::Io)?;
        self.stdin.flush().map_err(ReactBridgeError::Io)?;

        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .map_err(ReactBridgeError::Io)?;
        if read == 0 {
            return Err(ReactBridgeError::UnexpectedExit);
        }
        serde_json::from_str(line.trim()).map_err(ReactBridgeError::Protocol)
    }
}

/// Timescale seconds-valued `<Audio>` fields are converted at, matching
/// `packages/react/src/render.ts`'s `SECONDS_TIMESCALE`.
const REACT_AUDIO_SECONDS_TIMESCALE: u32 = 1_000_000;

fn react_seconds_to_time(seconds: f64) -> Time {
    Time::new(
        (seconds * f64::from(REACT_AUDIO_SECONDS_TIMESCALE)).round() as i64,
        REACT_AUDIO_SECONDS_TIMESCALE,
    )
}

/// Collapses per-frame `<Audio>` reports into one entry per distinct clip,
/// keeping first-seen order and how many frames reported it (its
/// multiplicity — two identical clips playing at once must stay two clips).
pub fn merge_react_audio_clips(
    clips: &[ReactAudioClipDescriptor],
) -> Vec<(ReactAudioClipDescriptor, usize)> {
    let mut merged: Vec<(ReactAudioClipDescriptor, usize)> = Vec::new();
    for clip in clips {
        match merged.iter_mut().find(|(known, _)| known == clip) {
            Some((_, count)) => *count += 1,
            None => merged.push((clip.clone(), 1)),
        }
    }
    merged
}

/// Turns merged `<Audio>` reports into `AudioClip`s, resolving relative `src`
/// paths against `entry_dir`. Generated ids are `react-audio:{n}` in
/// first-seen order.
pub fn react_audio_clips(clips: &[ReactAudioClipDescriptor], entry_dir: &Path) -> Vec<AudioClip> {
    merge_react_audio_clips(clips)
        .into_iter()
        .enumerate()
        .map(|(index, (clip, _))| {
            let path = if Path::new(&clip.src).is_relative() {
                entry_dir.join(&clip.src).to_string_lossy().into_owned()
            } else {
                clip.src.clone()
            };
            AudioClip {
                id: format!("react-audio:{index}"),
                asset: ResolvedAsset {
                    id: clip.src.clone(),
                    location: AssetLocation::File { path },
                },
                range: TimeRange {
                    start: react_seconds_to_time(clip.start),
                    duration: react_seconds_to_time(clip.duration),
                },
                source_start: react_seconds_to_time(clip.source_start),
                source_duration: None,
                playback_rate: clip.playback_rate,
                volume: clip.volume,
                muted: clip.muted,
            }
        })
        .collect()
}

#[derive(Serialize)]
struct Request<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<ProjectPayload<'a>>,
    /// Composition facts plus the requested time a component-resolution
    /// request's hooks should see (absent for frame requests, which carry
    /// `time` instead).
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<ResolutionRuntime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    components: Option<Vec<ComponentRequest<'a>>>,
}

/// The runtime context sent alongside component-resolution requests; the
/// TypeScript side turns it into the `CompositionRuntimeContext` those
/// components' `useCurrentFrame()`/`useVideoConfig()` read.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolutionRuntime {
    width: u32,
    height: u32,
    fps: f64,
    duration_in_frames: u64,
    time: Time,
    preview: bool,
}

#[derive(Serialize)]
struct ProjectPayload<'a> {
    layers: &'a [Layer],
    tracks: &'a BTreeMap<String, Vec<Layer>>,
}

#[derive(Serialize)]
struct ComponentRequest<'a> {
    component: &'a str,
    props: &'a BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Response {
    Ok {
        scene: Scene,
        #[serde(default)]
        audio: Vec<ReactAudioClipDescriptor>,
    },
    Components {
        components: Vec<Option<Vec<Layer>>>,
    },
    Err {
        error: String,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReadyMessage {
    Ready {
        config: ReactCompositionConfig,
        #[serde(default, rename = "componentSchemas")]
        component_schemas: BTreeMap<String, ComponentPropertySchema>,
        /// `null` (or absent) when the entry never called
        /// `defineProjectProperties()`; an empty object means it declared an
        /// intentionally empty schema.
        #[serde(default, rename = "propertySchema")]
        project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
    },
    ProbeMedia {
        #[serde(rename = "probeMedia")]
        probe_media: MediaProbeRequest,
    },
    Error {
        error: String,
    },
}

#[derive(Deserialize)]
struct MediaProbeRequest {
    path: PathBuf,
}

#[derive(Serialize)]
struct MediaProbeResponse<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    media: Option<MediaProbePayload<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaProbePayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<VideoProbePayload<'a>>,
    audio: Vec<AudioProbePayload<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoProbePayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    codec: Option<&'a str>,
    width: u32,
    height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_rate: Option<Rational>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_seconds: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioProbePayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    codec: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_seconds: Option<f64>,
}

fn media_probe_payload(probe: &MediaProbe) -> MediaProbePayload<'_> {
    MediaProbePayload {
        duration_seconds: seconds(probe.duration),
        video: probe.video.as_ref().map(video_probe_payload),
        audio: probe.audio.iter().map(audio_probe_payload).collect(),
    }
}

fn video_probe_payload(video: &VideoStream) -> VideoProbePayload<'_> {
    VideoProbePayload {
        codec: video.codec.as_deref(),
        width: video.width,
        height: video.height,
        frame_rate: video.frame_rate,
        duration_seconds: seconds(video.duration),
    }
}

fn audio_probe_payload(audio: &AudioStream) -> AudioProbePayload<'_> {
    AudioProbePayload {
        codec: audio.codec.as_deref(),
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        duration_seconds: seconds(audio.duration),
    }
}

fn seconds(time: Option<Time>) -> Option<f64> {
    time.and_then(|time| time.as_seconds().ok())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReactCompositionConfig {
    width: u32,
    height: u32,
    frame_rate: Rational,
    duration_in_frames: u64,
}

#[derive(Debug)]
pub enum ReactBridgeError {
    Executable {
        executable: PathBuf,
        source: io::Error,
    },
    MissingPipe,
    Io(io::Error),
    Protocol(serde_json::Error),
    UnexpectedExit,
    UnexpectedResponse,
    EntryFailed(String),
    Render(String),
    Time(celesta_composition::TimeError),
}

impl fmt::Display for ReactBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Executable { executable, source } => write!(
                formatter,
                "could not run {}: {source}",
                executable.display()
            ),
            Self::MissingPipe => {
                formatter.write_str("the Node.js React runtime did not provide its stdio pipes")
            }
            Self::Io(error) => write!(
                formatter,
                "could not communicate with the Node.js React runtime: {error}"
            ),
            Self::Protocol(error) => write!(
                formatter,
                "the Node.js React runtime sent an unexpected response: {error}"
            ),
            Self::UnexpectedExit => {
                formatter.write_str("the Node.js React runtime exited unexpectedly")
            }
            Self::UnexpectedResponse => formatter
                .write_str("the Node.js React runtime answered with the wrong kind of response"),
            Self::EntryFailed(error) => {
                write!(formatter, "could not load the React composition: {error}")
            }
            Self::Render(error) => write!(formatter, "could not render the composition: {error}"),
            Self::Time(error) => write!(formatter, "invalid composition time: {error}"),
        }
    }
}

impl Error for ReactBridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Executable { source, .. } | Self::Io(source) => Some(source),
            Self::Protocol(error) => Some(error),
            Self::MissingPipe
            | Self::UnexpectedExit
            | Self::UnexpectedResponse
            | Self::EntryFailed(_)
            | Self::Render(_) => None,
            Self::Time(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::{Keyframe, KeyframeAnimation, KeyframeAnimationType};

    #[test]
    fn reports_a_missing_node_executable() {
        let result = ReactBridge::spawn(
            "/definitely-not-installed/celesta-node",
            "cli.js",
            "entry.tsx",
        );
        assert!(matches!(result, Err(ReactBridgeError::Executable { .. })));
    }

    #[test]
    fn deserializes_component_schemas_from_the_ready_message() {
        let json = serde_json::json!({
            "config": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "durationInFrames": 30
            },
            "componentSchemas": {
                "BossIntroduction": {
                    "bossName": {"type": "string", "label": "Boss Name", "defaultValue": "Golem"},
                    "level": {
                        "type": "number",
                        "defaultValue": 1,
                        "min": 1,
                        "max": 999
                    }
                }
            }
        })
        .to_string();

        let ReadyMessage::Ready {
            config,
            component_schemas,
            ..
        } = serde_json::from_str(&json).unwrap()
        else {
            panic!("expected a Ready message");
        };
        assert_eq!(config.width, 640);
        let schema = component_schemas.get("BossIntroduction").unwrap();
        assert_eq!(
            schema.get("bossName"),
            Some(&ComponentPropertyField::String {
                label: Some("Boss Name".to_owned()),
                default_value: "Golem".to_owned(),
            })
        );
        assert_eq!(
            schema.get("level"),
            Some(&ComponentPropertyField::Number {
                label: None,
                default_value: 1.0,
                min: Some(1.0),
                max: Some(999.0),
                step: None,
            })
        );
    }

    #[test]
    fn ready_message_without_component_schemas_defaults_to_empty() {
        let json = serde_json::json!({
            "config": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "durationInFrames": 30
            }
        })
        .to_string();

        let ReadyMessage::Ready {
            component_schemas, ..
        } = serde_json::from_str(&json).unwrap()
        else {
            panic!("expected a Ready message");
        };
        assert!(component_schemas.is_empty());
    }

    #[test]
    fn deserializes_the_project_property_schema_from_the_ready_message() {
        let json = serde_json::json!({
            "config": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "durationInFrames": 30
            },
            "componentSchemas": {},
            "propertySchema": {
                "title": {"type": "string", "label": "Title", "defaultValue": "Celesta"},
                "accent": {"type": "color", "defaultValue": "#ff8800"},
                "opacity": {"type": "number", "defaultValue": 1.0, "min": 0.0, "max": 1.0},
                "visible": {"type": "boolean", "defaultValue": true},
                "style": {"type": "select", "defaultValue": "bold", "options": ["bold", "light"]}
            }
        })
        .to_string();

        let ReadyMessage::Ready {
            project_property_schema,
            ..
        } = serde_json::from_str(&json).unwrap()
        else {
            panic!("expected a Ready message");
        };
        let schema = project_property_schema.expect("a declared property schema");
        assert_eq!(
            schema.get("title"),
            Some(&ComponentPropertyField::String {
                label: Some("Title".to_owned()),
                default_value: "Celesta".to_owned(),
            })
        );
        assert_eq!(
            schema.get("accent"),
            Some(&ComponentPropertyField::Color {
                label: None,
                default_value: "#ff8800".to_owned(),
            })
        );
        assert_eq!(
            schema.get("opacity"),
            Some(&ComponentPropertyField::Number {
                label: None,
                default_value: 1.0,
                min: Some(0.0),
                max: Some(1.0),
                step: None,
            })
        );
        assert_eq!(
            schema.get("visible"),
            Some(&ComponentPropertyField::Boolean {
                label: None,
                default_value: true,
            })
        );
        assert_eq!(
            schema.get("style"),
            Some(&ComponentPropertyField::Select {
                label: None,
                default_value: "bold".to_owned(),
                options: vec!["bold".to_owned(), "light".to_owned()],
            })
        );
    }

    #[test]
    fn ready_message_without_a_project_property_schema_is_none() {
        for json in [
            serde_json::json!({
                "config": {
                    "width": 640,
                    "height": 360,
                    "frameRate": {"numerator": 30, "denominator": 1},
                    "durationInFrames": 30
                }
            }),
            // `null` is what the CLI sends when the entry never called
            // defineProjectProperties(); an empty object would be a
            // declared-but-empty schema.
            serde_json::json!({
                "config": {
                    "width": 640,
                    "height": 360,
                    "frameRate": {"numerator": 30, "denominator": 1},
                    "durationInFrames": 30
                },
                "propertySchema": null
            }),
        ] {
            let ReadyMessage::Ready {
                project_property_schema,
                ..
            } = serde_json::from_str(&json.to_string()).unwrap()
            else {
                panic!("expected a Ready message");
            };
            assert!(project_property_schema.is_none());
        }
    }

    #[test]
    fn deserializes_audio_clips_from_a_frame_response() {
        let json = serde_json::json!({
            "scene": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "time": {"value": 0, "timescale": 1},
                "layers": []
            },
            "audio": [
                {
                    "src": "./voice.wav",
                    "sourceStart": 1.5,
                    "playbackRate": {"type": "keyframes", "keyframes": [
                        {"time": {"value": 0, "timescale": 1}, "value": 1.0}
                    ]},
                    "volume": 0.5,
                    "muted": false,
                    "start": 2.0,
                    "duration": 3.0
                }
            ]
        })
        .to_string();

        let Response::Ok { audio, .. } = serde_json::from_str(&json).unwrap() else {
            panic!("expected an Ok response");
        };
        assert_eq!(
            audio,
            vec![ReactAudioClipDescriptor {
                src: "./voice.wav".to_owned(),
                source_start: 1.5,
                playback_rate: Animatable::Keyframes(KeyframeAnimation {
                    kind: KeyframeAnimationType::Keyframes,
                    keyframes: vec![Keyframe {
                        time: Time::ZERO,
                        value: 1.0,
                        easing: None,
                    }],
                }),
                volume: Animatable::Static(0.5),
                muted: false,
                start: 2.0,
                duration: 3.0,
            }]
        );
    }

    #[test]
    fn frame_response_without_audio_defaults_to_empty() {
        let json = serde_json::json!({
            "scene": {
                "width": 640,
                "height": 360,
                "frameRate": {"numerator": 30, "denominator": 1},
                "time": {"value": 0, "timescale": 1},
                "layers": []
            }
        })
        .to_string();

        let Response::Ok { audio, .. } = serde_json::from_str(&json).unwrap() else {
            panic!("expected an Ok response");
        };
        assert!(audio.is_empty());
    }

    #[test]
    fn deserializes_component_resolutions_from_a_response() {
        let json = serde_json::json!({
            "components": [
                null,
                [
                    {
                        "id": "resolved",
                        "transform": {
                            "position": {"x": 0.0, "y": 0.0},
                            "scale": {"x": 1.0, "y": 1.0},
                            "rotation": 0.0,
                            "anchor": {"x": 0.5, "y": 0.5}
                        },
                        "opacity": 1.0,
                        "content": {"type": "text", "text": "hello", "style": {}}
                    }
                ]
            ]
        })
        .to_string();

        let Response::Components { components } = serde_json::from_str(&json).unwrap() else {
            panic!("expected a Components response");
        };
        assert_eq!(components.len(), 2);
        assert!(components[0].is_none());
        let resolved = components[1].as_ref().unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, "resolved");
    }

    fn audio_descriptor(src: &str, start: f64) -> ReactAudioClipDescriptor {
        ReactAudioClipDescriptor {
            src: src.to_owned(),
            source_start: 0.0,
            playback_rate: Animatable::Static(1.0),
            volume: Animatable::Static(1.0),
            muted: false,
            start,
            duration: 2.0,
        }
    }

    #[test]
    fn merge_react_audio_clips_collapses_identical_per_frame_reports() {
        let clips = vec![
            audio_descriptor("a.wav", 0.0),
            audio_descriptor("a.wav", 0.0),
            audio_descriptor("b.wav", 1.0),
            audio_descriptor("a.wav", 0.0),
        ];
        let merged = merge_react_audio_clips(&clips);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].0.src, "a.wav");
        assert_eq!(merged[0].1, 3, "three frames reported the first clip");
        assert_eq!(merged[1].0.src, "b.wav");
        assert_eq!(merged[1].1, 1);
    }

    #[test]
    fn react_audio_clips_resolve_relative_paths_and_carry_ranges() {
        let clips = vec![
            audio_descriptor("./voice.wav", 1.0),
            audio_descriptor("/abs/music.wav", 0.0),
        ];
        let built = react_audio_clips(&clips, Path::new("/entry/dir"));
        assert_eq!(built.len(), 2);
        assert_eq!(built[0].id, "react-audio:0");
        assert_eq!(built[0].range.start.as_seconds().unwrap(), 1.0);
        assert_eq!(built[0].range.duration.as_seconds().unwrap(), 2.0);
        let AssetLocation::File { path } = &built[0].asset.location else {
            panic!("expected a file asset");
        };
        assert!(
            path.replace('\\', "/").ends_with("/entry/dir/./voice.wav"),
            "relative src joined against the entry dir, got {path}"
        );
        let AssetLocation::File { path } = &built[1].asset.location else {
            panic!("expected a file asset");
        };
        assert_eq!(path, "/abs/music.wav", "absolute src left untouched");
    }
}
