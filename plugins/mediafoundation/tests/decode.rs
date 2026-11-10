//! The SourceReader decoder, in this process: what it reads of files this
//! plugin's encoders and the FFmpeg plugin wrote (into the FFmpeg plugin's
//! muxers, the plugin having none), compared with the FFmpeg plugin's
//! decoder, which honours edit lists as this decoder must; what it refuses;
//! and that it leaves no file held open.
#![cfg(windows)]

use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_codec_mediafoundation::{DECODER_ID, MfCodec};
use celesta_composition::Rational;
use celesta_media::{
    AudioBuffer, AudioEncoderRequest, Color, ComponentKind, Encoded, MediaError, MediaProbe,
    MediaProvider, MuxerRequest, Packet, PixelFormat, Sequential, SettingValue, Settings, Track,
    TrackConfig, TrackPacket, VideoEncoderRequest, VideoFrame, VideoRequest,
};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;

/// The audio's rate, which `aac` takes as it is.
const AUDIO_RATE: u32 = 48_000;
/// The frame of the click in [`click`].
const CLICK: usize = 4_800;
/// The loop pre-roll the click file encodes before 0.
const PRE_ROLL: usize = 16_384;

/// The decoder's sequential-session constants (P5's): how many requests a
/// session may go unasked, times the sessions open, before it closes.
const IDLE_SESSION_FRAMES: usize = 30;

/// BT.709 limited-range red, green, blue, and white in 8 bits.
const COLORS: [[u8; 3]; 4] = [
    [63, 102, 240],
    [173, 42, 26],
    [32, 240, 118],
    [235, 128, 128],
];

/// Whether this system's Media Foundation describes the decoder and the
/// encoders the tests write with; prints why `test` skips when it does not.
fn available(test: &str) -> bool {
    let described = MfCodec::new().describe().unwrap();
    for (kind, id) in [
        (ComponentKind::Decoder, DECODER_ID),
        (ComponentKind::VideoEncoder, "h264"),
        (ComponentKind::AudioEncoder, "aac"),
    ] {
        let component = described
            .iter()
            .find(|component| component.kind == kind && component.id == id)
            .unwrap_or_else(|| panic!("{id} is not described"));
        if !component.available {
            println!(
                "skipping {test}: {}",
                component.reason.as_deref().unwrap_or("no reason")
            );
            return false;
        }
    }
    true
}

/// A 64x36 NV12 frame of one colour.
fn nv12_solid([y, cb, cr]: [u8; 3]) -> Vec<u8> {
    let luma = (WIDTH * HEIGHT) as usize;
    let mut frame = vec![y; luma];
    frame.extend([cb, cr].repeat(luma / 4));
    frame
}

/// Every packet of `answers`, and the config the first of them gave.
fn packets_and_config(answers: Vec<Encoded>) -> (Vec<Packet>, TrackConfig) {
    let mut packets = Vec::new();
    let mut config = None;
    for answer in answers {
        packets.extend(answer.packets);
        config = config.or(answer.config);
    }
    (packets, config.expect("the encoder gave its config"))
}

