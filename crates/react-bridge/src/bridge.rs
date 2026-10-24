use crate::animated_images::AnimatedImages;
use crate::audio::{react_audio_clips, react_video_clip_ids};
use crate::decode::{DecodedAudioFiles, decode_audio_response};
use crate::error::ReactBridgeError;
use crate::measure::measure_text_response;
use crate::occurrences::OccurrenceIndex;
use crate::probe::{MediaProbeResponse, animation_probe_payload, media_probe_payload};
use crate::properties::PropertyInputs;
use crate::protocol::{
    ComponentRequest, ProjectPayload, PropertyInputsMessage, ReadyMessage, Request,
    ResolutionRuntime, Response, StaticsRequest, ready_property_order,
};
use crate::statics::StaticCache;
use crate::stderr::StderrCapture;
use crate::types::{
    ComponentResolutionRequest, ComponentResolutions, FrameEvaluation, LayerSourceReport,
    ProjectFrame, PropOverride, ReactCompositionInfo, ReactCompositionMetadata,
    ReactSubtitleDescriptor, SpawnOptions,
};
use celesta_composition::{AudioGraph, Scene, Time};
use celesta_media::FfmpegBackend;
use celesta_remote::{RemoteAssetCache, is_remote_url};
use celesta_renderer::TextRasterizer;
use celesta_renderer::image_source::probe_animation;
use serde::Serialize;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// A live connection to the `@celesta/cli` process evaluating one entry module.
pub struct ReactBridge {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: BufReader<ChildStdout>,
    /// The runtime's stderr, passed through and kept to explain an exit.
    pub(crate) stderr: StderrCapture,
    /// The last message read from `stdout`, kept to reuse its allocation.
    pub(crate) message: Vec<u8>,
    pub(crate) metadata: ReactCompositionMetadata,
    pub(crate) text_measurer: Option<TextRasterizer>,
    /// The `<Static>` subtrees the runtime has sent.
    pub(crate) statics: StaticCache,
    pub(crate) animated_images: AnimatedImages,
    /// Whether frame requests ask for debug guides.
    pub(crate) debug_guides: bool,
    /// Whether frame requests are evaluated as the editor preview.
    pub(crate) preview: bool,
}

