//! The test codec's file: what an export wrote, stored exactly.
//!
//! One file per output, whatever its extension: the magic `CLSTTEST`, a
//! `u32` little-endian format version, a `u32` little-endian header length,
//! the JSON header ([`Header`]), then chunks. Each chunk is a tag byte, a
//! `u32` little-endian payload length, and the payload, compressed with
//! `lz4_flex::block::compress_prepend_size`: `V` one video frame (the bytes
//! the encoder was given), `A` the whole audio as interleaved `f32`
//! little-endian samples (the lead-in included), and `E` the end, with an
//! empty payload. The frame count is the number of `V` chunks; a file
//! without its `E` is unfinished and is refused.

use celesta_composition::Rational;
use celesta_media::{Container, MediaError, PixelFormat};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"CLSTTEST";
const VERSION: u32 = 1;
const VIDEO: u8 = b'V';
const AUDIO: u8 = b'A';
const END: u8 = b'E';

/// What a test codec file holds, as [`read`] gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct TestMedia {
    /// The container it was written as; `None` for a file written with
    /// [`write_rgba_video`] or [`write_audio`].
    pub container: Option<Container>,
    pub video: Option<StoredVideo>,
    pub audio: Option<StoredAudio>,
}

/// A video as its encoder was given it.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredVideo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub format: PixelFormat,
    /// Each frame, byte for byte, in `format`.
    pub frames: Vec<Vec<u8>>,
}

/// Audio as it was muxed in.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredAudio {
    pub sample_rate: u32,
    pub channels: u16,
    /// Sample frames at the start that play before the video's start.
    pub lead_in_frames: usize,
    /// Interleaved, the lead-in included.
    pub samples: Vec<f32>,
}

impl StoredAudio {
    /// The samples before the video's start.
    pub fn lead_in(&self) -> &[f32] {
        &self.samples[..self.lead_in_len()]
    }

    /// The samples from the video's start on, as decoding gives them.
    pub fn played(&self) -> &[f32] {
        &self.samples[self.lead_in_len()..]
    }

    fn lead_in_len(&self) -> usize {
        self.lead_in_frames
            .saturating_mul(usize::from(self.channels))
            .min(self.samples.len())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Header {
    pub container: Option<Container>,
    pub video: Option<VideoHeader>,
    pub audio: Option<AudioHeader>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoHeader {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub format: PixelFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AudioHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub lead_in_frames: usize,
}

/// An error about the file at `path`, naming it.
pub(crate) fn invalid(path: &Path, reason: impl std::fmt::Display) -> MediaError {
    MediaError::Codec(format!("{}: {reason}", path.display()))
}

fn read_error(path: &Path, error: std::io::Error) -> MediaError {
    invalid(path, format_args!("could not be read: {error}"))
}

fn write_error(path: &Path, error: std::io::Error) -> MediaError {
    invalid(path, format_args!("could not be written: {error}"))
}

/// Opens `path` to read, a missing file being [`MediaError::NotFound`].
pub(crate) fn open(path: &Path) -> Result<File, MediaError> {
    File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => MediaError::NotFound(path.to_owned()),
        _ => read_error(path, error),
    })
}

/// What a file is, by its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Test,
    Wav,
}

/// Opens `path` and tells what it is: a test codec file or a WAV file, and
/// [`MediaError::Unsupported`] for any other.
pub(crate) fn sniff(path: &Path) -> Result<(Kind, File), MediaError> {
    let mut file = open(path)?;
    let mut head = Vec::with_capacity(12);
    (&mut file)
        .take(12)
        .read_to_end(&mut head)
        .map_err(|error| read_error(path, error))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| read_error(path, error))?;
    let kind = if head.starts_with(MAGIC) {
        Kind::Test
    } else if head.len() == 12 && &head[..4] == b"RIFF" && &head[8..] == b"WAVE" {
        Kind::Wav
    } else {
        return Err(MediaError::Unsupported {
            path: path.to_owned(),
            tried: Vec::new(),
        });
    };
    Ok((kind, file))
}

