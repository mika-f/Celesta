use crate::decoder::{AudioDecoder, VideoFrameDecoder};
use crate::error::MediaError;
use crate::mix::video_seconds;
use crate::provider::{
    Container, EncoderId, EncoderSettings, MediaProvider, MuxRequest, Sequential, VideoRequest,
};
use crate::registry::{ComponentRef, MediaRegistry, registry};
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use celesta_plugin_host::PluginKillSwitch;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Media files through the plugins of a [`MediaRegistry`] (see
/// [`crate::set_registry`]): probes, cached by path, video frames, decoded
/// audio, and encoding.
///
/// The first request for a file routes it: each of the registry's
/// candidate decoders is asked to probe it in turn, and the first that
/// reads it serves every later request for it. A plugin's process is
/// started on first use, and started again once when it stops answering.
pub struct MediaBackend {
    /// `None` for the registry [`crate::set_registry`] chose when one is
    /// needed.
    registry: Option<Arc<MediaRegistry>>,
    /// The instances started, by plugin index in the registry.
    processes: Processes,
    /// The decoder each file was routed to, with its plugin's index.
    routes: HashMap<PathBuf, (usize, ComponentRef)>,
    probes: HashMap<PathBuf, MediaProbe>,
    sequential_frame_rate: Option<Rational>,
}

impl MediaBackend {
    /// A backend on the registry [`crate::set_registry`] has chosen when it
    /// first needs one, so a backend made before the choice still uses it.
    pub fn new() -> Self {
        Self::with(None)
    }

    /// A backend on `registry`'s plugins, whatever [`crate::set_registry`]
    /// chose.
    pub fn with_registry(registry: Arc<MediaRegistry>) -> Self {
        Self::with(Some(registry))
    }

    fn with(registry: Option<Arc<MediaRegistry>>) -> Self {
        Self {
            registry,
            processes: Processes::default(),
            routes: HashMap::new(),
            probes: HashMap::new(),
            sequential_frame_rate: None,
        }
    }

    /// Answers [`VideoFrameDecoder::decode_frame_for`] as a run of requests
    /// moving forward at about `frame_rate`, which a decoder can serve
    /// without seeking for every frame: playback and export.
    pub fn with_sequential_video(mut self, frame_rate: Rational) -> Self {
        if frame_rate.is_valid() {
            self.sequential_frame_rate = Some(frame_rate);
        }
        self
    }

    /// Registers the plugin processes this backend starts with `kill`,
    /// which another thread can kill to end a request in progress, such as
    /// an export's Cancel; once it is killed, no plugin process starts. A
    /// provider in this process is not stopped by it.
    pub fn with_kill_switch(mut self, kill: PluginKillSwitch) -> Self {
        self.processes.kill = Some(kill);
        self
    }

    /// The decoder that serves `path`, once a request has routed it.
    pub fn decoder_of(&self, path: &Path) -> Option<&ComponentRef> {
        self.routes.get(path).map(|(_, decoder)| decoder)
    }

    pub fn probe(&mut self, path: impl AsRef<Path>) -> Result<&MediaProbe, MediaError> {
        let path = path.as_ref();
        self.route(path)?;
        Ok(self
            .probes
            .get(path)
            .expect("a routed path's probe is cached"))
    }

    /// The registry this backend uses: the one it was given, or else the
    /// one [`crate::set_registry`] chose, kept from its first use on.
    fn registry(&mut self) -> Option<&MediaRegistry> {
        if self.registry.is_none() {
            self.registry = registry();
        }
        self.registry.as_deref()
    }

    /// Pins `path` to a decoder, unless a request already did: the first
    /// candidate whose probe reads it, with that probe cached.
    ///
    /// A missing file fails with [`MediaError::NotFound`] before any plugin
    /// starts. A candidate that answers [`MediaError::Unsupported`] moves
    /// on to the next, as does one with any other error, which is kept; but
    /// [`MediaError::CodecStopped`] (a deadline missed, or the kill switch)
    /// ends routing. When no candidate reads the file, the error is the
    /// most specific one kept (see [`specificity`]), the first among equals,
    /// or else [`MediaError::Unsupported`] naming every candidate.
    fn route(&mut self, path: &Path) -> Result<(), MediaError> {
        if self.routes.contains_key(path) {
            return Ok(());
        }
        if !path.exists() {
            return Err(MediaError::NotFound(path.to_owned()));
        }
        // Borrowed from the field, not through `Self::registry`, so the
        // processes, probes, and routes stay free to change.
        self.registry();
        let registry = self.registry.as_deref().ok_or(MediaError::NoDecoder)?;
        let candidates = registry.decoder_candidates(path);
        if candidates.is_empty() {
            return Err(MediaError::NoDecoder);
        }
        let mut kept = None;
        for (plugin, decoder) in &candidates {
            let probe = self.processes.retrying(registry, *plugin, |provider| {
                provider.probe(&decoder.component, path)
            });
            match probe {
                Ok(probe) => {
                    registry.record_use(decoder, registry.plugin(*plugin).origin, path);
                    self.probes.insert(path.to_owned(), probe);
                    self.routes
                        .insert(path.to_owned(), (*plugin, decoder.clone()));
                    return Ok(());
                }
                Err(MediaError::Unsupported { .. }) => {}
                Err(stopped @ MediaError::CodecStopped(_)) => return Err(stopped),
                Err(error) => {
                    if kept
                        .as_ref()
                        .is_none_or(|kept| specificity(&error) < specificity(kept))
                    {
                        kept = Some(error);
                    }
                }
            }
        }
        Err(kept.unwrap_or_else(|| MediaError::Unsupported {
            path: path.to_owned(),
            tried: candidates
                .iter()
                .map(|(_, decoder)| decoder.to_string())
                .collect(),
        }))
    }

    /// Runs `request` on the decoder `path` is routed to, routing it first.
    /// An error, even [`MediaError::Unsupported`], is the decoder's answer:
    /// a pinned path does not move to another decoder. An `Unsupported`
    /// names that decoder as the one tried, since a plugin's answer names
    /// none.
    fn decoding<T>(
        &mut self,
        path: &Path,
        mut request: impl FnMut(&mut dyn MediaProvider, &str) -> Result<T, MediaError>,
    ) -> Result<T, MediaError> {
        self.route(path)?;
        let registry = self.registry.as_deref().expect("routing found a registry");
        let (plugin, decoder) = self.routes.get(path).expect("routed above");
        self.processes
            .retrying(registry, *plugin, |provider| {
                request(provider, &decoder.component)
            })
            .map_err(|error| match error {
                MediaError::Unsupported { path, tried } if tried.is_empty() => {
                    MediaError::Unsupported {
                        path,
                        tried: vec![decoder.to_string()],
                    }
                }
                error => error,
            })
    }

    /// The index of the plugin that encodes.
    fn encoder(&mut self) -> Result<usize, MediaError> {
        self.registry()
            .and_then(MediaRegistry::legacy_encoder)
            .ok_or(MediaError::NoCodec)
    }

    /// Fails with [`MediaError::EncoderUnavailable`] when the encoding
    /// plugin cannot write `container`, before anything is rendered.
    pub fn check_encoder(&mut self, container: Container) -> Result<(), MediaError> {
        self.encoding(|provider| provider.check_encoder(container))
    }

    pub fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
        self.encoding(|provider| provider.open_encoder(settings))
    }

    pub fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
        self.encoder_process()?.encode_frame(encoder, pixels)
    }

    pub fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
        self.encoder_process()?.finish_encoder(encoder)
    }

    pub fn abort_encoder(&mut self, encoder: EncoderId) {
        let Some(plugin) = self
            .registry
            .as_deref()
            .and_then(MediaRegistry::legacy_encoder)
        else {
            return;
        };
        if let Some(provider) = self.processes.open.get_mut(&plugin) {
            provider.abort_encoder(encoder);
        }
    }

    pub fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
        self.encoding(|provider| provider.mux_audio(request))
    }

    /// Runs a request that keeps nothing open on the encoding plugin (see
    /// [`Processes::retrying`]).
    fn encoding<T>(
        &mut self,
        request: impl FnMut(&mut dyn MediaProvider) -> Result<T, MediaError>,
    ) -> Result<T, MediaError> {
        let plugin = self.encoder()?;
        let registry = self.registry.as_deref().expect("the encoder is in it");
        self.processes.retrying(registry, plugin, request)
    }

    /// The encoding plugin's instance, which an encoder opened on it keeps
    /// open.
    fn encoder_process(&mut self) -> Result<&mut dyn MediaProvider, MediaError> {
        let plugin = self.encoder()?;
        let registry = self.registry.as_deref().expect("the encoder is in it");
        self.processes.get(registry, plugin)
    }
}