impl Drop for ReactBridge {
    fn drop(&mut self) {
        // This also closes Node's stderr, which ends its reader thread.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ReactBridge {
    /// Spawns `node <cli_script> <entry> --length-prefixed` and reads its
    /// startup configuration. The CLI then writes every message as a
    /// little-endian `u32` byte count followed by the JSON.
    /// Release builds default to production React; an explicit `NODE_ENV`
    /// is preserved so callers can opt into development diagnostics.
    pub fn spawn(
        node: impl AsRef<Path>,
        cli_script: impl AsRef<Path>,
        entry: impl AsRef<Path>,
    ) -> Result<Self, ReactBridgeError> {
        Self::spawn_with_properties(node, cli_script, entry, &PropertyInputs::default())
    }

    /// Same as [`Self::spawn`], also giving the entry project property
    /// values (with `--properties-stdin` appended when there are any). They
    /// are checked against its `defineProjectProperties()`
    /// schema before `prepare()`; rejected values fail with
    /// [`ReactBridgeError::InvalidProperties`].
    pub fn spawn_with_properties(
        node: impl AsRef<Path>,
        cli_script: impl AsRef<Path>,
        entry: impl AsRef<Path>,
        properties: &PropertyInputs,
    ) -> Result<Self, ReactBridgeError> {
        Self::spawn_with_options(node, cli_script, entry, properties, SpawnOptions::default())
    }

    /// Same as [`Self::spawn_with_properties`], also giving the runtime
    /// `options` (see [`SpawnOptions`]).
    pub fn spawn_with_options(
        node: impl AsRef<Path>,
        cli_script: impl AsRef<Path>,
        entry: impl AsRef<Path>,
        properties: &PropertyInputs,
        options: SpawnOptions,
    ) -> Result<Self, ReactBridgeError> {
        let node = node.as_ref();
        // Relative asset paths in a scene resolve against the entry's directory.
        let asset_root = entry.as_ref().parent().unwrap_or(Path::new("")).to_owned();
        let mut command = Command::new(node);
        if !cfg!(debug_assertions) && std::env::var_os("NODE_ENV").is_none() {
            command.env("NODE_ENV", "production");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        command
            .arg(cli_script.as_ref())
            .arg(entry.as_ref())
            .arg("--length-prefixed");
        let property_payload = if properties.is_empty() {
            None
        } else {
            command.arg("--properties-stdin");
            let payload = serde_json::to_string(&PropertyInputsMessage {
                property_inputs: properties.layers(),
            })
            .map_err(ReactBridgeError::Protocol)?;
            Some(payload)
        };
        if let Some(id) = &options.composition_id {
            command.arg("--composition").arg(id);
        }
        if options.source_locations {
            command.arg("--source-locations");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ReactBridgeError::Executable {
                executable: node.to_owned(),
                source,
            })?;
        let (mut stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(stdin), Some(stdout), Some(stderr)) => (stdin, stdout, stderr),
                _ => {
                    end(&mut child);
                    return Err(ReactBridgeError::MissingPipe);
                }
            };
        let stderr = match StderrCapture::start(stderr) {
            Ok(stderr) => stderr,
            Err(error) => {
                end(&mut child);
                return Err(ReactBridgeError::Io(error));
            }
        };
        let mut stdout = BufReader::new(stdout);
        let mut message = Vec::new();
        if let Some(payload) = property_payload {
            // A runtime that already exited fails below as `UnexpectedExit`,
            // with its last stderr lines.
            let _ = writeln!(stdin, "{payload}").and_then(|()| stdin.flush());
        }

        let mut media = FfmpegBackend::new();
        // Created on the first `measureText`: it scans the system fonts.
        let mut text_measurer: Option<TextRasterizer> = None;
        let mut decoded_audio = DecodedAudioFiles::default();
        let mut handshake = || -> Result<ReactCompositionMetadata, ReactBridgeError> {
            loop {
                read_message(&mut stdout, &mut message)?;
                // Node reads a decoded file before it sends anything else.
                decoded_audio.remove();
                let ready: ReadyMessage =
                    serde_json::from_slice(&message).map_err(ReactBridgeError::Protocol)?;
                match ready {
                    ReadyMessage::Ready {
                        config,
                        composition_id,
                        mut compositions,
                        component_schemas,
                        project_property_schema,
                        dependencies,
                    } => {
                        // Without an order, fall back to the map's sorted keys.
                        let mut project_property_order = ready_property_order(&message);
                        if project_property_order.is_empty()
                            && let Some(schema) = &project_property_schema
                        {
                            project_property_order = schema.keys().cloned().collect();
                        }
                        let composition_id = composition_id.unwrap_or_else(|| "default".to_owned());
                        if compositions.is_empty() {
                            compositions.push(ReactCompositionInfo {
                                id: composition_id.clone(),
                                width: config.width,
                                height: config.height,
                                frame_rate: config.frame_rate,
                                duration_in_frames: config.duration_in_frames,
                            });
                        }
                        return Ok(ReactCompositionMetadata {
                            composition_id,
                            compositions,
                            width: config.width,
                            height: config.height,
                            frame_rate: config.frame_rate,
                            duration_in_frames: config.duration_in_frames,
                            component_schemas,
                            project_property_schema,
                            project_property_order,
                            prepare_dependencies: dependencies,
                            chapters: config.chapters,
                        });
                    }
                    ReadyMessage::ProbeMedia { probe_media } => {
                        let path = probe_media.path;
                        let local = if is_remote_url(&path) {
                            RemoteAssetCache::standard()
                                .fetch(&path)
                                .map_err(|error| error.to_string())
                        } else {
                            Ok(PathBuf::from(&path))
                        };
                        // `<Image>` plays an animated image itself; FFmpeg reads
                        // some of them as one frame, or not at all.
                        let animation = local
                            .as_ref()
                            .ok()
                            .and_then(|local| probe_animation(local).ok().flatten());
                        let probed = match &animation {
                            Some(animation) => Ok(animation_probe_payload(animation)),
                            None => local.and_then(|local| {
                                media
                                    .probe(local)
                                    .map(media_probe_payload)
                                    .map_err(|error| error.to_string())
                            }),
                        };
                        let response = match probed {
                            Ok(media) => MediaProbeResponse {
                                media: Some(media),
                                error: None,
                            },
                            Err(error) => MediaProbeResponse {
                                media: None,
                                error: Some(format!("could not probe {path}: {error}")),
                            },
                        };
                        let payload =
                            serde_json::to_string(&response).map_err(ReactBridgeError::Protocol)?;
                        writeln!(stdin, "{payload}").map_err(ReactBridgeError::Io)?;
                        stdin.flush().map_err(ReactBridgeError::Io)?;
                    }
                    ReadyMessage::MeasureText { measure_text } => {
                        let response = measure_text_response(&mut text_measurer, &measure_text);
                        let payload =
                            serde_json::to_string(&response).map_err(ReactBridgeError::Protocol)?;
                        writeln!(stdin, "{payload}").map_err(ReactBridgeError::Io)?;
                        stdin.flush().map_err(ReactBridgeError::Io)?;
                    }
                    ReadyMessage::DecodeAudio { decode_audio } => {
                        let response = decode_audio_response(
                            &mut media,
                            &mut decoded_audio,
                            &decode_audio.path,
                        );
                        let payload =
                            serde_json::to_string(&response).map_err(ReactBridgeError::Protocol)?;
                        writeln!(stdin, "{payload}").map_err(ReactBridgeError::Io)?;
                        stdin.flush().map_err(ReactBridgeError::Io)?;
                    }
                    ReadyMessage::InvalidProperties { invalid_properties } => {
                        return Err(ReactBridgeError::InvalidProperties(invalid_properties));
                    }
                    ReadyMessage::Error { error } => {
                        return Err(ReactBridgeError::EntryFailed(error));
                    }
                }
            }
        };
        let metadata = match handshake() {
            Ok(metadata) => metadata,
            Err(error) => {
                let error = stderr.exit_error(&mut child, error);
                // The entry may already be imported; handles it opened could
                // keep Node alive.
                end(&mut child);
                return Err(error);
            }
        };

        Ok(Self {
            child,
            stdin,
            stdout,
            stderr,
            message,
            metadata,
            text_measurer,
            statics: StaticCache::default(),
            animated_images: AnimatedImages::new(asset_root),
            debug_guides: false,
            preview: false,
        })
    }

    pub const fn metadata(&self) -> &ReactCompositionMetadata {
        &self.metadata
    }

    /// Makes later frame requests draw debug guides: `<DebugOverlay>`,
    /// `<DebugBounds>` and whatever else reads `useDebugGuides()`, which
    /// otherwise draw only in the editor preview. For checking stills; a
    /// video export leaves this off. A runtime too old to know the request
    /// ignores it.
    pub const fn set_debug_guides(&mut self, debug_guides: bool) {
        self.debug_guides = debug_guides;
    }

    /// Makes later frame requests evaluate the composition as the editor
    /// preview: `useIsPreview()` is true, and so `useDebugGuides()` too.
    /// Only the editor's preview sets this; exports and inspection never do.
    /// A runtime too old to know the request ignores it.
    pub const fn set_preview(&mut self, preview: bool) {
        self.preview = preview;
    }

    /// Requests the evaluated `Scene` at an exact composition time.
    pub fn scene_at(&mut self, time: Time) -> Result<Scene, ReactBridgeError> {
        Ok(self.evaluate_at(time, None)?.scene)
    }

    /// Same as [`Self::scene_at`], but also hands the entry's
    /// `<ProjectTimeline />`/`<ProjectTrack />`/`useProjectTrack()` a
    /// project's layers already evaluated for this exact time. The Node
    /// side receives project evaluation from the caller up front; it can
    /// request text measurements from this bridge while rendering.
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
        self.submit_frame(time, project)?;
        let frame = self.receive_frame()?;
        self.decode_frame(frame)
    }

