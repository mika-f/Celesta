use crate::codec::{
    CodecFactory, CodecProvider, Container, EncoderId, EncoderSettings, MuxRequest, Sequential,
    VideoRequest, codec,
};
use crate::decoder::{AudioDecoder, VideoFrameDecoder};
use crate::error::MediaError;
use crate::mix::video_seconds;
use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
use celesta_composition::Rational;
use celesta_plugin_host::PluginKillSwitch;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

/// Media files through a codec (see [`crate::set_codec`]): probes, cached
/// by path, video frames, decoded audio, and encoding. The codec is opened
/// on first use, and opened again once when it stops answering.
pub struct MediaBackend {
    /// `None` for the codec [`crate::set_codec`] chose when one is needed.
    factory: Option<CodecFactory>,
    codec: Option<Box<dyn CodecProvider>>,
    /// What kills the codec's process from another thread; once it is
    /// killed, no codec is opened.
    kill: Option<PluginKillSwitch>,
    probes: HashMap<PathBuf, MediaProbe>,
    sequential_frame_rate: Option<Rational>,
}

impl MediaBackend {
    /// A backend on the codec [`crate::set_codec`] has chosen when it first
    /// needs one, so a backend made before the choice still uses it.
    pub fn new() -> Self {
        Self::with_factory(None)
    }

    /// A backend on `factory`'s codec, whatever [`crate::set_codec`] chose.
    pub fn with_codec(factory: CodecFactory) -> Self {
        Self::with_factory(Some(factory))
    }

    /// A backend on `factory`, or on no codec at all whatever
    /// [`crate::set_codec`] chose, for tests.
    #[cfg(test)]
    pub(crate) fn with_factory_for_test(factory: Option<CodecFactory>) -> Self {
        let mut backend = Self::with_factory(factory.clone());
        if factory.is_none() {
            backend.factory = Some(Arc::new(|_| Err(MediaError::NoCodec)));
        }
        backend
    }

    fn with_factory(factory: Option<CodecFactory>) -> Self {
        Self {
            factory,
            codec: None,
            kill: None,
            probes: HashMap::new(),
            sequential_frame_rate: None,
        }
    }

    /// Answers [`VideoFrameDecoder::decode_frame_for`] as a run of requests
    /// moving forward at about `frame_rate`, which the codec can serve
    /// without seeking for every frame: playback and export.
    pub fn with_sequential_video(mut self, frame_rate: Rational) -> Self {
        if frame_rate.is_valid() {
            self.sequential_frame_rate = Some(frame_rate);
        }
        self
    }

    /// Registers the codec process this backend opens with `kill`, which
    /// another thread can kill to end a request in progress, such as an
    /// export's Cancel. A codec in this process is not stopped by it.
    pub fn with_kill_switch(mut self, kill: PluginKillSwitch) -> Self {
        self.kill = Some(kill);
        self
    }

    pub fn probe(&mut self, path: impl AsRef<Path>) -> Result<&MediaProbe, MediaError> {
        let path = path.as_ref();
        if !self.probes.contains_key(path) {
            let probe = self.retrying(|codec| codec.probe(path))?;
            self.probes.insert(path.to_owned(), probe);
        }
        Ok(self.probes.get(path).expect("probe was cached"))
    }

    fn codec(&mut self) -> Result<&mut dyn CodecProvider, MediaError> {
        if self.codec.is_none() {
            let factory = self
                .factory
                .clone()
                .or_else(codec)
                .ok_or(MediaError::NoCodec)?;
            // A backend without a switch gives its codec one nothing kills.
            let kill = self.kill.clone().unwrap_or_default();
            if kill.is_killed() {
                return Err(MediaError::CodecStopped(
                    "it was not started, as its plugins were stopped".to_owned(),
                ));
            }
            self.codec = Some(factory(&kill)?);
        }
        Ok(self.codec.as_deref_mut().expect("opened above"))
    }

    /// Runs `request` on the codec; once more on a fresh one if the first
    /// went away. One that was stopped (it missed its deadline, or was
    /// killed) is dropped without being asked again; the next request opens
    /// another. Only for requests that keep nothing open on the codec's side
    /// that the caller relies on.
    fn retrying<T>(
        &mut self,
        mut request: impl FnMut(&mut dyn CodecProvider) -> Result<T, MediaError>,
    ) -> Result<T, MediaError> {
        match request(self.codec()?) {
            Err(MediaError::CodecUnavailable(_)) => {
                self.codec = None;
                request(self.codec()?)
            }
            Err(stopped @ MediaError::CodecStopped(_)) => {
                self.codec = None;
                Err(stopped)
            }
            result => result,
        }
    }

    /// Fails with [`MediaError::EncoderUnavailable`] when the codec cannot
    /// write `container`, before anything is rendered.
    pub fn check_encoder(&mut self, container: Container) -> Result<(), MediaError> {
        self.retrying(|codec| codec.check_encoder(container))
    }

    pub fn open_encoder(&mut self, settings: &EncoderSettings) -> Result<EncoderId, MediaError> {
        self.retrying(|codec| codec.open_encoder(settings))
    }

    pub fn encode_frame(&mut self, encoder: EncoderId, pixels: Vec<u8>) -> Result<(), MediaError> {
        self.codec()?.encode_frame(encoder, pixels)
    }

    pub fn finish_encoder(&mut self, encoder: EncoderId) -> Result<(), MediaError> {
        self.codec()?.finish_encoder(encoder)
    }

    pub fn abort_encoder(&mut self, encoder: EncoderId) {
        if let Some(codec) = self.codec.as_deref_mut() {
            codec.abort_encoder(encoder);
        }
    }

    pub fn mux_audio(&mut self, request: &MuxRequest) -> Result<(), MediaError> {
        self.retrying(|codec| codec.mux_audio(request))
    }
}

impl Default for MediaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoFrameDecoder for MediaBackend {
    fn decode_frame(
        &mut self,
        path: &Path,
        source_time_seconds: f64,
    ) -> Result<VideoFrame, MediaError> {
        self.retrying(|codec| {
            codec.decode_video(&VideoRequest {
                path,
                time: source_time_seconds,
                sequential: None,
            })
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
        self.retrying(|codec| {
            codec.decode_video(&VideoRequest {
                path,
                time: source_time_seconds,
                sequential,
            })
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
        // A codec mixes down to mono or stereo only.
        if sample_rate == 0 || !(1..=2).contains(&channels) {
            return Err(MediaError::InvalidAudioFormat {
                sample_rate,
                channels,
            });
        }
        let audio = self.retrying(|codec| codec.decode_audio(path, sample_rate, channels))?;
        if audio.sample_rate != sample_rate || audio.channels != channels {
            return Err(MediaError::Codec(format!(
                "the codec decoded {} at {} Hz, {} channels, not {sample_rate} Hz, {channels} channels",
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