/// How much an error a candidate decoder answered says about the file,
/// most first: that it is not there, then that a decoder read it and found
/// no video, then anything else.
fn specificity(error: &MediaError) -> u8 {
    match error {
        MediaError::NotFound(_) => 0,
        MediaError::NoVideoStream(_) => 1,
        _ => 2,
    }
}

impl Default for MediaBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// The plugin instances a backend has started, by plugin index in its
/// registry.
#[derive(Default)]
struct Processes {
    open: HashMap<usize, Box<dyn MediaProvider>>,
    /// What kills the plugin processes from another thread; once it is
    /// killed, none starts.
    kill: Option<PluginKillSwitch>,
}

impl Processes {
    /// The instance of `registry`'s plugin `plugin`, started if it is not
    /// running.
    fn get(
        &mut self,
        registry: &MediaRegistry,
        plugin: usize,
    ) -> Result<&mut dyn MediaProvider, MediaError> {
        if !self.open.contains_key(&plugin) {
            // A backend without a switch gives its plugins one nothing
            // kills. A plugin process refuses to start on a killed switch;
            // a provider in this process has no process to kill and starts
            // as usual.
            let kill = self.kill.clone().unwrap_or_default();
            let provider = (registry.plugin(plugin).factory)(&kill)?;
            self.open.insert(plugin, provider);
        }
        Ok(self.open.get_mut(&plugin).expect("started above").as_mut())
    }