/// The bytes of one `width`x`height` frame in `format`, chroma rounded up
/// for odd sizes.
pub(crate) fn frame_len(format: PixelFormat, width: u32, height: u32) -> Option<usize> {
    let (width, height) = (u64::from(width), u64::from(height));
    let pixels = width.checked_mul(height)?;
    let chroma = width.div_ceil(2).checked_mul(height.div_ceil(2))?;
    let bytes = match format {
        PixelFormat::Rgba8 => pixels.checked_mul(4)?,
        PixelFormat::Rgba16 => pixels.checked_mul(8)?,
        PixelFormat::Yuv420p | PixelFormat::Nv12 => pixels.checked_add(chroma.checked_mul(2)?)?,
        PixelFormat::Yuv420p10 | PixelFormat::P010 => {
            pixels.checked_add(chroma.checked_mul(2)?)?.checked_mul(2)?
        }
    };
    usize::try_from(bytes).ok()
}

/// The frame size of `video`, refusing what cannot be written or read.
fn checked_video(path: &Path, video: &VideoHeader) -> Result<usize, MediaError> {
    if video.width == 0 || video.height == 0 {
        return Err(invalid(
            path,
            format_args!("a video cannot be {}x{}", video.width, video.height),
        ));
    }
    if !video.frame_rate.is_valid() {
        return Err(invalid(
            path,
            format_args!(
                "frame rate {}/{} is not positive",
                video.frame_rate.numerator, video.frame_rate.denominator
            ),
        ));
    }
    frame_len(video.format, video.width, video.height).ok_or(MediaError::FrameTooLarge {
        width: video.width,
        height: video.height,
    })
}

fn checked_audio(sample_rate: u32, channels: u16) -> Result<(), MediaError> {
    if sample_rate == 0 || channels == 0 {
        return Err(MediaError::InvalidAudioFormat {
            sample_rate,
            channels,
        });
    }
    Ok(())
}

/// A file being written.
struct Writer {
    file: BufWriter<File>,
    path: PathBuf,
}

impl Writer {
    fn create(path: &Path, header: &Header) -> Result<Self, MediaError> {
        let file = File::create(path).map_err(|error| write_error(path, error))?;
        let mut writer = Self {
            file: BufWriter::new(file),
            path: path.to_owned(),
        };
        let json = serde_json::to_vec(header).expect("the header serializes");
        let header_len = u32::try_from(json.len()).expect("the header is small");
        writer.write(MAGIC)?;
        writer.write(&VERSION.to_le_bytes())?;
        writer.write(&header_len.to_le_bytes())?;
        writer.write(&json)?;
        Ok(writer)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), MediaError> {
        self.file
            .write_all(bytes)
            .map_err(|error| write_error(&self.path, error))
    }

    /// Writes a chunk whose payload is already compressed.
    fn chunk(&mut self, tag: u8, payload: &[u8]) -> Result<(), MediaError> {
        let len = u32::try_from(payload.len())
            .map_err(|_| invalid(&self.path, "a chunk is larger than 4 GiB"))?;
        self.write(&[tag])?;
        self.write(&len.to_le_bytes())?;
        self.write(payload)
    }

    fn compressed(&mut self, tag: u8, data: &[u8]) -> Result<(), MediaError> {
        self.chunk(tag, &lz4_flex::block::compress_prepend_size(data))
    }

    /// Writes the end and makes the file durable.
    fn finish(mut self) -> Result<(), MediaError> {
        self.chunk(END, &[])?;
        let file = self
            .file
            .into_inner()
            .map_err(|error| write_error(&self.path, error.into_error()))?;
        file.sync_all()
            .map_err(|error| write_error(&self.path, error))
    }
}

/// A video file frames are pushed to, as an encoder writes it.
pub(crate) struct VideoWriter {
    writer: Writer,
    video: VideoHeader,
    frame_len: usize,
}

impl VideoWriter {
    pub(crate) fn create(
        path: &Path,
        container: Option<Container>,
        video: VideoHeader,
    ) -> Result<Self, MediaError> {
        let frame_len = checked_video(path, &video)?;
        let header = Header {
            container,
            video: Some(video),
            audio: None,
        };
        Ok(Self {
            writer: Writer::create(path, &header)?,
            video,
            frame_len,
        })
    }

