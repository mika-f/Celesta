//! The test codec in this process: what it writes it reads back exactly,
//! and it refuses what the FFmpeg codec would.

use celesta_codec_test::{
    StoredAudio, StoredVideo, TestCodec, TestMedia, read, write_audio, write_rgba_video,
};
use celesta_composition::{Rational, Time};
use celesta_media::{
    CodecProvider, Container, EncoderId, EncoderSettings, MediaError, MuxRequest, PixelFormat,
    Sequential, VideoFrame, VideoRequest,
};
use std::path::{Path, PathBuf};

fn settings(
    output: &Path,
    width: u32,
    height: u32,
    input: PixelFormat,
    container: Container,
) -> EncoderSettings {
    EncoderSettings {
        output: output.to_path_buf(),
        width,
        height,
        frame_rate: Rational::new(25, 1),
        input,
        container,
        preset: "medium".into(),
        crf: 18,
        bit_depth: 8,
    }
}

/// `len` bytes that differ from frame to frame by `seed`.
fn frame(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn encode(
    codec: &mut TestCodec,
    settings: &EncoderSettings,
    frames: &[Vec<u8>],
) -> Result<(), MediaError> {
    let encoder = codec.open_encoder(settings)?;
    for frame in frames {
        codec.encode_frame(encoder, frame.clone())?;
    }
    codec.finish_encoder(encoder)
}

fn codec_message<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::Codec(message)) => message,
        other => panic!("expected a codec error, got {other:?}"),
    }
}

fn frame_at(codec: &mut TestCodec, path: &Path, time: f64) -> Result<VideoFrame, MediaError> {
    codec.decode_video(&VideoRequest {
        path,
        time,
        sequential: None,
    })
}

fn write_pcm(path: &Path, samples: &[f32]) {
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    std::fs::write(path, bytes).unwrap();
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn stores_every_pixel_format_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    for (format, len) in [
        (PixelFormat::Rgba8, 96),
        (PixelFormat::Rgba16, 192),
        (PixelFormat::Yuv420p, 36),
        (PixelFormat::Yuv420p10, 72),
    ] {
        let output = directory.path().join(format!("{format:?}.ctc"));
        let frames: Vec<_> = (0..3).map(|index| frame(len, index * 7 + 1)).collect();
        let mut codec = TestCodec::new();
        encode(
            &mut codec,
            &settings(&output, 6, 4, format, Container::Mov),
            &frames,
        )
        .unwrap();

        assert_eq!(
            read(&output).unwrap(),
            TestMedia {
                container: Some(Container::Mov),
                video: Some(StoredVideo {
                    width: 6,
                    height: 4,
                    frame_rate: Rational::new(25, 1),
                    format,
                    frames,
                }),
                audio: None,
            },
            "{format:?}"
        );
        let probe = codec.probe(&output).unwrap();
        let video = probe.video.expect("a video stream");
        assert_eq!((video.width, video.height), (6, 4));
        assert_eq!(video.frame_rate, Some(Rational::new(25, 1)));
        assert_eq!(video.codec.as_deref(), Some("celesta-test"));
        assert_eq!(video.duration, Some(Time::new(3, 25)));
        assert_eq!(probe.duration, Some(Time::new(3, 25)));
        assert!(probe.audio.is_empty());
    }
}

#[test]
fn yuv_frame_sizes_round_chroma_up() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("odd.ctc");
    let mut codec = TestCodec::new();
    let encoder = codec
        .open_encoder(&settings(
            &output,
            5,
            3,
            PixelFormat::Yuv420p,
            Container::Mp4,
        ))
        .unwrap();
    // 5x3 luma, and two 3x2 chroma planes.
    codec.encode_frame(encoder, vec![0; 27]).unwrap();
    let message = codec_message(codec.encode_frame(encoder, vec![0; 26]));
    assert!(message.contains("27"), "{message}");
    codec.finish_encoder(encoder).unwrap();
}