    /// Evaluates a frame past the composition's end as if the video went on,
    /// for checking that a loop's last frame runs into its first: a
    /// `<Sequence>` that ends with the composition stays open, and
    /// `useVideoConfig()` still reports the real durations. Fails with
    /// [`ReactBridgeError::OpenEndUnsupported`] when the runtime is too old
    /// to honor the request.
    pub fn evaluate_past_end(&mut self, time: Time) -> Result<FrameEvaluation, ReactBridgeError> {
        self.write_request(Request {
            time: Some(time),
            project: None,
            runtime: None,
            components: None,
            compact_transforms: true,
            open_end: true,
            debug_guides: self.debug_guides,
            preview: self.preview,
            statics: Some(StaticsRequest {
                decoded: self.statics.decoded(),
            }),
            prop_overrides: &[],
        })?;
        let frame = self.receive_frame()?;
        match self.decode_response(frame)? {
            Response::Ok {
                scene,
                audio,
                subtitles,
                open_end: true,
                ..
            } => Ok(FrameEvaluation {
                scene,
                audio,
                subtitles,
            }),
            Response::Ok { .. } => Err(ReactBridgeError::OpenEndUnsupported),
            response => Err(Self::unexpected_frame_response(response)),
        }
    }

    /// Asks Node to evaluate the frame at `time`, without waiting for it.
    /// Every submitted frame must be collected, in order, with
    /// [`Self::receive_frame`] before any other request is made.
    ///
    /// Together with [`Self::receive_frame`] and [`Self::decode_frame`] this
    /// lets a caller that renders consecutive frames pipeline them: submit
    /// frame N + 1 between receiving frame N and decoding it, so Node
    /// evaluates the next frame while this side parses the current one.
    pub fn submit_frame(
        &mut self,
        time: Time,
        project: Option<ProjectFrame<'_>>,
    ) -> Result<(), ReactBridgeError> {
        self.submit(time, project, &[])
    }