    pub(crate) fn push(&mut self, frame: &[u8]) -> Result<(), MediaError> {
        if frame.len() != self.frame_len {
            let VideoHeader {
                width,
                height,
                format,
                ..
            } = self.video;
            return Err(MediaError::Codec(format!(
                "a {width}x{height} {format:?} frame is {} bytes, not {}",
                self.frame_len,
                frame.len()
            )));
        }
        self.writer.compressed(VIDEO, frame)
    }

    pub(crate) fn finish(self) -> Result<(), MediaError> {
        self.writer.finish()
    }
}

/// Writes an RGBA8 video, as an encoder opened without a container would.
pub fn write_rgba_video(
    path: &Path,
    width: u32,
    height: u32,
    frame_rate: Rational,
    frames: &[Vec<u8>],
) -> Result<(), MediaError> {
    let mut writer = VideoWriter::create(
        path,
        None,
        VideoHeader {
            width,
            height,
            frame_rate,
            format: PixelFormat::Rgba8,
        },
    )?;
    for frame in frames {
        writer.push(frame)?;
    }
    writer.finish()
}

/// Writes an audio-only file of interleaved `samples`.
pub fn write_audio(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Result<(), MediaError> {
    checked_audio(sample_rate, channels)?;
    if !samples.len().is_multiple_of(usize::from(channels)) {
        return Err(MediaError::InvalidAudioSampleCount {
            samples: samples.len(),
            channels,
        });
    }
    let header = Header {
        container: None,
        video: None,
        audio: Some(AudioHeader {
            sample_rate,
            channels,
            lead_in_frames: 0,
        }),
    };
    let mut writer = Writer::create(path, &header)?;
    writer.compressed(AUDIO, &samples_bytes(samples))?;
    writer.finish()
}

fn samples_bytes(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn bytes_samples(bytes: &[u8]) -> Vec<f32> {
    let (samples, _) = bytes.as_chunks::<4>();
    samples.iter().copied().map(f32::from_le_bytes).collect()
}

/// Writes `output`: `video`'s frames as they are stored, with `pcm` (raw
/// interleaved `f32` little-endian samples) as its audio.
pub(crate) fn mux(
    video_path: &Path,
    pcm_path: &Path,
    output: &Path,
    container: Container,
    audio: AudioHeader,
) -> Result<(), MediaError> {
    let mut source = match sniff(video_path)? {
        (Kind::Test, file) => Scanned::new(video_path, file)?,
        (Kind::Wav, _) => return Err(MediaError::NoVideoStream(video_path.to_owned())),
    };
    let Some(video) = source.header.video else {
        return Err(MediaError::NoVideoStream(video_path.to_owned()));
    };
    if source.header.audio.is_some() {
        return Err(invalid(video_path, "already has audio"));
    }
    if let Some(written) = source.header.container
        && written != container
    {
        return Err(invalid(
            video_path,
            format_args!("is a {written:?} video, so it cannot be muxed into {container:?}"),
        ));
    }
    // Read before the output is created, which may be the same file.
    let frames = source
        .frames
        .clone()
        .into_iter()
        .map(|chunk| source.raw(chunk))
        .collect::<Result<Vec<_>, _>>()?;
    checked_audio(audio.sample_rate, audio.channels)?;
    let mut pcm = Vec::new();
    open(pcm_path)?
        .read_to_end(&mut pcm)
        .map_err(|error| read_error(pcm_path, error))?;
    if !pcm.len().is_multiple_of(4) {
        return Err(invalid(
            pcm_path,
            format_args!("{} bytes are not whole f32 samples", pcm.len()),
        ));
    }
    let samples = pcm.len() / 4;
    if !samples.is_multiple_of(usize::from(audio.channels)) {
        return Err(MediaError::InvalidAudioSampleCount {
            samples,
            channels: audio.channels,
        });
    }
    let sample_frames = samples / usize::from(audio.channels);
    if audio.lead_in_frames > sample_frames {
        return Err(invalid(
            pcm_path,
            format_args!(
                "has {sample_frames} sample frames, fewer than its lead-in of {}",
                audio.lead_in_frames
            ),
        ));
    }

    let header = Header {
        container: Some(container),
        video: Some(video),
        audio: Some(audio),
    };
    let mut writer = Writer::create(output, &header)?;
    for frame in frames {
        writer.chunk(VIDEO, &frame)?;
    }
    writer.compressed(AUDIO, &pcm)?;
    writer.finish()
}

/// The next `len` bytes of `reader`.
fn exact(reader: &mut impl Read, len: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes).map(|()| bytes)
}

/// Where a chunk's payload is in its file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chunk {
    offset: u64,
    len: u32,
}