#[test]
fn refuses_what_it_cannot_write() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("refused.ctc");
    let mut codec = TestCodec::new();

    let unknown = EncoderId(99);
    for message in [
        codec_message(codec.encode_frame(unknown, vec![0; 4])),
        codec_message(codec.finish_encoder(unknown)),
    ] {
        assert!(message.contains("no open encoder"), "{message}");
    }

    let base = settings(&output, 2, 2, PixelFormat::Rgba8, Container::Mp4);
    let refused = [
        EncoderSettings {
            width: 0,
            ..base.clone()
        },
        EncoderSettings {
            frame_rate: Rational::new(0, 1),
            ..base.clone()
        },
        EncoderSettings {
            bit_depth: 12,
            ..base.clone()
        },
        EncoderSettings {
            crf: 52,
            ..base.clone()
        },
    ];
    for settings in refused {
        assert!(codec.open_encoder(&settings).is_err(), "{settings:?}");
    }
    // The bit depth and CRF are H.264's, as the FFmpeg codec takes them.
    let mov = EncoderSettings {
        container: Container::Mov,
        bit_depth: 12,
        crf: 52,
        ..base
    };
    encode(&mut codec, &mov, &[vec![0; 16]]).unwrap();

    for container in [Container::Mp4, Container::Mov, Container::Webm] {
        codec.check_encoder(container).unwrap();
    }
}

#[test]
fn an_aborted_encode_is_unfinished() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("aborted.ctc");
    let mut codec = TestCodec::new();
    let encoder = codec
        .open_encoder(&settings(&output, 2, 2, PixelFormat::Rgba8, Container::Mp4))
        .unwrap();
    codec.encode_frame(encoder, vec![7; 16]).unwrap();
    codec.abort_encoder(encoder);

    assert!(output.exists());
    let probed = codec.probe(&output).unwrap_err().to_string();
    assert!(probed.contains("unfinished"), "{probed}");
    let read = read(&output).unwrap_err().to_string();
    assert!(read.contains("unfinished"), "{read}");
}

#[test]
fn every_cut_of_a_file_and_an_extra_byte_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("video.ctc");
    let pcm = directory.path().join("audio.pcm");
    let muxed = directory.path().join("muxed.ctc");
    let mut codec = TestCodec::new();
    let frames: Vec<_> = (0..2).map(|index| frame(16, index)).collect();
    encode(
        &mut codec,
        &settings(&video, 2, 2, PixelFormat::Rgba8, Container::Mp4),
        &frames,
    )
    .unwrap();
    write_pcm(&pcm, &[0.25, -0.5, 0.75, -1.0]);
    codec
        .mux_audio(&MuxRequest {
            video,
            audio: pcm,
            sample_rate: 48_000,
            channels: 2,
            output: muxed.clone(),
            container: Container::Mp4,
            lead_in_frames: 1,
        })
        .unwrap();
    read(&muxed).unwrap();
    codec.probe(&muxed).unwrap();
    let bytes = std::fs::read(&muxed).unwrap();

    // The magic, `CLSTTEST`: a file cut within it is not a test codec file
    // at all; one cut anywhere after it is unfinished.
    let magic = b"CLSTTEST".len();
    for len in 0..bytes.len() {
        let cut = directory.path().join(format!("cut-{len}.ctc"));
        std::fs::write(&cut, &bytes[..len]).unwrap();
        let read = read(&cut).unwrap_err().to_string();
        let probed = codec.probe(&cut).unwrap_err().to_string();
        if len >= magic {
            assert!(read.contains("unfinished"), "{len} bytes: {read}");
            assert!(probed.contains("unfinished"), "{len} bytes: {probed}");
        }
    }

    let longer = directory.path().join("longer.ctc");
    std::fs::write(&longer, [&bytes[..], &[0]].concat()).unwrap();
    for message in [
        read(&longer).unwrap_err().to_string(),
        codec.probe(&longer).unwrap_err().to_string(),
    ] {
        assert!(message.contains("after its end chunk"), "{message}");
    }
}

