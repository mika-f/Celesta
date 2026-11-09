//! The `AVAssetReader` decoder, in this process: what it reads of files this
//! plugin's encoders and muxers wrote (and of a rotated one `AVAssetWriter`
//! wrote here), compared with the FFmpeg plugin's decoder where both read
//! them, and what it refuses.
#![cfg(target_os = "macos")]

use block2::RcBlock;
use celesta_codec_avfoundation::{AvfCodec, DECODER_ID};
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoderRequest, Color, Encoded, MediaError, MediaProbe, MediaProvider, MuxerRequest,
    Packet, PixelFormat, Sequential, Settings, Track, TrackConfig, TrackPacket,
    VideoEncoderRequest, VideoFrame, VideoRequest,
};
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeVideo, AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoHeightKey,
    AVVideoWidthKey,
};
use objc2_core_foundation::{CFRetained, CGAffineTransform};
use objc2_core_media::{CMTime, CMTimeFlags, kCMTimeZero};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::path::{Path, PathBuf};
use std::ptr::{NonNull, null_mut};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;

/// The audio's rate, which `aac` takes as it is.
const AUDIO_RATE: u32 = 48_000;
/// The frame of the click in [`click`].
const CLICK: usize = 4_800;
/// The loop pre-roll the click file encodes before 0.
const PRE_ROLL: usize = 16_384;

/// BT.709 limited-range red, green, blue, and white in 8 bits.
const COLORS: [[u8; 3]; 4] = [
    [63, 102, 240],
    [173, 42, 26],
    [32, 240, 118],
    [235, 128, 128],
];

/// A 64x36 NV12 frame of one colour.
fn nv12_solid([y, cb, cr]: [u8; 3]) -> Vec<u8> {
    let luma = (WIDTH * HEIGHT) as usize;
    let mut frame = vec![y; luma];
    frame.extend([cb, cr].repeat(luma / 4));
    frame
}

/// Every packet of `answers`, and the config of the first with packets.
fn packets_and_config(answers: Vec<Encoded>) -> (Vec<Packet>, TrackConfig) {
    let config = answers
        .iter()
        .find(|answer| !answer.packets.is_empty())
        .expect("the encoder made packets")
        .config
        .clone()
        .expect("the first packets come with the config");
    let packets = answers
        .into_iter()
        .flat_map(|answer| answer.packets)
        .collect();
    (packets, config)
}