/// `frames` (of `input`) through `provider`'s video encoder `component` at
/// 64x36 and `rate`, in `timescale`, where a frame lasts `frame` ticks: the
/// packets and the track.
#[expect(clippy::too_many_arguments, reason = "a test helper")]
fn encode_video(
    provider: &mut dyn MediaProvider,
    component: &str,
    settings: &Settings,
    input: PixelFormat,
    rate: Rational,
    timescale: u32,
    frame: i64,
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> (Vec<Packet>, Track) {
    let encoder = provider.video_encoder().expect("video encoders");
    let opened = encoder
        .open_video(&VideoEncoderRequest {
            component,
            settings,
            width: WIDTH,
            height: HEIGHT,
            frame_rate: rate,
            timescale,
            offered: &[input],
            color: &Color::bt709_limited(),
        })
        .unwrap();
    assert_eq!(opened.input, input);
    let mut answers: Vec<Encoded> = frames
        .into_iter()
        .enumerate()
        .map(|(index, pixels)| {
            encoder
                .encode_video(opened.session, index as i64 * frame, pixels)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_video(opened.session).unwrap());
    let (packets, config) = packets_and_config(answers);
    let track = Track::Video {
        config,
        timescale,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: rate,
        color: Color::bt709_limited(),
        alpha: opened.alpha,
    };
    (packets, track)
}

/// `hardware: false`: Microsoft's software H.264 MFT, which takes 64x36
/// and reorders.
fn software() -> Settings {
    [("hardware".to_owned(), SettingValue::Boolean(false))].into()
}

/// `frames` of NV12 through this plugin's software `h264`.
fn h264(
    rate: Rational,
    timescale: u32,
    frame: i64,
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> (Vec<Packet>, Track) {
    encode_video(
        &mut MfCodec::new(),
        "h264",
        &software(),
        PixelFormat::Nv12,
        rate,
        timescale,
        frame,
        frames,
    )
}

/// Interleaved stereo `samples`, the first at `start`, through the
/// provider's audio encoder `component` in chunks of 1024 × 16 frames: the
/// packets and the track.
fn encode_audio(
    provider: &mut dyn MediaProvider,
    component: &str,
    start: i64,
    samples: &[f32],
) -> (Vec<Packet>, Track) {
    let encoder = provider.audio_encoder().expect("audio encoders");
    let opened = encoder
        .open_audio(&AudioEncoderRequest {
            component,
            settings: &Settings::new(),
            sample_rate: AUDIO_RATE,
            channels: 2,
        })
        .unwrap();
    assert_eq!((opened.sample_rate, opened.channels), (AUDIO_RATE, 2));
    let mut answers: Vec<Encoded> = samples
        .chunks(1024 * 16 * 2)
        .scan(start, |pts, chunk| {
            let answer = encoder.encode_audio(opened.session, *pts, chunk).unwrap();
            *pts += (chunk.len() / 2) as i64;
            Some(answer)
        })
        .collect();
    answers.push(encoder.finish_audio(opened.session).unwrap());
    let (packets, config) = packets_and_config(answers);
    let track = Track::Audio {
        config,
        timescale: AUDIO_RATE,
        sample_rate: AUDIO_RATE,
        channels: 2,
        delay: opened.delay,
    };
    (packets, track)
}

/// A second of stereo silence with a click (1.0) at [`CLICK`].
fn click() -> Vec<f32> {
    let mut samples = vec![0.0; AUDIO_RATE as usize * 2];
    samples[CLICK * 2] = 1.0;
    samples[CLICK * 2 + 1] = 1.0;
    samples
}

/// The packets of every track, in decode order across them; `tracks` gives
/// each one's timescale.
fn interleaved(tracks: &[(u32, &[Packet])]) -> Vec<TrackPacket> {
    let mut all: Vec<TrackPacket> = tracks
        .iter()
        .enumerate()
        .flat_map(|(track, (_, packets))| {
            packets.iter().map(move |packet| TrackPacket {
                track,
                packet: packet.clone(),
            })
        })
        .collect();
    all.sort_by(|left, right| {
        let seconds =
            |packet: &TrackPacket| packet.packet.dts as f64 / f64::from(tracks[packet.track].0);
        seconds(left).total_cmp(&seconds(right))
    });
    all
}

/// Writes `packets` of `tracks` into `output` with the FFmpeg plugin's
/// muxer `component`.
fn mux(
    component: &str,
    output: &Path,
    tracks: &[Track],
    leading_audio: u64,
    packets: &[TrackPacket],
) {
    let mut ffmpeg = FfmpegCodec::new();
    let muxer = ffmpeg.muxer().expect("muxers");
    let session = muxer
        .open_muxer(&MuxerRequest {
            component,
            settings: &Settings::new(),
            output,
            tracks,
            leading_audio,
        })
        .unwrap();
    muxer.write_packets(session, packets).unwrap();
    muxer.finish_muxer(session).unwrap();
}

/// The audio track of `samples` from `start` through this plugin's `aac`
/// (whose 1024-frame lead-in `ffmpeg/mp4` hides with an edit list), written
/// to `output` with `leading_audio` frames of pre-roll and the track's
/// `delay` replaced by `delay` when given.
fn aac_file(output: &Path, start: i64, samples: &[f32], leading_audio: u64, delay: Option<u64>) {
    let (audio, mut track) = encode_audio(&mut MfCodec::new(), "aac", start, samples);
    if let (Some(delay), Track::Audio { delay: given, .. }) = (delay, &mut track) {
        *given = delay;
    }
    let packets = interleaved(&[(AUDIO_RATE, &audio)]);
    mux("mp4", output, &[track], leading_audio, &packets);
}

/// The click file: this plugin's AAC of a loop pre-roll then [`click`],
/// whose lead-in and pre-roll the `mp4` muxer's edit list hides.
fn click_file(output: &Path) {
    let mut samples = vec![0.25; PRE_ROLL * 2];
    samples.extend(click());
    aac_file(output, -(PRE_ROLL as i64), &samples, PRE_ROLL as u64, None);
}

/// The luma of frame `index` of [`ffmpeg_mov`]: a step lighter each frame,
/// so a frame out of order decodes to the wrong grey.
fn grey(index: usize) -> u8 {
    40 + 15 * index as u8
}

/// An FFmpeg MOV at `output`: ten frames of libx264 (B-frames: its `elst`
/// skips two frames of reorder delay) at 29.97 fps, and FFmpeg's own AAC of
/// [`click`] (its `elst` skips 1024 frames of priming).
fn ffmpeg_mov(output: &Path) {
    let mut ffmpeg = FfmpegCodec::new();
    let frames = (0..10).map(|index| nv12_solid([grey(index), 128, 128]));
    let (video, video_track) = encode_video(
        &mut ffmpeg,
        "libx264",
        &Settings::new(),
        PixelFormat::Nv12,
        Rational::new(30_000, 1_001),
        30_000,
        1_001,
        frames,
    );
    let (audio, audio_track) = encode_audio(&mut ffmpeg, "aac", 0, &click());
    let packets = interleaved(&[(30_000, &video), (AUDIO_RATE, &audio)]);
    mux("mov", output, &[video_track, audio_track], 0, &packets);
}

/// A WebM of Opus, which only the FFmpeg plugin writes and reads, at
/// `output`.
fn webm(output: &Path) {
    let (audio, track) = encode_audio(&mut FfmpegCodec::new(), "opus", 0, &click());
    let packets = interleaved(&[(AUDIO_RATE, &audio)]);
    mux("webm", output, &[track], 0, &packets);
}

fn probe(path: &Path) -> Result<MediaProbe, MediaError> {
    MfCodec::new().decoder().unwrap().probe(DECODER_ID, path)
}

fn request(path: &Path, time: f64) -> VideoRequest<'_> {
    VideoRequest {
        path,
        time,
        sequential: None,
    }
}

/// The frame of `path` at `time` as the FFmpeg plugin decodes it.
fn ffmpeg_frame(path: &Path, time: f64) -> VideoFrame {
    FfmpegCodec::new()
        .decoder()
        .unwrap()
        .decode_video(celesta_codec_ffmpeg::DECODER_ID, &request(path, time))
        .unwrap()
}

/// `path`'s audio as the FFmpeg plugin decodes it.
fn ffmpeg_audio(path: &Path, sample_rate: u32, channels: u16) -> AudioBuffer {
    FfmpegCodec::new()
        .decoder()
        .unwrap()
        .decode_audio(
            celesta_codec_ffmpeg::DECODER_ID,
            path,
            sample_rate,
            channels,
        )
        .unwrap()
}

/// The colour of `frame`'s pixel at `(column, row)`.
fn rgb(frame: &VideoFrame, column: u32, row: u32) -> [u8; 3] {
    let start = ((row * frame.width + column) * 4) as usize;
    frame.pixels[start..start + 3].try_into().unwrap()
}

/// The largest difference of any channel of any pixel of `frame` from
/// `reference`'s; `None` when their sizes differ.
fn difference(frame: &VideoFrame, reference: &VideoFrame) -> Option<u8> {
    ((frame.width, frame.height) == (reference.width, reference.height)).then(|| {
        frame
            .pixels
            .iter()
            .zip(reference.pixels.iter())
            .map(|(actual, expected)| actual.abs_diff(*expected))
            .max()
            .unwrap_or(0)
    })
}

/// Whether `frame` is `reference`'s size and every channel of every pixel
/// is within 3 of `reference`'s.
fn is_like(frame: &VideoFrame, reference: &VideoFrame) -> bool {
    difference(frame, reference).is_some_and(|most| most <= 3)
}

/// The frame of the loudest sample of `samples`' first channel of
/// `channels`.
fn peak(samples: &[f32], channels: usize) -> usize {
    samples
        .iter()
        .step_by(channels)
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .map(|(frame, _)| frame)
        .unwrap()
}

#[test]
fn probes_what_it_wrote() {
    if !available("probes_what_it_wrote") {
        return;
    }
    const RATE: Rational = Rational::new(30_000, 1_001);
    let (video, video_track) = h264(RATE, 30_000, 1_001, (0..10).map(|_| nv12_solid(COLORS[0])));
    let (audio, audio_track) = encode_audio(&mut MfCodec::new(), "aac", 0, &click());
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.mp4");
    let packets = interleaved(&[(30_000, &video), (AUDIO_RATE, &audio)]);
    mux("mp4", &output, &[video_track, audio_track], 0, &packets);

    let probe = probe(&output).unwrap();
    println!("probe: {probe:?}");
    let stream = probe.video.expect("a video stream");
    assert_eq!(stream.codec.as_deref(), Some("h264"));
    assert_eq!((stream.width, stream.height), (WIDTH, HEIGHT));
    assert_eq!(stream.frame_rate, Some(RATE));
    assert_eq!(stream.color.primaries.as_deref(), Some("bt709"));
    assert_eq!(stream.color.transfer.as_deref(), Some("bt709"));
    assert_eq!(stream.color.matrix.as_deref(), Some("bt709"));
    let seconds = probe.duration.expect("a duration").as_seconds().unwrap();
    assert!((seconds - 1.0).abs() < 0.05, "{seconds} s");
    assert_eq!(probe.audio.len(), 1);
    let audio = &probe.audio[0];
    assert_eq!(audio.codec.as_deref(), Some("aac"));
    assert_eq!(audio.sample_rate, Some(AUDIO_RATE));
    assert_eq!(audio.channels, Some(2));
    assert_ne!(audio.index, stream.index);
}

#[test]
fn decodes_the_frame_shown_at_each_time() {
    if !available("decodes_the_frame_shown_at_each_time") {
        return;
    }
    // Four frames of their own colours at 10 fps; the software MFT
    // reorders, so `ffmpeg/mp4` writes an edit list the reader ignores.
    let (video, track) = h264(Rational::new(10, 1), 10, 1, COLORS.map(nv12_solid));
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("colors.mp4");
    let packets = interleaved(&[(10, &video)]);
    mux("mp4", &output, &[track], 0, &packets);

    // Each frame as the FFmpeg plugin decodes it, at its start.
    let references: Vec<VideoFrame> = (0..COLORS.len())
        .map(|index| ffmpeg_frame(&output, index as f64 / 10.0))
        .collect();
    for pair in references.windows(2) {
        assert!(!is_like(&pair[0], &pair[1]), "two frames look alike");
    }

    let shown = [(0.0, 0), (0.15, 1), (0.299, 2), (0.3, 3), (5.0, 3)];
    let mut codec = MfCodec::new();
    let decoder = codec.decoder().unwrap();
    for (time, index) in shown {
        let frame = decoder
            .decode_video(DECODER_ID, &request(&output, time))
            .unwrap();
        assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
        println!(
            "{time} s: {:?}, frame {index} {:?}, within {:?}",
            rgb(&frame, 0, 0),
            rgb(&references[index], 0, 0),
            difference(&frame, &references[index])
        );
        assert!(
            is_like(&frame, &references[index]),
            "{time} s: {:?}, not frame {index}'s {:?}",
            rgb(&frame, 0, 0),
            rgb(&references[index], 0, 0)
        );
    }

    // The same through a session moving forward, then back to the start.
    let sequential = Some(Sequential {
        session: "layer",
        frame_rate: Rational::new(10, 1),
    });
    for (time, index) in shown.into_iter().chain([(0.0, 0), (0.1, 1)]) {
        let frame = decoder
            .decode_video(
                DECODER_ID,
                &VideoRequest {
                    path: &output,
                    time,
                    sequential,
                },
            )
            .unwrap();
        assert!(
            is_like(&frame, &references[index]),
            "{time} s in a session: {:?}, not frame {index}'s {:?}",
            rgb(&frame, 0, 0),
            rgb(&references[index], 0, 0)
        );
    }

    for time in [-0.1, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            decoder.decode_video(DECODER_ID, &request(&output, time)),
            Err(MediaError::InvalidTimestamp(_))
        ));
    }
}

