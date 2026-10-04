use crate::audio_cache::DiskAudioCache;
use crate::meter::MasterLevels;
use crate::preview::AudioPreview;
use crate::waveform::{clip_level_envelope, map_clip_waveform, waveform_peaks};
use celesta_composition::{AudioClip, AudioGraph, Time};
use celesta_media::{
    AudioBuffer, AudioDecoder, FfmpegBackend, MediaError, mix_audio_graph_cancellable,
};
use celesta_remote::resolve_asset_path;
use gpui_kit::Pixels;
use rodio::buffer::SamplesBuffer;
use rodio::{DeviceSinkBuilder, Player};
use std::collections::HashMap;
use std::error::Error;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

pub(crate) struct AudioMixRequest {
    pub(crate) generation: u64,
    pub(crate) graph: AudioGraph,
    pub(crate) asset_root: PathBuf,
    pub(crate) duration: Time,
}

pub(crate) struct AudioMixResult {
    pub(crate) generation: u64,
    pub(crate) output: Result<AudioMixOutput, String>,
}

pub(crate) struct AudioMixOutput {
    pub(crate) clip_waveforms: HashMap<String, Vec<f32>>,
    pub(crate) clip_levels: HashMap<String, Vec<f32>>,
    pub(crate) master_levels: MasterLevels,
    pub(crate) buffer: AudioBuffer,
}

pub(crate) struct MasterVolumeDrag {
    pub(crate) pointer_x: Pixels,
    pub(crate) start_volume: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AudioCacheKey {
    pub(crate) path: PathBuf,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
}

pub(crate) struct CachedAudioDecoder {
    pub(crate) backend: FfmpegBackend,
    pub(crate) buffers: HashMap<AudioCacheKey, AudioBuffer>,
    pub(crate) waveforms: HashMap<AudioCacheKey, Vec<f32>>,
    pub(crate) disk_cache: DiskAudioCache,
}

impl CachedAudioDecoder {
    pub(crate) fn new() -> Self {
        Self::with_disk_cache(DiskAudioCache::standard())
    }

    pub(crate) fn with_disk_cache(disk_cache: DiskAudioCache) -> Self {
        Self {
            backend: FfmpegBackend::new(),
            buffers: HashMap::new(),
            waveforms: HashMap::new(),
            disk_cache,
        }
    }

    pub(crate) fn clip_waveform(
        &self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
        clip: &AudioClip,
    ) -> Option<Vec<f32>> {
        let key = AudioCacheKey {
            path: path.to_owned(),
            sample_rate,
            channels,
        };
        let peaks = self.waveforms.get(&key)?;
        let source_frames = self.buffers.get(&key)?.frame_count();
        Some(map_clip_waveform(
            peaks,
            source_frames,
            sample_rate,
            clip,
            peaks.len(),
        ))
    }
}

impl AudioDecoder for CachedAudioDecoder {
    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        let key = AudioCacheKey {
            path: path.to_owned(),
            sample_rate,
            channels,
        };
        if let Some(buffer) = self.buffers.get(&key) {
            return Ok(buffer.clone());
        }
        if let Ok(Some(cached)) = self.disk_cache.load(path, sample_rate, channels) {
            self.waveforms.insert(key.clone(), cached.waveform);
            self.buffers.insert(key, cached.buffer.clone());
            return Ok(cached.buffer);
        }
        let buffer = self.backend.decode_audio(path, sample_rate, channels)?;
        let waveform = waveform_peaks(&buffer, 512);
        let _ = self.disk_cache.store(path, &buffer, &waveform);
        self.waveforms.insert(key.clone(), waveform);
        self.buffers.insert(key, buffer.clone());
        Ok(buffer)
    }
}

pub(crate) struct AudioMixWorker {
    pub(crate) requests: mpsc::Sender<AudioMixRequest>,
    pub(crate) results: mpsc::Receiver<AudioMixResult>,
    pub(crate) current_generation: Arc<AtomicU64>,
}