/// `frames` through this plugin's `h264` at 64x36 and `rate`, in the
/// timescale of `rate`'s numerator, `timescale`, where a frame lasts
/// `frame` ticks: the packets and the track.
fn h264(
    rate: Rational,
    timescale: u32,
    frame: i64,
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> (Vec<Packet>, Track) {
    let mut codec = AvfCodec::default();
    let encoder = codec.video_encoder().expect("video encoders");
    let opened = encoder
        .open_video(&VideoEncoderRequest {
            component: "h264",
            settings: &Settings::new(),
            width: WIDTH,
            height: HEIGHT,
            frame_rate: rate,
            timescale,
            offered: &[PixelFormat::Nv12],
            color: &Color::bt709_limited(),
        })
        .unwrap();
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

/// Writes `packets` of `tracks` into `output` with `provider`'s muxer
/// `component`.
fn mux(
    provider: &mut dyn MediaProvider,
    component: &str,
    output: &Path,
    tracks: &[Track],
    leading_audio: u64,
    packets: &[TrackPacket],
) {
    let muxer = provider.muxer().expect("muxers");
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

/// The click file of the muxer tests: AAC whose encoder delay and loop
/// pre-roll the `mp4` muxer hides, at `output`.
fn click_file(output: &Path) {
    let mut samples = vec![0.25; PRE_ROLL * 2];
    samples.extend(click());
    let mut codec = AvfCodec::default();
    let (audio, track) = encode_audio(&mut codec, "aac", -(PRE_ROLL as i64), &samples);
    let packets = interleaved(&[(AUDIO_RATE, &audio)]);
    mux(
        &mut codec,
        "mp4",
        output,
        &[track],
        PRE_ROLL as u64,
        &packets,
    );
}

/// A WebM of Opus, which only the FFmpeg plugin writes and reads, at
/// `output`.
fn webm(output: &Path) {
    let mut ffmpeg = FfmpegCodec::new();
    let (audio, track) = encode_audio(&mut ffmpeg, "opus", 0, &click());
    let packets = interleaved(&[(AUDIO_RATE, &audio)]);
    mux(&mut ffmpeg, "webm", output, &[track], 0, &packets);
}

fn probe(path: &Path) -> Result<MediaProbe, MediaError> {
    AvfCodec::default()
        .decoder()
        .unwrap()
        .probe(DECODER_ID, path)
}

fn request(path: &Path, time: f64) -> VideoRequest<'_> {
    VideoRequest {
        path,
        time,
        sequential: None,
    }
}

/// The colour of `frame`'s pixel at `(column, row)`.
fn rgb(frame: &VideoFrame, column: u32, row: u32) -> [u8; 3] {
    let start = ((row * frame.width + column) * 4) as usize;
    frame.pixels[start..start + 3].try_into().unwrap()
}

/// Whether `frame` is `reference`'s size and every channel of every pixel
/// is within 3 of `reference`'s.
fn is_like(frame: &VideoFrame, reference: &VideoFrame) -> bool {
    (frame.width, frame.height) == (reference.width, reference.height)
        && frame
            .pixels
            .iter()
            .zip(reference.pixels.iter())
            .all(|(actual, expected)| actual.abs_diff(*expected) <= 3)
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
    const RATE: Rational = Rational::new(30_000, 1_001);
    let (video, video_track) = h264(RATE, 30_000, 1_001, (0..10).map(|_| nv12_solid(COLORS[0])));
    let mut codec = AvfCodec::default();
    let (audio, audio_track) = encode_audio(&mut codec, "aac", 0, &click());
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.mp4");
    let packets = interleaved(&[(30_000, &video), (AUDIO_RATE, &audio)]);
    mux(
        &mut codec,
        "mp4",
        &output,
        &[video_track, audio_track],
        0,
        &packets,
    );

    let probe = probe(&output).unwrap();
    let stream = probe.video.expect("a video stream");
    assert_eq!(stream.codec.as_deref(), Some("h264"));
    assert_eq!((stream.width, stream.height), (WIDTH, HEIGHT));
    assert_eq!(stream.frame_rate, Some(RATE));
    let seconds = stream.duration.expect("a duration").as_seconds().unwrap();
    assert!(
        (seconds - 10.0 * 1_001.0 / 30_000.0).abs() < 1e-6,
        "{seconds} s"
    );
    assert_eq!(stream.color.primaries.as_deref(), Some("bt709"));
    assert_eq!(stream.color.transfer.as_deref(), Some("bt709"));
    assert_eq!(stream.color.matrix.as_deref(), Some("bt709"));
    assert!(probe.duration.is_some());
    assert_eq!(probe.audio.len(), 1);
    let audio = &probe.audio[0];
    assert_eq!(audio.codec.as_deref(), Some("aac"));
    assert_eq!(audio.sample_rate, Some(AUDIO_RATE));
    assert_eq!(audio.channels, Some(2));
    assert_ne!(audio.index, stream.index);
}

#[test]
fn decodes_the_frame_shown_at_each_time() {
    // Four frames of their own colours at 10 fps.
    let (video, track) = h264(Rational::new(10, 1), 10, 1, COLORS.map(nv12_solid));
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("colors.mp4");
    let packets = interleaved(&[(10, &video)]);
    mux(
        &mut AvfCodec::default(),
        "mp4",
        &output,
        &[track],
        0,
        &packets,
    );

    // Each frame as the FFmpeg plugin decodes it, at its start.
    let mut ffmpeg = FfmpegCodec::new();
    let references: Vec<VideoFrame> = (0..COLORS.len())
        .map(|index| {
            ffmpeg
                .decoder()
                .unwrap()
                .decode_video(
                    celesta_codec_ffmpeg::DECODER_ID,
                    &request(&output, index as f64 / 10.0),
                )
                .unwrap()
        })
        .collect();
    for pair in references.windows(2) {
        assert!(!is_like(&pair[0], &pair[1]), "two frames look alike");
    }

    let shown = [(0.0, 0), (0.15, 1), (0.299, 2), (0.3, 3), (5.0, 3)];
    let mut codec = AvfCodec::default();
    let decoder = codec.decoder().unwrap();
    for (time, index) in shown {
        let frame = decoder
            .decode_video(DECODER_ID, &request(&output, time))
            .unwrap();
        assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
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
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("click.mp4");
    click_file(&output);

    let mut ffmpeg = FfmpegCodec::new();
    let reference = ffmpeg
        .decoder()
        .unwrap()
        .decode_audio(celesta_codec_ffmpeg::DECODER_ID, &output, AUDIO_RATE, 2)
        .unwrap();
    let mut codec = AvfCodec::default();
    let decoder = codec.decoder().unwrap();
    let decoded = decoder
        .decode_audio(DECODER_ID, &output, AUDIO_RATE, 2)
        .unwrap();
    assert_eq!((decoded.sample_rate, decoded.channels), (AUDIO_RATE, 2));
    assert_eq!(decoded.frame_count(), AUDIO_RATE as usize);
    assert_eq!(peak(&decoded.samples, 2), CLICK);
    assert_eq!(peak(&reference.samples, 2), CLICK);

    let mono = decoder
        .decode_audio(DECODER_ID, &output, 44_100, 1)
        .unwrap();
    assert_eq!((mono.sample_rate, mono.channels), (44_100, 1));
    let expected = (CLICK as f64 * 44_100.0 / f64::from(AUDIO_RATE)).round() as usize;
    let found = peak(&mono.samples, 1);
    assert!(found.abs_diff(expected) <= 1, "{found}, not {expected}");

    for (rate, channels) in [(0, 2), (48_000, 0), (48_000, 3)] {
        assert!(matches!(
            decoder.decode_audio(DECODER_ID, &output, rate, channels),
            Err(MediaError::InvalidAudioFormat { .. })
        ));
    }
}

#[test]
fn reads_m4a() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("sound.m4a");
    let mut codec = AvfCodec::default();
    let (audio, track) = encode_audio(&mut codec, "aac", 0, &click());
    let packets = interleaved(&[(AUDIO_RATE, &audio)]);
    mux(&mut codec, "mp4", &output, &[track], 0, &packets);

    let probe = probe(&output).unwrap();
    assert_eq!(probe.video, None);
    assert_eq!(probe.audio.len(), 1);
    let decoder = codec.decoder().unwrap();
    assert!(matches!(
        decoder.decode_video(DECODER_ID, &request(&output, 0.0)),
        Err(MediaError::NoVideoStream(path)) if path == output
    ));
    let decoded = decoder
        .decode_audio(DECODER_ID, &output, AUDIO_RATE, 2)
        .unwrap();
    assert_eq!(peak(&decoded.samples, 2), CLICK);
}

#[test]
fn refuses_what_it_cannot_read() {
    let directory = tempfile::tempdir().unwrap();
    let unsupported = |result: Result<MediaProbe, MediaError>, path: &Path| match result {
        Err(MediaError::Unsupported { path: refused, .. }) => assert_eq!(refused, path),
        other => panic!("{}: expected unsupported, got {other:?}", path.display()),
    };
    let webm_file = directory.path().join("sound.webm");
    webm(&webm_file);
    unsupported(probe(&webm_file), &webm_file);
    let text = directory.path().join("x.mp4");
    std::fs::write(&text, "not a movie\n").unwrap();
    unsupported(probe(&text), &text);

    let missing = directory.path().join("missing.mp4");
    let mut codec = AvfCodec::default();
    let decoder = codec.decoder().unwrap();
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
        Err(MediaError::Codec(message)) if message == r#"no decoder "ffmpeg" in avfoundation"#
    ));
}