#[test]
fn decodes_audio_as_ffmpeg_does() {
    if !available("decodes_audio_as_ffmpeg_does") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("click.mp4");
    click_file(&output);

    let reference = ffmpeg_audio(&output, AUDIO_RATE, 2);
    let mut codec = MfCodec::new();
    let decoder = codec.decoder().unwrap();
    let decoded = decoder
        .decode_audio(DECODER_ID, &output, AUDIO_RATE, 2)
        .unwrap();
    println!(
        "48000 Hz stereo: {} frames, the click at {} (FFmpeg: {} frames, the click at {})",
        decoded.frame_count(),
        peak(&decoded.samples, 2),
        reference.frame_count(),
        peak(&reference.samples, 2)
    );
    assert_eq!((decoded.sample_rate, decoded.channels), (AUDIO_RATE, 2));
    assert_eq!(peak(&reference.samples, 2), CLICK);
    assert_eq!(peak(&decoded.samples, 2), CLICK);
    // The edit's duration caps the track as FFmpeg's decoder does.
    assert_eq!(reference.frame_count(), AUDIO_RATE as usize);
    assert!(
        decoded.frame_count().abs_diff(reference.frame_count()) <= 1,
        "{} frames, not FFmpeg's {}",
        decoded.frame_count(),
        reference.frame_count()
    );
    // The pre-roll stays hidden.
    let early = decoded.samples[..4_000 * 2]
        .iter()
        .fold(0.0_f32, |most, sample| most.max(sample.abs()));
    assert!(early < 0.1, "the first 4000 frames reach {early}");

    let mono = decoder
        .decode_audio(DECODER_ID, &output, 44_100, 1)
        .unwrap();
    let reference = ffmpeg_audio(&output, 44_100, 1);
    assert_eq!((mono.sample_rate, mono.channels), (44_100, 1));
    let expected = (CLICK as f64 * 44_100.0 / f64::from(AUDIO_RATE)).round() as usize;
    let found = peak(&mono.samples, 1);
    println!(
        "44100 Hz mono: {} frames, the click at {found} (FFmpeg: {} frames)",
        mono.frame_count(),
        reference.frame_count()
    );
    assert!(found.abs_diff(expected) <= 1, "{found}, not {expected}");
    assert!(
        mono.frame_count().abs_diff(reference.frame_count()) <= 1,
        "{} frames, not FFmpeg's {}",
        mono.frame_count(),
        reference.frame_count()
    );

    for (rate, channels) in [(0, 2), (48_000, 0), (48_000, 3)] {
        assert!(matches!(
            decoder.decode_audio(DECODER_ID, &output, rate, channels),
            Err(MediaError::InvalidAudioFormat { .. })
        ));
    }
}