/// A test codec file whose chunks have been found but not read.
pub(crate) struct Scanned {
    pub header: Header,
    pub frames: Vec<Chunk>,
    audio: Option<Chunk>,
    reader: BufReader<File>,
    path: PathBuf,
}

impl Scanned {
    /// Reads the header and finds the chunks of the file `file`, opened from
    /// `path`, refusing one that is unfinished or does not fit its header.
    pub(crate) fn new(path: &Path, file: File) -> Result<Self, MediaError> {
        let file_len = file
            .metadata()
            .map_err(|error| read_error(path, error))?
            .len();
        let mut reader = BufReader::new(file);
        let unfinished = || {
            invalid(
                path,
                "is unfinished: it ends before its end chunk, as an aborted encode leaves it",
            )
        };
        let fixed = exact(&mut reader, 16).map_err(|_| unfinished())?;
        if &fixed[..8] != MAGIC {
            return Err(invalid(path, "is not a test codec file"));
        }
        let version = u32::from_le_bytes(fixed[8..12].try_into().expect("four bytes"));
        if version != VERSION {
            return Err(invalid(
                path,
                format_args!("is version {version} of the test codec's format, not {VERSION}"),
            ));
        }
        let header_len = u32::from_le_bytes(fixed[12..16].try_into().expect("four bytes"));
        if 16 + u64::from(header_len) > file_len {
            return Err(unfinished());
        }
        let json = exact(&mut reader, header_len as usize).map_err(|_| unfinished())?;
        let header: Header = serde_json::from_slice(&json)
            .map_err(|error| invalid(path, format_args!("has a malformed header: {error}")))?;
        if let Some(video) = &header.video {
            checked_video(path, video)?;
        }
        if let Some(audio) = &header.audio {
            checked_audio(audio.sample_rate, audio.channels)?;
        }

        let mut position = 16 + u64::from(header_len);
        let mut frames = Vec::new();
        let mut audio = None;
        loop {
            let prefix = exact(&mut reader, 5).map_err(|_| unfinished())?;
            let tag = prefix[0];
            let len = u32::from_le_bytes(prefix[1..].try_into().expect("four bytes"));
            let offset = position + 5;
            position = offset + u64::from(len);
            if position > file_len {
                return Err(unfinished());
            }
            let chunk = Chunk { offset, len };
            match tag {
                VIDEO if header.video.is_some() => frames.push(chunk),
                AUDIO if header.audio.is_some() && audio.is_none() => audio = Some(chunk),
                END if len == 0 => break,
                _ => {
                    return Err(invalid(
                        path,
                        format_args!(
                            "has a {:?} chunk of {len} bytes that its header does not allow",
                            char::from(tag)
                        ),
                    ));
                }
            }
            reader
                .seek_relative(i64::from(len))
                .map_err(|error| read_error(path, error))?;
        }
        if position != file_len {
            return Err(invalid(path, "has data after its end chunk"));
        }
        if header.audio.is_some() && audio.is_none() {
            return Err(invalid(path, "has an audio header but no audio"));
        }
        Ok(Self {
            header,
            frames,
            audio,
            reader,
            path: path.to_owned(),
        })
    }

    /// A chunk's payload as it is stored, compressed.
    fn raw(&mut self, chunk: Chunk) -> Result<Vec<u8>, MediaError> {
        let mut bytes = vec![0; chunk.len as usize];
        self.reader
            .seek(SeekFrom::Start(chunk.offset))
            .and_then(|_| self.reader.read_exact(&mut bytes))
            .map_err(|error| read_error(&self.path, error))?;
        Ok(bytes)
    }