/// The quadrants of the rotated test's source: top left, top right,
/// bottom left, bottom right, as RGB.
const QUADRANTS: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];

/// A 64x36 BGRA pixel buffer of [`QUADRANTS`].
fn quadrants() -> CFRetained<CVPixelBuffer> {
    let mut buffer: *mut CVPixelBuffer = null_mut();
    // SAFETY: `buffer` is a valid place for the new pixel buffer; no
    // attributes are given.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            WIDTH as usize,
            HEIGHT as usize,
            kCVPixelFormatType_32BGRA,
            None,
            NonNull::from(&mut buffer),
        )
    };
    assert_eq!(status, 0);
    // SAFETY: the buffer is the caller's to release (the Create rule).
    let buffer = unsafe { CFRetained::from_raw(NonNull::new(buffer).unwrap()) };
    let lock = CVPixelBufferLockFlags(0);
    // SAFETY: the buffer is valid.
    assert_eq!(unsafe { CVPixelBufferLockBaseAddress(&buffer, lock) }, 0);
    let base = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
    let stride = CVPixelBufferGetBytesPerRow(&buffer);
    for row in 0..HEIGHT as usize {
        for column in 0..WIDTH as usize {
            let quadrant = usize::from(row >= HEIGHT as usize / 2) * 2
                + usize::from(column >= WIDTH as usize / 2);
            let [r, g, b] = QUADRANTS[quadrant];
            // SAFETY: the base address is locked; a 32BGRA buffer of this
            // size has `HEIGHT` rows of `stride >= 4 × WIDTH` bytes.
            unsafe {
                base.add(row * stride + column * 4)
                    .copy_from_nonoverlapping([b, g, r, 255].as_ptr(), 4);
            }
        }
    }
    // SAFETY: locked above with these flags.
    assert_eq!(unsafe { CVPixelBufferUnlockBaseAddress(&buffer, lock) }, 0);
    buffer
}

