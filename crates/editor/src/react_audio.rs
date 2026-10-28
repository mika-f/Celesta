use crate::audio::take_latest;
use crate::problems::error_chain;
use celesta_composition::{AssetLocation, AudioClip, AudioGraph};
use celesta_editor_core::{ClipKind, ClipSummary, TrackSummary};
use celesta_media::FfmpegBackend;
use celesta_project::TrackKind;
use celesta_react_bridge::{PropertyInputs, ReactBridge, SpawnOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

pub(crate) struct ReactAudioRequest {
    pub(crate) generation: u64,
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
    pub(crate) properties: PropertyInputs,
    pub(crate) composition_id: String,
    pub(crate) sample_rate: u32,
    pub(crate) master_volume: f64,
}

pub(crate) struct ReactAudioResult {
    pub(crate) generation: u64,
    pub(crate) media: Result<ReactMediaClips, String>,
}

/// A standalone entry's audio graph, and which of its clips a `<Video>`
/// plays.
pub(crate) struct ReactMediaClips {
    pub(crate) graph: AudioGraph,
    /// The ids of the clips a `<Video>` plays, each with whether its file has
    /// sound. A file that cannot be probed here counts as having sound.
    pub(crate) videos: HashMap<String, bool>,
}

/// Spawns a transient `@celesta/cli` process, sweeps every frame of a
/// standalone entry for its `<Audio>` and `<Video>` declarations, and returns
/// the assembled [`AudioGraph`] — the standalone-preview counterpart of the audio graph
/// `celesta-exporter` accumulates while rendering. Runs on its own thread
/// because `ReactBridge::spawn` and the per-frame sweep both block.
pub(crate) struct ReactAudioWorker {
    pub(crate) requests: mpsc::Sender<ReactAudioRequest>,
    pub(crate) results: mpsc::Receiver<ReactAudioResult>,
}

impl ReactAudioWorker {
    pub(crate) fn spawn() -> anyhow::Result<Self> {
        let (request_tx, request_rx) = mpsc::channel::<ReactAudioRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ReactAudioResult>();
        thread::Builder::new()
            .name("celesta-react-audio".to_owned())
            .spawn(move || {
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let entry_dir = request
                        .entry
                        .parent()
                        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                    let media = ReactBridge::spawn_with_options(
                        &request.node,
                        &request.cli_script,
                        &request.entry,
                        &request.properties,
                        SpawnOptions {
                            composition_id: Some(request.composition_id),
                            ..SpawnOptions::default()
                        },
                    )
                    .map_err(|error| error_chain(&error))
                    .and_then(|mut bridge| {
                        bridge
                            .collect_media(request.sample_rate, request.master_volume, &entry_dir)
                            .map_err(|error| error_chain(&error))
                    })
                    .map(|media| {
                        let mut backend = FfmpegBackend::new();
                        let videos = media
                            .video_clip_ids
                            .into_iter()
                            .map(|id| {
                                let has_audio = media
                                    .audio
                                    .clips
                                    .iter()
                                    .find(|clip| clip.id == id)
                                    .is_none_or(|clip| has_audio(&mut backend, &entry_dir, clip));
                                (id, has_audio)
                            })
                            .collect();
                        ReactMediaClips {
                            graph: media.audio,
                            videos,
                        }
                    });
                    if result_tx
                        .send(ReactAudioResult {
                            generation: request.generation,
                            media,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    pub(crate) fn request(&self, request: ReactAudioRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "react audio worker stopped unexpectedly".to_owned())
    }
}

/// Whether `clip`'s local file has an audio stream. A URL is not downloaded
/// for this, and a file that cannot be probed is left to the mix to report:
/// both count as having sound.
fn has_audio(backend: &mut FfmpegBackend, entry_dir: &Path, clip: &AudioClip) -> bool {
    let AssetLocation::File { path } = &clip.asset.location else {
        return true;
    };
    backend
        .probe(entry_dir.join(path))
        .map_or(true, |probe| !probe.audio.is_empty())
}

/// The timeline's tracks for a standalone React composition: the picture of
/// each `<Video>` on video tracks, then the sound of every `<Audio>` and
/// `<Video>` on audio tracks. `videos` tells the clips a `<Video>` plays, and
/// whether its file has sound; one without has no audio track.
pub(crate) fn react_tracks(
    clips: &[AudioClip],
    videos: &HashMap<String, bool>,
) -> Vec<TrackSummary> {
    let pictures: Vec<_> = clips
        .iter()
        .filter(|clip| videos.contains_key(&clip.id))
        .collect();
    let sounds: Vec<_> = clips
        .iter()
        .filter(|clip| videos.get(&clip.id) != Some(&false))
        .collect();
    let mut tracks = lay_out(&pictures, TrackKind::Video);
    tracks.extend(lay_out(&sounds, TrackKind::Audio));
    tracks
}

/// Lays clips out on tracks of `kind`: one per source file, in the order the
/// files first appear. Clips of one file that play at the same time take as
/// many tracks as they need.
fn lay_out(clips: &[&AudioClip], kind: TrackKind) -> Vec<TrackSummary> {
    let (track_prefix, clip_suffix, clip_kind) = match kind {
        TrackKind::Audio => ("react-audio-track", "", ClipKind::Audio),
        _ => ("react-video-track", ":video", ClipKind::Video),
    };
    let mut sources: Vec<(&str, Vec<&AudioClip>)> = Vec::new();
    for &clip in clips {
        match sources.iter_mut().find(|(src, _)| *src == clip.asset.id) {
            Some((_, group)) => group.push(clip),
            None => sources.push((&clip.asset.id, vec![clip])),
        }
    }
    let mut tracks = Vec::new();
    for (src, mut group) in sources {
        group.sort_by(|a, b| seconds(a.range.start).total_cmp(&seconds(b.range.start)));
        let name = source_name(src);
        // Each track of this file and the second its last clip ends.
        let mut lanes: Vec<(TrackSummary, f64)> = Vec::new();
        for clip in group {
            let start = seconds(clip.range.start);
            let end = clip.range.end().map_or(start, seconds);
            let lane = match lanes.iter().position(|(_, lane_end)| *lane_end <= start) {
                Some(index) => index,
                None => {
                    lanes.push((
                        TrackSummary {
                            id: format!("{track_prefix}:{}", tracks.len() + lanes.len()),
                            name: name.clone(),
                            kind,
                            item_count: 0,
                            clips: Vec::new(),
                            enabled: true,
                            locked: false,
                            muted: false,
                            solo: false,
                        },
                        start,
                    ));
                    lanes.len() - 1
                }
            };
            let (track, lane_end) = &mut lanes[lane];
            let sound = kind == TrackKind::Audio;
            track.clips.push(ClipSummary {
                // The picture of a `<Video>` gets an id of its own, so its
                // sound's waveform and level stay on its audio track.
                id: format!("{}{clip_suffix}", clip.id),
                name: name.clone(),
                start: clip.range.start,
                duration: clip.range.duration,
                kind: clip_kind,
                // Muting silences the sound, not the picture.
                enabled: !sound || !clip.muted,
                volume: sound.then(|| clip.volume.clone()),
                component: None,
                dialogue: None,
            });
            track.item_count += 1;
            *lane_end = end;
        }
        tracks.extend(lanes.into_iter().map(|(track, _)| track));
    }
    tracks
}

/// The id of the [`AudioClip`] a timeline clip of [`react_tracks`] shows.
pub(crate) fn react_audio_clip_id(timeline_clip_id: &str) -> &str {
    timeline_clip_id
        .strip_suffix(":video")
        .unwrap_or(timeline_clip_id)
}

fn seconds(time: celesta_composition::Time) -> f64 {
    time.as_seconds().unwrap_or(0.0)
}

/// A clip's `src` as the timeline names it: the file name, or the whole
/// `src` when it has none.
fn source_name(src: &str) -> String {
    Path::new(src).file_name().map_or_else(
        || src.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