    /// Same as [`Self::scene_at`], but the elements written at each
    /// override's position take its value in this frame only, as if the
    /// source said so: everything computed from the value follows. For the
    /// editor's preview of a drag; the runtime must have been spawned with
    /// source locations (see [`SpawnOptions`]), or the overrides match
    /// nothing. The next frame asked for without overrides draws the written
    /// values again.
    pub fn scene_at_with_overrides(
        &mut self,
        time: Time,
        prop_overrides: &[PropOverride],
    ) -> Result<Scene, ReactBridgeError> {
        self.submit(time, None, prop_overrides)?;
        let frame = self.receive_frame()?;
        Ok(self.decode_frame(frame)?.scene)
    }

    fn submit(
        &mut self,
        time: Time,
        project: Option<ProjectFrame<'_>>,
        prop_overrides: &[PropOverride],
    ) -> Result<(), ReactBridgeError> {
        self.write_request(Request {
            time: Some(time),
            project: project.map(|frame| ProjectPayload {
                layers: frame.layers,
                tracks: frame.tracks,
            }),
            runtime: None,
            components: None,
            compact_transforms: true,
            open_end: false,
            debug_guides: self.debug_guides,
            preview: self.preview,
            statics: Some(StaticsRequest {
                decoded: self.statics.decoded(),
            }),
            prop_overrides,
        })
    }

    /// Waits for the oldest submitted frame and returns its response still
    /// encoded. Text measurements Node asks for meanwhile are answered here.
    pub fn receive_frame(&mut self) -> Result<PendingFrame, ReactBridgeError> {
        self.receive_message()?;
        Ok(PendingFrame(std::mem::take(&mut self.message)))
    }

    /// Decodes a frame from [`Self::receive_frame`].
    pub fn decode_frame(
        &mut self,
        frame: PendingFrame,
    ) -> Result<FrameEvaluation, ReactBridgeError> {
        match self.decode_response(frame)? {
            Response::Ok {
                scene,
                audio,
                subtitles,
                ..
            } => Ok(FrameEvaluation {
                scene,
                audio,
                subtitles,
            }),
            response => Err(Self::unexpected_frame_response(response)),
        }
    }

    fn decode_response(&mut self, frame: PendingFrame) -> Result<Response, ReactBridgeError> {
        let response = serde_json::from_slice(&frame.0).map_err(ReactBridgeError::Protocol);
        // Keep the larger allocation for the next message.
        if frame.0.capacity() > self.message.capacity() {
            self.message = frame.0;
        }
        let mut response = response?;
        // A frame's `<Static>` references become the layers they stand for
        // before anything else sees the scene.
        if let Response::Ok {
            scene,
            seq,
            statics,
            dropped_statics,
            ..
        } = &mut response
        {
            self.statics.apply(
                *seq,
                std::mem::take(statics),
                std::mem::take(dropped_statics),
                scene,
            )?;
            self.animated_images.apply(&mut scene.layers);
        }
        Ok(response)
    }

    /// The error for a frame request answered with anything but a scene.
    fn unexpected_frame_response(response: Response) -> ReactBridgeError {
        match response {
            Response::Err { error } => ReactBridgeError::Render(error),
            _ => ReactBridgeError::UnexpectedResponse,
        }
    }