/// Writes ten frames of [`quadrants`] at 10 fps with `AVAssetWriter`'s own
/// H.264 to `output`, its track turned a quarter clockwise
/// (`preferredTransform`), as a phone held upright records.
fn rotated(output: &Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(output.to_str().unwrap()));
    // SAFETY: AVFoundation's documented calls, in order: the input and its
    // adaptor before writing starts, frames while the input is ready, then
    // the end; the transform is set before writing starts.
    unsafe {
        let writer =
            AVAssetWriter::assetWriterWithURL_fileType_error(&url, AVFileTypeMPEG4.unwrap())
                .unwrap();
        let keys = [
            AVVideoCodecKey.unwrap(),
            AVVideoWidthKey.unwrap(),
            AVVideoHeightKey.unwrap(),
        ];
        let width = NSNumber::new_u32(WIDTH);
        let height = NSNumber::new_u32(HEIGHT);
        let values: [&AnyObject; 3] = [
            AVVideoCodecTypeH264.unwrap().as_ref(),
            width.as_ref(),
            height.as_ref(),
        ];
        let settings = NSDictionary::from_slices(&keys, &values);
        let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
            AVMediaTypeVideo.unwrap(),
            Some(&settings),
        );
        input.setExpectsMediaDataInRealTime(false);
        input.setTransform(CGAffineTransform {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            tx: f64::from(HEIGHT),
            ty: 0.0,
        });
        let adaptor =
            AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &input, None,
            );
        assert!(writer.canAddInput(&input));
        writer.addInput(&input);
        assert!(writer.startWriting(), "{:?}", writer.error());
        writer.startSessionAtSourceTime(kCMTimeZero);
        let buffer = quadrants();
        for index in 0..10 {
            let started = Instant::now();
            while !input.isReadyForMoreMediaData() {
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(1));
            }
            let time = CMTime {
                value: index,
                timescale: 10,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            };
            assert!(adaptor.appendPixelBuffer_withPresentationTime(&buffer, time));
        }
        input.markAsFinished();
        let (sender, done) = mpsc::channel();
        let handler = RcBlock::new(move || {
            let _ = sender.send(());
        });
        writer.finishWritingWithCompletionHandler(&handler);
        done.recv().unwrap();
        assert_eq!(writer.status(), AVAssetWriterStatus::Completed);
    }
}

#[test]
fn rotated_video_is_displayed_upright() {
    let directory = tempfile::tempdir().unwrap();
    let output: PathBuf = directory.path().join("portrait.mp4");
    rotated(&output);

    let probe = probe(&output).unwrap();
    let stream = probe.video.expect("a video stream");
    assert_eq!((stream.width, stream.height), (HEIGHT, WIDTH));
    let frame = AvfCodec::default()
        .decoder()
        .unwrap()
        .decode_video(DECODER_ID, &request(&output, 0.0))
        .unwrap();
    assert_eq!((frame.width, frame.height), (HEIGHT, WIDTH));
    assert_eq!(frame.pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    // Turned a quarter clockwise, the source's bottom left is the top left,
    // its top left the top right, and its bottom right the bottom left.
    // Within 40 per channel: AVAssetWriter converts the fixture itself, and
    // VideoToolbox's software H.264 encoder (a CI machine's) bleeds blue
    // into (25, 30, 247); the four quadrants stay far apart.
    let close = |actual: [u8; 3], expected: [u8; 3]| {
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 40)
    };
    for ((column, row), quadrant) in [
        ((2, 2), 2),
        ((HEIGHT - 3, 2), 0),
        ((2, WIDTH - 3), 3),
        ((HEIGHT - 3, WIDTH - 3), 1),
    ] {
        let actual = rgb(&frame, column, row);
        assert!(
            close(actual, QUADRANTS[quadrant]),
            "({column}, {row}): {actual:?}, not {:?}",
            QUADRANTS[quadrant]
        );
    }
}