impl AudioMixWorker {
    pub(crate) fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<AudioMixRequest>();
        let (result_tx, result_rx) = mpsc::channel::<AudioMixResult>();
        let current_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&current_generation);
        thread::Builder::new()
            .name("celesta-audio-mix".to_owned())
            .spawn(move || {
                let mut decoder = CachedAudioDecoder::new();
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let output = mix_audio_graph_cancellable(
                        &request.graph,
                        &request.asset_root,
                        request.duration,
                        &mut decoder,
                        || worker_generation.load(Ordering::Acquire) != request.generation,
                    )
                    .map(|buffer| {
                        let mut clip_waveforms = HashMap::new();
                        let mut clip_levels = HashMap::new();
                        for clip in &request.graph.clips {
                            let Some((clip_id, waveform)) = (|| {
                                // The mix above already downloaded any URL.
                                let path =
                                    resolve_asset_path(&request.asset_root, &clip.asset.location)
                                        .ok()?;
                                let waveform = decoder.clip_waveform(
                                    &path,
                                    request.graph.sample_rate,
                                    2,
                                    clip,
                                )?;
                                let clip_id = clip
                                    .id
                                    .strip_suffix(":voice")
                                    .unwrap_or(&clip.id)
                                    .to_owned();
                                Some((clip_id, waveform))
                            })() else {
                                continue;
                            };
                            if !clip.muted {
                                clip_levels
                                    .insert(clip_id.clone(), clip_level_envelope(&waveform, clip));
                            }
                            clip_waveforms.insert(clip_id, waveform);
                        }
                        AudioMixOutput {
                            clip_waveforms,
                            clip_levels,
                            master_levels: MasterLevels::from_buffer(&buffer),
                            buffer,
                        }
                    })
                    .map_err(|error| error.to_string());
                    if result_tx
                        .send(AudioMixResult {
                            generation: request.generation,
                            output,
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
            current_generation,
        })
    }

    pub(crate) fn request(&self, request: AudioMixRequest) -> Result<(), String> {
        self.cancel_before(request.generation);
        self.requests
            .send(request)
            .map_err(|_| "audio mix worker stopped unexpectedly".to_owned())
    }

    pub(crate) fn cancel_before(&self, generation: u64) {
        self.current_generation.store(generation, Ordering::Release);
    }
}

pub(crate) fn take_latest<T>(mut latest: T, receiver: &mpsc::Receiver<T>) -> T {
    while let Ok(next) = receiver.try_recv() {
        latest = next;
    }
    latest
}

impl AudioPreview {
    pub(crate) fn from_buffer(buffer: AudioBuffer, volume: f64) -> Result<Self, Box<dyn Error>> {
        let channels = NonZeroU16::new(buffer.channels).ok_or("audio has zero channels")?;
        let sample_rate =
            NonZeroU32::new(buffer.sample_rate).ok_or("audio has a zero sample rate")?;
        let source = SamplesBuffer::new(channels, sample_rate, buffer.samples);
        let device_sink = DeviceSinkBuilder::open_default_sink()?;
        let player = Player::connect_new(device_sink.mixer());
        let volume = volume as f32;
        player.set_volume(volume);
        player.append(source.clone());
        player.pause();
        Ok(Self {
            _device_sink: device_sink,
            player,
            source,
            volume,
        })
    }

    pub(crate) fn set_volume(&mut self, volume: f64) {
        self.volume = volume as f32;
        self.player.set_volume(self.volume);
    }

    pub(crate) fn seek(&mut self, time: Time, playing: bool) -> Result<(), Box<dyn Error>> {
        self.player.stop();
        self.player = Player::connect_new(self._device_sink.mixer());
        self.player.set_volume(self.volume);
        self.player.append(self.source.clone());
        self.player.pause();
        let seconds = time.as_seconds()?.max(0.0);
        self.player.try_seek(Duration::from_secs_f64(seconds))?;
        if playing {
            self.player.play();
        }
        Ok(())
    }

    pub(crate) fn pause(&self) {
        self.player.pause();
    }
}
