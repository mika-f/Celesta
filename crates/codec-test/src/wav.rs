//! PCM WAV, as the checked-in voices are: RIFF/WAVE with a 16-bit PCM
//! `fmt ` chunk, other chunks skipped.

use crate::format::invalid;
use celesta_media::MediaError;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// The probe's codec name for a WAV's stream.
pub(crate) const CODEC: &str = "pcm_s16le";

/// A WAV file's audio.
pub(crate) struct Wav {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved, `i16 as f32 / 32768.0`.
    pub samples: Vec<f32>,
}

impl Wav {
    pub(crate) fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

/// Reads the WAV file `file`, opened from `path`.
pub(crate) fn read(path: &Path, mut file: File) -> Result<Wav, MediaError> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| invalid(path, format_args!("could not be read: {error}")))?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid(path, "is not a RIFF/WAVE file"));
    }

    let mut format = None;
    let mut data = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(&bytes, at + 4) as usize;
        let body = at + 8;
        let end = body.saturating_add(size);
        match id {
            b"fmt " => {
                if size < 16 || end > bytes.len() {
                    return Err(invalid(path, "has a truncated fmt chunk"));
                }
                format = Some((
                    u16_at(&bytes, body),
                    u16_at(&bytes, body + 2),
                    u32_at(&bytes, body + 4),
                    u16_at(&bytes, body + 14),
                ));
            }
            // A writer that streamed the file may not have known its size.
            b"data" => data = Some(&bytes[body..end.min(bytes.len())]),
            _ => {}
        }
        // Chunks are padded to an even size.
        at = end.saturating_add(size % 2);
    }

    let Some((tag, channels, sample_rate, bits)) = format else {
        return Err(invalid(path, "has no fmt chunk"));
    };
    if tag != 1 || bits != 16 {
        return Err(invalid(
            path,
            format_args!(
                "is a WAV of format tag {tag} at {bits} bits; \
                 the test codec reads 16-bit PCM (format tag 1) only"
            ),
        ));
    }
    if channels == 0 || sample_rate == 0 {
        return Err(invalid(
            path,
            format_args!("is a WAV of {sample_rate} Hz with {channels} channels"),
        ));
    }
    let Some(data) = data else {
        return Err(invalid(path, "has no data chunk"));
    };
    if !data.len().is_multiple_of(2 * usize::from(channels)) {
        return Err(invalid(
            path,
            format_args!(
                "has {} bytes of samples, not whole frames of {channels} 16-bit channels",
                data.len()
            ),
        ));
    }
    Ok(Wav {
        sample_rate,
        channels,
        samples: data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&sample| f32::from(i16::from_le_bytes(sample)) / 32768.0)
            .collect(),
    })
}