    /// Resolves individual `registerComponent()` names against the entry's
    /// registry without rendering the whole composition — one rendered layer
    /// list per request, in order, with `None` for names nothing registered,
    /// plus the custom shaders those layers use.
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
    ) -> Result<ComponentResolutions, ReactBridgeError> {
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
            compact_transforms: false,
            open_end: false,
            debug_guides: false,
            preview: false,
            statics: None,
            prop_overrides: &[],
        };
        match self.request_response(request)? {
            Response::Components {
                components,
                shaders,
            } => Ok(ComponentResolutions {
                components,
                shaders,
            }),
            Response::Ok { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::CollectedAudio { .. }
            | Response::CollectedSubtitles { .. }
            | Response::CollectedOccurrences { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
            Response::MeasureText { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::LayerSource { .. } => Err(ReactBridgeError::UnexpectedResponse),
        }
    }

    /// The JSX that produced the layer `id` in the frame at `time`: a chain
    /// with the layer's own element first, then each enclosing component,
    /// plus the props and local frame the runtime saw. `None` when no layer
    /// of the frame has that id. The chain is empty unless the
    /// bridge was spawned with [`SpawnOptions::source_locations`]. The frame
    /// is evaluated as [`Self::set_preview`] says, so it matches the layers
    /// the preview shows.
    pub fn layer_source(
        &mut self,
        time: Time,
        id: &str,
    ) -> Result<Option<LayerSourceReport>, ReactBridgeError> {
        let request = serde_json::json!({
            "layerSource": id,
            "time": time,
            "preview": self.preview,
        });
        match self.request_response(request)? {
            Response::LayerSource { report } => Ok(report),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
            _ => Err(ReactBridgeError::UnexpectedResponse),
        }
    }

    /// Sweeps every frame of the composition and builds the complete
    /// `AudioGraph` its `<Audio>` declarations imply — the standalone
    /// counterpart of what `celesta-exporter` accumulates during its render
    /// loop. Node evaluates all frames in one request, retaining hooks and
    /// text measurements but omitting visual layers and per-frame scene JSON.
    /// Conditional / sequence-shifted audio is still captured exactly;
    /// [`merge_react_audio_clips`] collapses the per-frame duplicates.
    /// Relative `src` paths resolve against `entry_dir`.
    pub fn collect_audio_graph(
        &mut self,
        sample_rate: u32,
        master_volume: f64,
        entry_dir: &Path,
    ) -> Result<AudioGraph, ReactBridgeError> {
        Ok(self
            .collect_media(sample_rate, master_volume, entry_dir)?
            .audio)
    }

    /// [`Self::collect_audio_graph`], also telling which of its clips play a
    /// `<Video>`'s soundtrack: every `<Video>` reports one, so they also
    /// place its picture in time.
    pub fn collect_media(
        &mut self,
        sample_rate: u32,
        master_volume: f64,
        entry_dir: &Path,
    ) -> Result<ReactMedia, ReactBridgeError> {
        let reports = match self.request_response(serde_json::json!({ "collectAudio": true }))? {
            Response::CollectedAudio { collected_audio } => collected_audio,
            Response::Err { error } => return Err(ReactBridgeError::Render(error)),
            _ => return Err(ReactBridgeError::UnexpectedResponse),
        };
        Ok(ReactMedia {
            audio: AudioGraph {
                sample_rate,
                master_volume,
                clips: react_audio_clips(&reports, entry_dir)?,
            },
            video_clip_ids: react_video_clip_ids(&reports),
        })
    }

    /// The subtitles shown on `frames` frames from zero-based frame `first`
    /// (cut to the composition), each once in the order first shown: what a
    /// video export of those frames reports, without building their scenes
    /// here. A subtitle shown across several calls is reported by each.
    pub fn collect_subtitles(
        &mut self,
        first: u64,
        frames: u64,
    ) -> Result<Vec<ReactSubtitleDescriptor>, ReactBridgeError> {
        let request = serde_json::json!({
            "collectSubtitles": { "from": first, "frames": frames },
        });
        match self.request_response(request)? {
            Response::CollectedSubtitles {
                collected_subtitles,
            } => Ok(collected_subtitles),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
            _ => Err(ReactBridgeError::UnexpectedResponse),
        }
    }

    /// Where each element renders in `frames` frames from zero-based frame
    /// `from` (cut to the composition), by instance. The frames are evaluated
    /// as [`Self::set_preview`] says, like [`Self::layer_source`], so an
    /// instance found there is in this index. Sweep the composition in
    /// chunks and [`OccurrenceIndex::merge`] them to index all of it.
    /// The index is empty unless the bridge was spawned with
    /// [`SpawnOptions::source_locations`].
    pub fn collect_occurrences(
        &mut self,
        from: u64,
        frames: u64,
    ) -> Result<OccurrenceIndex, ReactBridgeError> {
        let request = serde_json::json!({
            "collectOccurrences": { "from": from, "frames": frames, "preview": self.preview },
        });
        match self.request_response(request)? {
            Response::CollectedOccurrences {
                collected_occurrences,
            } => Ok(collected_occurrences),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
            _ => Err(ReactBridgeError::UnexpectedResponse),
        }
    }

    pub(crate) fn request_response<R: Serialize>(
        &mut self,
        request: R,
    ) -> Result<Response, ReactBridgeError> {
        self.write_request(request)?;
        self.receive_message()?;
        serde_json::from_slice(&self.message).map_err(ReactBridgeError::Protocol)
    }

    fn write_request<R: Serialize>(&mut self, request: R) -> Result<(), ReactBridgeError> {
        let written = write_line(&mut self.stdin, &request);
        written.map_err(|error| self.exit_error(error))
    }

    /// Reads messages into `self.message` until one is not a text
    /// measurement, answering each measurement as it arrives.
    fn receive_message(&mut self) -> Result<(), ReactBridgeError> {
        let received = self.receive_answering_measurements();
        received.map_err(|error| self.exit_error(error))
    }

    /// Gives an error from a runtime that went away the reason it printed;
    /// see [`StderrCapture::exit_error`].
    fn exit_error(&mut self, error: ReactBridgeError) -> ReactBridgeError {
        self.stderr.exit_error(&mut self.child, error)
    }

    fn receive_answering_measurements(&mut self) -> Result<(), ReactBridgeError> {
        loop {
            read_message(&mut self.stdout, &mut self.message)?;
            // The CLI writes the key first; checking it leaves a frame's
            // scene to be parsed only once, possibly later.
            if !self.message.starts_with(br#"{"measureText":"#) {
                return Ok(());
            }
            let Response::MeasureText { measure_text } =
                serde_json::from_slice(&self.message).map_err(ReactBridgeError::Protocol)?
            else {
                return Err(ReactBridgeError::UnexpectedResponse);
            };
            let metrics = measure_text_response(&mut self.text_measurer, &measure_text);
            // `receive_message` explains a closed pipe here.
            write_line(&mut self.stdin, &metrics)?;
        }
    }
}

/// Writes `message` to the CLI as one line of JSON.
fn write_line(stdin: &mut ChildStdin, message: &impl Serialize) -> Result<(), ReactBridgeError> {
    let payload = serde_json::to_string(message).map_err(ReactBridgeError::Protocol)?;
    writeln!(stdin, "{payload}")
        .and_then(|()| stdin.flush())
        .map_err(ReactBridgeError::Io)
}

/// Ends a runtime this bridge gives up on before it is set up.
fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// A frame's response read by [`ReactBridge::receive_frame`], not yet
/// decoded.
pub struct PendingFrame(Vec<u8>);

/// What [`ReactBridge::collect_media`] found: the composition's audio graph
/// and the ids of its clips that a `<Video>` plays.
#[derive(Clone, Debug)]
pub struct ReactMedia {
    pub audio: AudioGraph,
    pub video_clip_ids: Vec<String>,
}

/// Reads one message from the CLI into `buffer`, reusing its allocation. With
/// `--length-prefixed` every message is a little-endian `u32` byte count
/// followed by that many bytes of JSON, so a frame's scene is read in a few
/// large reads instead of being scanned for its line end.
///
/// A closed stdout fails as an `UnexpectedExit` without stderr;
/// [`StderrCapture::exit_error`] fills it in.
fn read_message(
    stdout: &mut BufReader<ChildStdout>,
    buffer: &mut Vec<u8>,
) -> Result<(), ReactBridgeError> {
    let exited = || ReactBridgeError::UnexpectedExit {
        stderr: String::new(),
    };
    let eof = |error: io::Error| match error.kind() {
        io::ErrorKind::UnexpectedEof => exited(),
        _ => ReactBridgeError::Io(error),
    };
    let mut length = [0; 4];
    stdout.read_exact(&mut length).map_err(eof)?;
    let length = u32::from_le_bytes(length) as usize;
    buffer.clear();
    buffer.reserve(length);
    let read = stdout
        .take(length as u64)
        .read_to_end(buffer)
        .map_err(ReactBridgeError::Io)?;
    if read == length {
        Ok(())
    } else {
        Err(exited())
    }
}