#[test]
fn reads_mov_and_m4a() {
    if !available("reads_mov_and_m4a") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mov = directory.path().join("clip.mov");
    ffmpeg_mov(&mov);
    let probe_mov = probe(&mov).unwrap();
    println!("MOV probe: {probe_mov:?}");
    let stream = probe_mov.video.expect("a video stream");
    assert_eq!(stream.codec.as_deref(), Some("h264"));
    assert_eq!((stream.width, stream.height), (WIDTH, HEIGHT));
    assert_eq!(probe_mov.audio.len(), 1);
    assert_eq!(probe_mov.audio[0].codec.as_deref(), Some("aac"));
    let mut codec = MfCodec::new();
    let decoder = codec.decoder().unwrap();
    // Frame 6, at its start.
    let time = 6.0 * 1_001.0 / 30_000.0;
    let frame = decoder
        .decode_video(DECODER_ID, &request(&mov, time))
        .unwrap();
    let reference = ffmpeg_frame(&mov, time);
    assert!(
        is_like(&frame, &reference),
        "{time} s: {:?}, not FFmpeg's {:?}",
        rgb(&frame, 0, 0),
        rgb(&reference, 0, 0)
    );
    let audio = decoder
        .decode_audio(DECODER_ID, &mov, AUDIO_RATE, 2)
        .unwrap();
    assert_eq!(peak(&audio.samples, 2), CLICK);

    let m4a = directory.path().join("sound.m4a");
    aac_file(&m4a, 0, &click(), 0, None);
    let probe_m4a = probe(&m4a).unwrap();
    assert_eq!(probe_m4a.video, None);
    assert_eq!(probe_m4a.audio.len(), 1);
    assert!(matches!(
        decoder.decode_video(DECODER_ID, &request(&m4a, 0.0)),
        Err(MediaError::NoVideoStream(path)) if path == m4a
    ));
    let decoded = decoder
        .decode_audio(DECODER_ID, &m4a, AUDIO_RATE, 2)
        .unwrap();
    assert_eq!(peak(&decoded.samples, 2), CLICK);
}

