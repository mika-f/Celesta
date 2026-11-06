//! `decodeAudio`: the PCM an entry's `prepare()` analyzes, decoded through
//! the codec plugin by the same `MediaBackend` that mixes preview and export
//! audio.

use celesta_media::{AudioDecoder, MediaBackend};
use celesta_remote::local_asset_path;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;
use tempfile::TempPath;

#[derive(Deserialize)]
pub(crate) struct DecodeAudioRequest {
    /// A local path, or an `http`/`https` URL.
    pub(crate) path: String,
}

#[derive(Serialize)]
pub(crate) struct DecodeAudioResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) audio: Option<DecodedAudioPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

/// Mono little-endian `f32` samples in a temporary file: tens of megabytes
/// for a song, which a JSON line would carry far more slowly.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecodedAudioPayload {
    pub(crate) path: PathBuf,
    pub(crate) sample_rate: u32,
    pub(crate) frames: usize,
}

/// The temporary files handed to Node, removed once Node has read them: when
/// its next message arrives, or when the handshake ends.
#[derive(Default)]
pub(crate) struct DecodedAudioFiles(Vec<TempPath>);

impl DecodedAudioFiles {
    pub(crate) fn remove(&mut self) {
        // Dropping a `TempPath` removes its file.
        self.0.clear();
    }
}

pub(crate) fn decode_audio_response(
    media: &mut MediaBackend,
    files: &mut DecodedAudioFiles,
    path: &str,
) -> DecodeAudioResponse {
    match decode_to_file(media, files, path) {
        Ok(audio) => DecodeAudioResponse {
            audio: Some(audio),
            error: None,
        },
        Err(error) => DecodeAudioResponse {
            audio: None,
            error: Some(format!("could not decode {path}: {error}")),
        },
    }
}

fn decode_to_file(
    media: &mut MediaBackend,
    files: &mut DecodedAudioFiles,
    path: &str,
) -> Result<DecodedAudioPayload, String> {
    let local = local_asset_path(path).map_err(|error| error.to_string())?;
    let stream = media
        .probe(&local)
        .map_err(|error| error.to_string())?
        .audio
        .first()
        .cloned()
        .ok_or("it has no audio stream")?;
    // The stream's own rate, so nothing is resampled: the browser runtime
    // decodes without resampling too.
    let sample_rate = stream
        .sample_rate
        .filter(|rate| *rate > 0)
        .unwrap_or(48_000);
    // A mono stream stays as it is; FFmpeg would spread it over two channels
    // at -3 dB each. Anything wider is mixed to stereo and then averaged.
    let channels = if stream.channels == Some(1) { 1 } else { 2 };
    let buffer = media
        .decode_audio(&local, sample_rate, channels)
        .map_err(|error| error.to_string())?;
    let mono = downmix(&buffer.samples, usize::from(channels));
    let mut bytes = Vec::with_capacity(mono.len() * 4);
    for sample in &mono {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    let file = write_temporary_file(&bytes).map_err(|error| error.to_string())?;
    let path = file.to_path_buf();
    files.0.push(file);
    Ok(DecodedAudioPayload {
        path,
        sample_rate,
        frames: mono.len(),
    })
}

/// The mean of each frame's channels.
pub(crate) fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Writes `bytes` to a new file in the temporary directory. The name is not
/// guessable and the file must not exist yet, so on a shared `/tmp` another
/// user cannot have placed a link there for it to write through; only the
/// owner can read it. The file is removed when the returned path drops.
fn write_temporary_file(bytes: &[u8]) -> io::Result<TempPath> {
    let mut file = tempfile::Builder::new()
        .prefix("celesta-pcm-")
        .suffix(".f32le")
        .tempfile()?;
    file.write_all(bytes)?;
    Ok(file.into_temp_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn averages_the_channels_of_each_frame() {
        assert_eq!(downmix(&[0.5, -0.5, 1.0, 0.0], 2), vec![0.0, 0.5]);
        assert_eq!(downmix(&[0.25, -1.0], 1), vec![0.25, -1.0]);
    }

    #[test]
    fn writes_mono_samples_and_removes_them_once_read() {
        let wav =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/voices/001.wav");
        let mut media = MediaBackend::with_codec(celesta_codec_test::TestCodec::factory());
        let mut files = DecodedAudioFiles::default();
        let response = decode_audio_response(&mut media, &mut files, wav.to_str().unwrap());
        let audio = response.audio.expect("decodes the WAV");
        assert_eq!(audio.sample_rate, 44_100);
        assert_eq!(audio.frames, 84_022);
        let bytes = fs::read(&audio.path).unwrap();
        assert_eq!(bytes.len(), audio.frames * 4);
        let sample =
            |index: usize| f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        assert_eq!(sample(0), -11.0 / 32768.0);
        assert_eq!(sample(1), -15.0 / 32768.0);
        files.remove();
        assert!(!audio.path.exists());
    }

    #[test]
    fn temporary_files_get_fresh_private_names() {
        let a = write_temporary_file(b"a").unwrap();
        let b = write_temporary_file(b"b").unwrap();
        assert_ne!(&*a, &*b);
        assert_eq!(fs::read(&a).unwrap(), b"a");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&a).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let paths = [a.to_path_buf(), b.to_path_buf()];
        drop((a, b));
        assert!(paths.iter().all(|path| !path.exists()));
    }

    #[test]
    fn reports_a_file_it_cannot_decode() {
        let mut media = MediaBackend::with_codec(celesta_codec_test::TestCodec::factory());
        let mut files = DecodedAudioFiles::default();
        let response = decode_audio_response(&mut media, &mut files, "missing.wav");
        assert!(response.audio.is_none());
        assert!(
            response
                .error
                .unwrap()
                .starts_with("could not decode missing.wav: ")
        );
    }
}