#[test]
fn decodes_rgba_frames_at_their_times() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("two.ctc");
    let a = frame(8, 1);
    let b = frame(8, 2);
    write_rgba_video(&path, 2, 1, Rational::new(2, 1), &[a.clone(), b.clone()]).unwrap();
    let mut codec = TestCodec::new();

    for (time, expected) in [(0.0, &a), (0.49, &a), (0.5, &b), (60.0, &b)] {
        let decoded = frame_at(&mut codec, &path, time).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(*decoded.pixels, *expected, "at {time}");
    }
    let sequential = codec
        .decode_video(&VideoRequest {
            path: &path,
            time: 0.5,
            sequential: Some(Sequential {
                session: "layer",
                frame_rate: Rational::new(2, 1),
            }),
        })
        .unwrap();
    assert_eq!(*sequential.pixels, b);
    assert!(matches!(
        frame_at(&mut codec, &path, -0.1),
        Err(MediaError::InvalidTimestamp(time)) if time == -0.1
    ));
    assert!(matches!(
        frame_at(&mut codec, &path, f64::NAN),
        Err(MediaError::InvalidTimestamp(time)) if time.is_nan()
    ));

    let deep = directory.path().join("deep.ctc");
    let pixel: Vec<u8> = [65535u16, 32896, 0, 257]
        .iter()
        .flat_map(|channel| channel.to_le_bytes())
        .collect();
    encode(
        &mut codec,
        &settings(&deep, 1, 1, PixelFormat::Rgba16, Container::Mov),
        &[pixel],
    )
    .unwrap();
    assert_eq!(
        *frame_at(&mut codec, &deep, 0.0).unwrap().pixels,
        [255, 128, 0, 1]
    );

    let yuv = directory.path().join("yuv.ctc");
    encode(
        &mut codec,
        &settings(&yuv, 2, 2, PixelFormat::Yuv420p, Container::Mp4),
        &[vec![16; 6]],
    )
    .unwrap();
    codec_message(frame_at(&mut codec, &yuv, 0.0));

    let sound = directory.path().join("sound.ctc");
    write_audio(&sound, 48_000, 1, &[0.0; 4]).unwrap();
    assert!(matches!(
        frame_at(&mut codec, &sound, 0.0),
        Err(MediaError::NoVideoStream(path)) if path == sound
    ));
}

#[test]
fn muxes_audio_with_its_lead_in() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("video.ctc");
    let pcm = directory.path().join("audio.pcm");
    let output = directory.path().join("muxed.ctc");
    let mut codec = TestCodec::new();
    let frames: Vec<_> = (0..3).map(|index| frame(16, index)).collect();
    encode(
        &mut codec,
        &settings(&video, 2, 2, PixelFormat::Rgba8, Container::Mp4),
        &frames,
    )
    .unwrap();
    // Ten stereo frames, the first four of them the lead-in.
    let samples: Vec<f32> = (0..20).map(|index| index as f32 / 20.0 - 0.5).collect();
    write_pcm(&pcm, &samples);
    let request = MuxRequest {
        video: video.clone(),
        audio: pcm.clone(),
        sample_rate: 48_000,
        channels: 2,
        output: output.clone(),
        container: Container::Mp4,
        lead_in_frames: 4,
    };
    codec.mux_audio(&request).unwrap();

    let media = read(&output).unwrap();
    assert_eq!(media.container, Some(Container::Mp4));
    assert_eq!(media.video, read(&video).unwrap().video);
    let audio = media.audio.expect("muxed audio");
    assert_eq!(
        audio,
        StoredAudio {
            sample_rate: 48_000,
            channels: 2,
            lead_in_frames: 4,
            samples: samples.clone(),
        }
    );
    assert_eq!(audio.lead_in(), &samples[..8]);
    assert_eq!(audio.played(), &samples[8..]);

    let decoded = codec.decode_audio(&output, 48_000, 2).unwrap();
    assert_eq!((decoded.sample_rate, decoded.channels), (48_000, 2));
    assert_eq!(decoded.samples, &samples[8..]);

    let probe = codec.probe(&output).unwrap();
    assert_eq!(probe.audio.len(), 1);
    let stream = &probe.audio[0];
    assert_eq!(stream.codec.as_deref(), Some("pcm_f32le"));
    assert_eq!(stream.sample_rate, Some(48_000));
    assert_eq!(stream.channels, Some(2));
    assert_eq!(stream.duration, Some(Time::new(6, 48_000)));
    // The video's three frames outlast the audio's six samples.
    assert_eq!(probe.duration, Some(Time::new(3, 25)));

    // A file that already has audio takes no more.
    let again = MuxRequest {
        video: output.clone(),
        output: directory.path().join("again.ctc"),
        ..request.clone()
    };
    assert!(codec.mux_audio(&again).is_err());

    let mov = directory.path().join("video.mov.ctc");
    encode(
        &mut codec,
        &settings(&mov, 2, 2, PixelFormat::Rgba16, Container::Mov),
        &[vec![0; 32]],
    )
    .unwrap();
    let webm = MuxRequest {
        video: mov,
        output: directory.path().join("muxed.webm.ctc"),
        container: Container::Webm,
        ..request
    };
    assert!(codec.mux_audio(&webm).is_err());
}