    /// Runs `request` on plugin `plugin`'s instance; once more on a fresh
    /// one if the first went away. One that was stopped (it missed its
    /// deadline, or was killed) is dropped without being asked again; the
    /// next request starts another. Only for requests that keep nothing
    /// open on the plugin's side that the caller relies on.
    fn retrying<T>(
        &mut self,
        registry: &MediaRegistry,
        plugin: usize,
        mut request: impl FnMut(&mut dyn MediaProvider) -> Result<T, MediaError>,
    ) -> Result<T, MediaError> {
        let result = match request(self.get(registry, plugin)?) {
            Err(MediaError::CodecUnavailable(_)) => {
                self.open.remove(&plugin);
                request(self.get(registry, plugin)?)
            }
            result => result,
        };
        if matches!(result, Err(MediaError::CodecStopped(_))) {
            self.open.remove(&plugin);
        }
        result
    }
}

impl VideoFrameDecoder for MediaBackend {
    fn decode_frame(
        &mut self,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        self.decoding(path, |provider, decoder| {
            provider.decode_video(
                decoder,
                &VideoRequest {
                    path,
                    time: source_time_seconds,
                    sequential: None,
                },
            )
        })
    }

    fn decode_frame_for(
        &mut self,
        request_id: &str,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        let sequential = self.sequential_frame_rate.map(|frame_rate| Sequential {
            session: request_id,
            frame_rate,
        });
        self.decoding(path, |provider, decoder| {
            provider.decode_video(
                decoder,
                &VideoRequest {
                    path,
                    time: source_time_seconds,
                    sequential,
                },
            )
        })
    }

    fn video_duration(&mut self, path: &Path) -> Result<Option<f64>, MediaError> {
        let probe = self.probe(path)?;
        Ok(probe
            .video
            .as_ref()
            .and_then(|video| video_seconds(video, probe.duration)))
    }
}

impl AudioDecoder for MediaBackend {
    fn decode_audio(
        &mut self,
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        // A decoder mixes down to mono or stereo only.
        if sample_rate == 0 || !(1..=2).contains(&channels) {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            });
        }
        let audio = self.decoding(path, |provider, decoder| {
            provider.decode_audio(decoder, path, sample_rate, channels)
        })?;
        if audio.sample_rate != sample_rate || audio.channels != channels {
            let decoder = self.decoder_of(path).expect("routed to decode");
            return Err(MediaError::Codec(format!(
                "{decoder} decoded {} at {} Hz, {} channels, not {sample_rate} Hz, {channels} channels",
                path.display(),
                audio.sample_rate,
                audio.channels
            )));
        }
        if audio.samples.len() % usize::from(channels) != 0 {
            return Err(MediaError::InvalidAudioSampleCount {
                samples: audio.samples.len(),
                channels,
            });
        }
        Ok(audio)
    }
}