#[test]
fn refuses_what_it_cannot_read() {
    if !available("refuses_what_it_cannot_read") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let unsupported = |result: Result<MediaProbe, MediaError>, path: &Path| match result {
        Err(MediaError::Unsupported { path: refused, .. }) => assert_eq!(refused, path),
        other => panic!("{}: expected unsupported, got {other:?}", path.display()),
    };
    let mut codec = MfCodec::new();
    let decoder = codec.decoder().unwrap();
    // A WebM, unless this Windows has a WebM source (this one's reads Opus
    // in WebM): then it decodes.
    let webm_file = directory.path().join("sound.webm");
    webm(&webm_file);
    match probe(&webm_file) {
        Ok(probe) => {
            let decoded = decoder
                .decode_audio(DECODER_ID, &webm_file, AUDIO_RATE, 2)
                .unwrap();
            println!(
                "this Windows reads WebM: {probe:?}; {} frames, the click at {}",
                decoded.frame_count(),
                peak(&decoded.samples, 2)
            );
            assert_eq!(probe.audio.len(), 1);
            // Its length is not FFmpeg's (48648 frames, not 48000: Media
            // Foundation applies neither Opus's pre-skip nor its trim), so
            // only the click is checked.
            assert_eq!(peak(&decoded.samples, 2), CLICK);
        }
        refused => unsupported(refused, &webm_file),
    }
    let text = directory.path().join("x.mp4");
    std::fs::write(&text, "not a movie\n").unwrap();
    unsupported(probe(&text), &text);
    assert!(matches!(
        decoder.decode_video(DECODER_ID, &request(&text, 0.0)),
        Err(MediaError::Unsupported { .. })
    ));
    assert!(matches!(
        decoder.decode_audio(DECODER_ID, &text, AUDIO_RATE, 2),
        Err(MediaError::Unsupported { .. })
    ));

    let missing = directory.path().join("missing.mp4");
    assert!(matches!(
        decoder.probe(DECODER_ID, &missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    assert!(matches!(
        decoder.decode_video(DECODER_ID, &request(&missing, 0.0)),
        Err(MediaError::NotFound(_))
    ));
    assert!(matches!(
        decoder.decode_audio(DECODER_ID, &missing, AUDIO_RATE, 2),
        Err(MediaError::NotFound(_))
    ));
    assert!(matches!(
        decoder.probe("ffmpeg", &text),
        Err(MediaError::Codec(message)) if message == r#"no decoder "ffmpeg" in mediafoundation"#
    ));
}

/// The quadrants of the rotated test's source: top left, top right,
/// bottom left, bottom right, as RGB.
const QUADRANTS: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];

/// A 64x36 RGBA frame of [`QUADRANTS`].
fn quadrants() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in 0..HEIGHT {
        for column in 0..WIDTH {
            let quadrant = usize::from(row >= HEIGHT / 2) * 2 + usize::from(column >= WIDTH / 2);
            let [r, g, b] = QUADRANTS[quadrant];
            pixels.extend([r, g, b, 255]);
        }
    }
    pixels
}

/// Rewrites the identity matrix in the track header (`tkhd`) of `path` to
/// the display matrix a portrait phone recording carries: show the coded
/// frame turned a quarter clockwise. It is what `ffmpeg -display_rotation`
/// changes (the FFmpeg plugin's `tag_portrait_rotation`).
fn tag_portrait_rotation(path: &Path) {
    const IDENTITY: [u32; 9] = [0x1_0000, 0, 0, 0, 0x1_0000, 0, 0, 0, 0x4000_0000];
    const PORTRAIT: [u32; 9] = [0, 0x1_0000, 0, 0xFFFF_0000, 0, 0, 0, 0, 0x4000_0000];
    let encode = |matrix: [u32; 9]| -> Vec<u8> {
        matrix
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    };
    let identity = encode(IDENTITY);
    let mut bytes = std::fs::read(path).unwrap();
    let tkhd = bytes
        .windows(4)
        .position(|window| window == b"tkhd")
        .expect("a track header");
    let offset = tkhd
        + bytes[tkhd..]
            .windows(identity.len())
            .position(|window| window == identity)
            .expect("the track header has an identity matrix");
    bytes[offset..offset + identity.len()].copy_from_slice(&encode(PORTRAIT));
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn rotated_video_is_displayed_upright() {
    if !available("rotated_video_is_displayed_upright") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output: PathBuf = directory.path().join("portrait.mp4");
    let (video, track) = encode_video(
        &mut MfCodec::new(),
        "h264",
        &software(),
        PixelFormat::Rgba8,
        Rational::new(10, 1),
        10,
        1,
        (0..10).map(|_| quadrants()),
    );
    mux("mp4", &output, &[track], 0, &interleaved(&[(10, &video)]));
    tag_portrait_rotation(&output);

    let probe = probe(&output).unwrap();
    let stream = probe.video.expect("a video stream");
    assert_eq!((stream.width, stream.height), (HEIGHT, WIDTH));
    let frame = MfCodec::new()
        .decoder()
        .unwrap()
        .decode_video(DECODER_ID, &request(&output, 0.0))
        .unwrap();
    assert_eq!((frame.width, frame.height), (HEIGHT, WIDTH));
    assert_eq!(frame.pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    let reference = ffmpeg_frame(&output, 0.0);
    assert_eq!((reference.width, reference.height), (HEIGHT, WIDTH));
    // Turned a quarter clockwise, the source's bottom left is the top left,
    // its top left the top right, and its bottom right the bottom left.
    // Each corner is judged by the quadrant colour it is nearest (P5's
    // rule): how close it comes depends on the encoder, and the four
    // colours are far apart.
    let nearest = |actual: [u8; 3]| {
        (0..QUADRANTS.len())
            .min_by_key(|&quadrant| {
                actual
                    .iter()
                    .zip(QUADRANTS[quadrant])
                    .map(|(&actual, expected)| u32::from(actual.abs_diff(expected)).pow(2))
                    .sum::<u32>()
            })
            .expect("four quadrants")
    };
    for ((column, row), quadrant) in [
        ((2, 2), 2),
        ((HEIGHT - 3, 2), 0),
        ((2, WIDTH - 3), 3),
        ((HEIGHT - 3, WIDTH - 3), 1),
    ] {
        let actual = rgb(&frame, column, row);
        println!(
            "({column}, {row}): {actual:?}, FFmpeg's {:?}",
            rgb(&reference, column, row)
        );
        assert_eq!(
            nearest(actual),
            quadrant,
            "({column}, {row}): {actual:?} is nearest {:?}, not {:?}",
            QUADRANTS[nearest(actual)],
            QUADRANTS[quadrant]
        );
        assert_eq!(nearest(rgb(&reference, column, row)), quadrant);
    }
}

/// Whether `path` opens for writing, without truncating it: Media
/// Foundation's reader shares a file for reading and deleting only, so this
/// fails (os error 32) while a reader holds it.
fn opens_for_writing(path: &Path) -> bool {
    match OpenOptions::new().write(true).open(path) {
        Ok(_) => true,
        Err(error) if error.raw_os_error() == Some(32) => false,
        Err(error) => panic!("{}: {error}", path.display()),
    }
}

#[test]
fn probing_does_not_lock_the_file() {
    if !available("probing_does_not_lock_the_file") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("colors.mp4");
    let (video, track) = h264(Rational::new(10, 1), 10, 1, COLORS.map(nv12_solid));
    mux("mp4", &output, &[track], 0, &interleaved(&[(10, &video)]));
    let other = directory.path().join("other.mp4");
    std::fs::copy(&output, &other).unwrap();
    let size = std::fs::metadata(&output).unwrap().len();

    let mut codec = MfCodec::new();
    let decoder = codec.decoder().unwrap();
    decoder.probe(DECODER_ID, &output).unwrap();
    assert!(opens_for_writing(&output), "held after a probe");
    decoder
        .decode_video(DECODER_ID, &request(&output, 0.15))
        .unwrap();
    assert!(opens_for_writing(&output), "held after a decode");
    decoder
        .decode_audio(DECODER_ID, &output, AUDIO_RATE, 2)
        .unwrap();
    assert!(opens_for_writing(&output), "held after an audio decode");

    // A sequential session keeps its reader, until it goes unasked.
    let sequential = |session| {
        Some(Sequential {
            session,
            frame_rate: Rational::new(10, 1),
        })
    };
    decoder
        .decode_video(
            DECODER_ID,
            &VideoRequest {
                path: &output,
                time: 0.0,
                sequential: sequential("first"),
            },
        )
        .unwrap();
    assert!(
        !opens_for_writing(&output),
        "a sequential session's reader is not held, so this test cannot tell"
    );
    // With two sessions open, one closes after twice IDLE_SESSION_FRAMES
    // requests to the other.
    let mut requests = 0;
    while !opens_for_writing(&output) {
        requests += 1;
        assert!(
            requests <= 2 * IDLE_SESSION_FRAMES + 1,
            "still held after {requests} requests to another file"
        );
        decoder
            .decode_video(
                DECODER_ID,
                &VideoRequest {
                    path: &other,
                    time: 0.0,
                    sequential: sequential("second"),
                },
            )
            .unwrap();
    }
    println!("released after {requests} requests to another file");
    // P5's rule: a session closes once it is unasked for longer than
    // IDLE_SESSION_FRAMES times the sessions open, here two.
    assert_eq!(requests, 2 * IDLE_SESSION_FRAMES + 1);
    assert_eq!(std::fs::metadata(&output).unwrap().len(), size);
}

#[test]
fn edit_lists_are_honoured() {
    if !available("edit_lists_are_honoured") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut codec = MfCodec::new();
    let decoder = codec.decoder().unwrap();

    // FFmpeg's own AAC in an `.m4a` (`elst` 1024): the reader alone gives
    // the click at 5824.
    let m4a = directory.path().join("ffmpeg.m4a");
    let (audio, track) = encode_audio(&mut FfmpegCodec::new(), "aac", 0, &click());
    mux(
        "mp4",
        &m4a,
        &[track],
        0,
        &interleaved(&[(AUDIO_RATE, &audio)]),
    );
    let decoded = decoder
        .decode_audio(DECODER_ID, &m4a, AUDIO_RATE, 2)
        .unwrap();
    let reference = ffmpeg_audio(&m4a, AUDIO_RATE, 2);
    println!(
        "FFmpeg's AAC: the click at {} in {} frames (FFmpeg: {} in {})",
        peak(&decoded.samples, 2),
        decoded.frame_count(),
        peak(&reference.samples, 2),
        reference.frame_count()
    );
    assert_eq!(peak(&reference.samples, 2), CLICK);
    assert_eq!(peak(&decoded.samples, 2), CLICK);
    assert!(decoded.frame_count().abs_diff(reference.frame_count()) <= 1);

    // An FFmpeg MOV of libx264 with B-frames (`elst` 2002): the reader alone
    // starts two frames late.
    let mov = directory.path().join("x264.mov");
    ffmpeg_mov(&mov);
    let references: Vec<VideoFrame> = (0..3)
        .map(|index| ffmpeg_frame(&mov, f64::from(index) * 1_001.0 / 30_000.0))
        .collect();
    assert!(!is_like(&references[0], &references[2]));
    let first = decoder
        .decode_video(DECODER_ID, &request(&mov, 0.0))
        .unwrap();
    println!(
        "x264 MOV at 0: {:?}, FFmpeg's {:?}",
        rgb(&first, 0, 0),
        rgb(&references[0], 0, 0)
    );
    assert!(
        is_like(&first, &references[0]),
        "0 s: {:?}, not FFmpeg's {:?}",
        rgb(&first, 0, 0),
        rgb(&references[0], 0, 0)
    );
    // One frame in: unmapped, the reader's first frame (at 2002) would still
    // be the one shown, so this needs the edit applied.
    assert!(!is_like(&references[0], &references[1]));
    let second = decoder
        .decode_video(DECODER_ID, &request(&mov, 1_001.0 / 30_000.0))
        .unwrap();
    println!(
        "x264 MOV one frame in: {:?}, FFmpeg's {:?}",
        rgb(&second, 0, 0),
        rgb(&references[1], 0, 0)
    );
    assert!(
        is_like(&second, &references[1]),
        "one frame in: {:?}, not FFmpeg's {:?}",
        rgb(&second, 0, 0),
        rgb(&references[1], 0, 0)
    );

    // This plugin's AAC, its packets moved to start at 0, through
    // `ffmpeg/mp4` with `delay: 0`, whose `elst` then has media time 0: read
    // as it is, the lead-in kept, as FFmpeg reads it.
    let plain = directory.path().join("delay0.mp4");
    aac_file(&plain, 1_024, &click(), 0, Some(0));
    let decoded = decoder
        .decode_audio(DECODER_ID, &plain, AUDIO_RATE, 2)
        .unwrap();
    let reference = ffmpeg_audio(&plain, AUDIO_RATE, 2);
    println!(
        "delay 0: the click at {} in {} frames (FFmpeg: {} in {})",
        peak(&decoded.samples, 2),
        decoded.frame_count(),
        peak(&reference.samples, 2),
        reference.frame_count()
    );
    assert_eq!(peak(&reference.samples, 2), CLICK + 1_024);
    assert_eq!(peak(&decoded.samples, 2), peak(&reference.samples, 2));
    assert!(decoded.frame_count().abs_diff(reference.frame_count()) <= 1);
}
