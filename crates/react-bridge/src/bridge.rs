use crate::audio::react_audio_clips;
use crate::error::ReactBridgeError;
use crate::measure::measure_text_response;
use crate::probe::{MediaProbeResponse, media_probe_payload};
use crate::protocol::{
    ComponentRequest, ProjectPayload, ReadyMessage, Request, ResolutionRuntime, Response,
};
use crate::types::{
    ComponentResolutionRequest, FrameEvaluation, ProjectFrame, ReactCompositionMetadata,
};
use celesta_composition::{AudioGraph, Layer, Scene, Time};
use celesta_media::FfmpegBackend;
use celesta_remote::{RemoteAssetCache, is_remote_url};
use celesta_renderer::TextRasterizer;
use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// A live connection to the `@celesta/react` CLI evaluating one entry module.
pub struct ReactBridge {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: BufReader<ChildStdout>,
    /// The last message read from `stdout`.
    pub(crate) message: String,
    pub(crate) metadata: ReactCompositionMetadata,
    pub(crate) text_measurer: Option<TextRasterizer>,
}

impl Drop for ReactBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ReactBridge {
    /// Spawns `node <cli_script> <entry>` and reads its startup configuration.
    /// Release builds default to production React; an explicit `NODE_ENV`
    /// is preserved so callers can opt into development diagnostics.
    pub fn spawn(
        node: impl AsRef<Path>,
        cli_script: impl AsRef<Path>,
        entry: impl AsRef<Path>,
    ) -> Result<Self, ReactBridgeError> {
        let node = node.as_ref();
        let mut command = Command::new(node);
        if !cfg!(debug_assertions) && std::env::var_os("NODE_ENV").is_none() {
            command.env("NODE_ENV", "production");
        }
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
        let mut message = String::new();

        let mut media = FfmpegBackend::new();
        // Created on the first `measureText`: it scans the system fonts.
        let mut text_measurer: Option<TextRasterizer> = None;
        let metadata = loop {
            if let Err(error) = read_message(&mut stdout, &mut message) {
                if matches!(error, ReactBridgeError::UnexpectedExit) {
                    let _ = child.wait();
                }
                return Err(error);
            }
            let ready: ReadyMessage =
                serde_json::from_str(&message).map_err(ReactBridgeError::Protocol)?;
            match ready {
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
                    let path = probe_media.path;
                    let probed = if is_remote_url(&path) {
                        RemoteAssetCache::standard()
                            .fetch(&path)
                            .map_err(|error| error.to_string())
                    } else {
                        Ok(PathBuf::from(&path))
                    }
                    .and_then(|local| media.probe(local).map_err(|error| error.to_string()));
                    let response = match probed {
                        Ok(probe) => MediaProbeResponse {
                            media: Some(media_probe_payload(probe)),
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
                ReadyMessage::Error { error } => {
                    return Err(ReactBridgeError::EntryFailed(error));
                }
            }
        };

        Ok(Self {
            child,
            stdin,
            stdout,
            message,
            metadata,
            text_measurer,
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
        self.write_request(Request {
            time: Some(time),
            project: project.map(|frame| ProjectPayload {
                layers: frame.layers,
                tracks: frame.tracks,
            }),
            runtime: None,
            components: None,
            compact_transforms: true,
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
        match serde_json::from_str(&frame.0).map_err(ReactBridgeError::Protocol)? {
            Response::Ok { scene, audio } => Ok(FrameEvaluation { scene, audio }),
            Response::Components { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::CollectedAudio { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
            Response::MeasureText { .. } => Err(ReactBridgeError::UnexpectedResponse),
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
            compact_transforms: false,
        };
        match self.request_response(request)? {
            Response::Components { components } => Ok(components),
            Response::Ok { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::CollectedAudio { .. } => Err(ReactBridgeError::UnexpectedResponse),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
            Response::MeasureText { .. } => Err(ReactBridgeError::UnexpectedResponse),
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
        let reports = match self.request_response(serde_json::json!({ "collectAudio": true }))? {
            Response::CollectedAudio { collected_audio } => collected_audio,
            Response::Err { error } => return Err(ReactBridgeError::Render(error)),
            _ => return Err(ReactBridgeError::UnexpectedResponse),
        };
        Ok(AudioGraph {
            sample_rate,
            master_volume,
            clips: react_audio_clips(&reports, entry_dir),
        })
    }

    pub(crate) fn request_response<R: Serialize>(
        &mut self,
        request: R,
    ) -> Result<Response, ReactBridgeError> {
        self.write_request(request)?;
        self.receive_message()?;
        serde_json::from_str(&self.message).map_err(ReactBridgeError::Protocol)
    }

    fn write_request<R: Serialize>(&mut self, request: R) -> Result<(), ReactBridgeError> {
        let payload = serde_json::to_string(&request).map_err(ReactBridgeError::Protocol)?;
        writeln!(self.stdin, "{payload}").map_err(ReactBridgeError::Io)?;
        self.stdin.flush().map_err(ReactBridgeError::Io)
    }

    /// Reads messages into `self.message` until one is not a text
    /// measurement, answering each measurement as it arrives.
    fn receive_message(&mut self) -> Result<(), ReactBridgeError> {
        loop {
            read_message(&mut self.stdout, &mut self.message)?;
            // The CLI writes the key first; checking it leaves a frame's
            // scene to be parsed only once, possibly later.
            if !self.message.starts_with(r#"{"measureText":"#) {
                return Ok(());
            }
            let Response::MeasureText { measure_text } =
                serde_json::from_str(&self.message).map_err(ReactBridgeError::Protocol)?
            else {
                return Err(ReactBridgeError::UnexpectedResponse);
            };
            let metrics = measure_text_response(&mut self.text_measurer, &measure_text);
            serde_json::to_writer(&mut self.stdin, &metrics).map_err(ReactBridgeError::Protocol)?;
            writeln!(self.stdin).map_err(ReactBridgeError::Io)?;
            self.stdin.flush().map_err(ReactBridgeError::Io)?;
        }
    }
}

/// A frame's response read by [`ReactBridge::receive_frame`], not yet
/// decoded.
pub struct PendingFrame(String);

/// Reads one JSON line from the CLI into `buffer`. It stays a `String`:
/// `read_line` has already checked the UTF-8, which `serde_json::from_str`
/// then skips and `from_slice` would check again.
fn read_message(
    stdout: &mut BufReader<ChildStdout>,
    buffer: &mut String,
) -> Result<(), ReactBridgeError> {
    buffer.clear();
    let read = stdout.read_line(buffer).map_err(ReactBridgeError::Io)?;
    if read == 0 {
        return Err(ReactBridgeError::UnexpectedExit);
    }
    Ok(())
}