    fn decompressed(&mut self, chunk: Chunk) -> Result<Vec<u8>, MediaError> {
        let raw = self.raw(chunk)?;
        lz4_flex::block::decompress_size_prepended(&raw)
            .map_err(|error| invalid(&self.path, format_args!("has a corrupt chunk: {error}")))
    }

    /// Frame `index`, as the encoder was given it.
    pub(crate) fn frame(&mut self, index: usize) -> Result<Vec<u8>, MediaError> {
        let video = self.header.video.expect("frames come with a video header");
        let expected =
            frame_len(video.format, video.width, video.height).expect("the header was checked");
        let frame = self.decompressed(self.frames[index])?;
        if frame.len() != expected {
            return Err(invalid(
                &self.path,
                format_args!(
                    "frame {index} is {} bytes, not the {expected} of a {}x{} {:?} frame",
                    frame.len(),
                    video.width,
                    video.height,
                    video.format
                ),
            ));
        }
        Ok(frame)
    }

    /// The sample frames of the audio, the lead-in included, from the
    /// size its compressed payload starts with.
    pub(crate) fn audio_frames(&mut self) -> Result<Option<usize>, MediaError> {
        let (Some(header), Some(chunk)) = (self.header.audio, self.audio) else {
            return Ok(None);
        };
        let mut size = [0; 4];
        if chunk.len < 4 {
            return Err(invalid(&self.path, "has a corrupt audio chunk"));
        }
        self.reader
            .seek(SeekFrom::Start(chunk.offset))
            .and_then(|_| self.reader.read_exact(&mut size))
            .map_err(|error| read_error(&self.path, error))?;
        let bytes = u32::from_le_bytes(size) as usize;
        self.checked_frames(&header, bytes).map(Some)
    }

    fn checked_frames(&self, header: &AudioHeader, bytes: usize) -> Result<usize, MediaError> {
        let channels = usize::from(header.channels);
        if !bytes.is_multiple_of(4 * channels) {
            return Err(invalid(
                &self.path,
                format_args!(
                    "has {bytes} bytes of audio, not whole frames of {} f32 channels",
                    header.channels
                ),
            ));
        }
        let frames = bytes / (4 * channels);
        if header.lead_in_frames > frames {
            return Err(invalid(
                &self.path,
                format_args!(
                    "has {frames} sample frames, fewer than its lead-in of {}",
                    header.lead_in_frames
                ),
            ));
        }
        Ok(frames)
    }

    /// The audio, the lead-in included.
    pub(crate) fn audio(&mut self) -> Result<Option<StoredAudio>, MediaError> {
        let (Some(header), Some(chunk)) = (self.header.audio, self.audio) else {
            return Ok(None);
        };
        let bytes = self.decompressed(chunk)?;
        self.checked_frames(&header, bytes.len())?;
        Ok(Some(StoredAudio {
            sample_rate: header.sample_rate,
            channels: header.channels,
            lead_in_frames: header.lead_in_frames,
            samples: bytes_samples(&bytes),
        }))
    }
}

/// Reads a test codec file whole: its frames as they were pushed to the
/// encoder and its audio as it was muxed, to compare byte for byte.
pub fn read(path: &Path) -> Result<TestMedia, MediaError> {
    let mut scanned = match sniff(path)? {
        (Kind::Test, file) => Scanned::new(path, file)?,
        (Kind::Wav, _) => return Err(invalid(path, "is a WAV file, not a test codec file")),
    };
    let video = match scanned.header.video {
        Some(video) => Some(StoredVideo {
            width: video.width,
            height: video.height,
            frame_rate: video.frame_rate,
            format: video.format,
            frames: (0..scanned.frames.len())
                .map(|index| scanned.frame(index))
                .collect::<Result<_, _>>()?,
        }),
        None => None,
    };
    Ok(TestMedia {
        container: scanned.header.container,
        video,
        audio: scanned.audio()?,
    })
}