#[test]
fn converts_channels_but_never_resamples() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();

    let mono = directory.path().join("mono.ctc");
    write_audio(&mono, 48_000, 1, &[0.5, -0.25]).unwrap();
    let stereo = codec.decode_audio(&mono, 48_000, 2).unwrap();
    assert_eq!((stereo.sample_rate, stereo.channels), (48_000, 2));
    assert_eq!(stereo.samples, [0.5, 0.5, -0.25, -0.25]);

    let both = directory.path().join("stereo.ctc");
    write_audio(&both, 48_000, 2, &[1.0, 0.0]).unwrap();
    let down = codec.decode_audio(&both, 48_000, 1).unwrap();
    assert_eq!(down.channels, 1);
    assert_eq!(down.samples, [0.5]);

    let message = codec_message(codec.decode_audio(&mono, 44_100, 1));
    assert!(
        message.contains("48000") && message.contains("44100"),
        "{message}"
    );

    let silent = directory.path().join("silent.ctc");
    write_rgba_video(&silent, 1, 1, Rational::new(30, 1), &[vec![0; 4]]).unwrap();
    let none = codec.decode_audio(&silent, 48_000, 2).unwrap();
    assert_eq!((none.sample_rate, none.channels), (48_000, 2));
    assert!(none.samples.is_empty());
}

#[test]
fn reads_the_checked_in_voices() {
    let assets = workspace_root().join("examples/assets");
    let voice = assets.join("voices/001.wav");
    let mut codec = TestCodec::new();

    let probe = codec.probe(&voice).unwrap();
    assert_eq!(probe.video, None);
    assert_eq!(probe.audio.len(), 1);
    let stream = &probe.audio[0];
    assert_eq!(stream.codec.as_deref(), Some("pcm_s16le"));
    assert_eq!(stream.sample_rate, Some(44_100));
    assert_eq!(stream.channels, Some(1));
    assert_eq!(stream.duration, Some(Time::new(84_022, 44_100)));
    assert_eq!(probe.duration, Some(Time::new(84_022, 44_100)));

    let decoded = codec.decode_audio(&voice, 44_100, 1).unwrap();
    assert_eq!(decoded.samples.len(), 84_022);
    assert_eq!(decoded.samples[..2], [-11.0 / 32768.0, -15.0 / 32768.0]);

    // Its fmt chunk is followed by an FLLR chunk before the data.
    let padded = codec
        .probe(&assets.join("voice-folder/shizuku/voice-1.wav"))
        .unwrap();
    assert_eq!(padded.audio.len(), 1);
    assert_eq!(padded.audio[0].sample_rate, Some(16_000));
    assert_eq!(padded.audio[0].channels, Some(1));
}

#[test]
fn names_files_it_cannot_read() {
    let directory = tempfile::tempdir().unwrap();
    let mut codec = TestCodec::new();

    let missing = directory.path().join("missing.wav");
    assert!(matches!(codec.probe(&missing), Err(MediaError::NotFound(path)) if path == missing));
    assert!(matches!(
        frame_at(&mut codec, &missing, 0.0),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    assert!(matches!(
        codec.decode_audio(&missing, 48_000, 2),
        Err(MediaError::NotFound(path)) if path == missing
    ));

    let random = directory.path().join("random.mp4");
    let mut state = 0x2545_f491_u32;
    let noise: Vec<u8> = (0..64)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    std::fs::write(&random, noise).unwrap();

    // A 32-bit float WAV: format tag 3.
    let float = directory.path().join("float.wav");
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&40u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&3u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&48_000u32.to_le_bytes());
    wav.extend_from_slice(&192_000u32.to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&32u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&4u32.to_le_bytes());
    wav.extend_from_slice(&0.5f32.to_le_bytes());
    std::fs::write(&float, wav).unwrap();

    for path in [&random, &float] {
        let name = path.display().to_string();
        for message in [
            codec.probe(path).unwrap_err().to_string(),
            frame_at(&mut codec, path, 0.0).unwrap_err().to_string(),
            codec.decode_audio(path, 48_000, 1).unwrap_err().to_string(),
        ] {
            assert!(message.contains(&name), "{message}");
        }
    }
}
