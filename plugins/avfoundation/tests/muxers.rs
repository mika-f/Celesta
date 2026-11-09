//! The AVAssetWriter muxers, in this process: what they write of the
//! VideoToolbox, AudioToolbox, and FFmpeg encoders' packets, read back with
//! the FFmpeg plugin's decoder and libavformat, and with AVFoundation's own
//! reader (`AVAssetReader`, through this plugin's decoder).
#![cfg(target_os = "macos")]

use celesta_codec_avfoundation::AvfCodec;
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoderRequest, Color, Encoded, MediaError, MediaProvider, MuxerRequest, Packet,
    PixelFormat, SettingValue, Settings, Track, TrackConfig, TrackPacket, VideoEncoderRequest,
    VideoRequest,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;
/// 29.97 fps: a frame lasts 1001 of the 30000 ticks a second.
const RATE: Rational = Rational {
    numerator: 30_000,
    denominator: 1_001,
};
const TIMESCALE: u32 = 30_000;
const FRAME: i64 = 1_001;

/// The audio's rate, which `aac` takes as it is.
const AUDIO_RATE: u32 = 48_000;
/// AudioToolbox's AAC-LC priming, in sample frames.
const AAC_DELAY: u64 = 2_112;
/// The frame of the click in [`click`].
const CLICK: usize = 4_800;
/// The loop pre-roll the delay test encodes before 0.
const PRE_ROLL: usize = 16_384;

/// BT.709 limited-range red in 8 bits.
const RED_8: [u8; 3] = [63, 102, 240];

fn text_setting(key: &str, value: &str) -> Settings {
    [(key.to_owned(), SettingValue::String(value.to_owned()))].into()
}

/// An NV12 frame of one colour.
fn nv12_solid([y, cb, cr]: [u8; 3]) -> Vec<u8> {
    let luma = (WIDTH * HEIGHT) as usize;
    let mut frame = vec![y; luma];
    frame.extend([cb, cr].repeat(luma / 4));
    frame
}

/// An RGBA16 frame: `left` in the left half and `right` in the right.
fn rgba16_halves(left: [u16; 4], right: [u16; 4]) -> Vec<u8> {
    (0..HEIGHT)
        .flat_map(|_| {
            (0..WIDTH).flat_map(move |column| if column < WIDTH / 2 { left } else { right })
        })
        .flat_map(u16::to_le_bytes)
        .collect()
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

/// A 64x36 29.97 fps BT.709 video track of `config`.
fn video_track(config: TrackConfig, alpha: bool) -> Track {
    Track::Video {
        config,
        timescale: TIMESCALE,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: RATE,
        color: Color::bt709_limited(),
        alpha,
    }
}

/// A 48 kHz stereo audio track of `config` whose encoder delays it by
/// `delay`.
fn audio_track(config: TrackConfig, delay: u64) -> Track {
    Track::Audio {
        config,
        timescale: AUDIO_RATE,
        sample_rate: AUDIO_RATE,
        channels: 2,
        delay,
    }
}

/// `count` frames of `frame` through the provider's video encoder
/// `component` at 64x36, 29.97 fps: the packets and the track.
fn encode_video(
    provider: &mut dyn MediaProvider,
    component: &str,
    settings: &Settings,
    offered: &[PixelFormat],
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> (Vec<Packet>, Track) {
    let encoder = provider.video_encoder().expect("video encoders");
    let opened = encoder
        .open_video(&VideoEncoderRequest {
            component,
            settings,
            width: WIDTH,
            height: HEIGHT,
            frame_rate: RATE,
            timescale: TIMESCALE,
            offered,
            color: &Color::bt709_limited(),
        })
        .unwrap();
    let mut answers: Vec<Encoded> = frames
        .into_iter()
        .enumerate()
        .map(|(index, frame)| {
            encoder
                .encode_video(opened.session, index as i64 * FRAME, frame)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_video(opened.session).unwrap());
    let (packets, config) = packets_and_config(answers);
    (packets, video_track(config, opened.alpha))
}

/// `count` frames of BT.709 red through this plugin's `h264`.
fn h264(count: usize) -> (Vec<Packet>, Track) {
    encode_video(
        &mut AvfCodec::default(),
        "h264",
        &Settings::new(),
        &[PixelFormat::Nv12],
        (0..count).map(|_| nv12_solid(RED_8)),
    )
}

/// Interleaved stereo `samples`, the first at `start`, through the
/// provider's audio encoder `component` in the exporter's chunks of 1024 ×
/// 16 frames: the packets and the track.
fn encode_audio(
    provider: &mut dyn MediaProvider,
    component: &str,
    settings: &Settings,
    start: i64,
    samples: &[f32],
) -> (Vec<Packet>, Track) {
    let encoder = provider.audio_encoder().expect("audio encoders");
    let opened = encoder
        .open_audio(&AudioEncoderRequest {
            component,
            settings,
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
    (packets, audio_track(config, opened.delay))
}

/// `samples` from `start` through this plugin's `aac`.
fn aac(start: i64, samples: &[f32]) -> (Vec<Packet>, Track) {
    encode_audio(
        &mut AvfCodec::default(),
        "aac",
        &Settings::new(),
        start,
        samples,
    )
}

/// A second of stereo silence with a click (1.0) at [`CLICK`].
fn click() -> Vec<f32> {
    let mut samples = vec![0.0; AUDIO_RATE as usize * 2];
    samples[CLICK * 2] = 1.0;
    samples[CLICK * 2 + 1] = 1.0;
    samples
}

/// `packets` of track `track`.
fn on_track(track: usize, packets: &[Packet]) -> Vec<TrackPacket> {
    packets
        .iter()
        .map(|packet| TrackPacket {
            track,
            packet: packet.clone(),
        })
        .collect()
}

/// The packets of every track, in decode order across them, as the
/// exporter interleaves them; `tracks` gives each one's timescale.
fn interleaved(tracks: &[(u32, &[Packet])]) -> Vec<TrackPacket> {
    let mut all: Vec<TrackPacket> = tracks
        .iter()
        .enumerate()
        .flat_map(|(track, (_, packets))| on_track(track, packets))
        .collect();
    all.sort_by(|left, right| {
        let seconds =
            |packet: &TrackPacket| packet.packet.dts as f64 / f64::from(tracks[packet.track].0);
        seconds(left).total_cmp(&seconds(right))
    });
    all
}

/// A request for `component` with its default settings.
fn request<'a>(
    component: &'a str,
    output: &'a Path,
    tracks: &'a [Track],
    leading_audio: u64,
) -> MuxerRequest<'a> {
    MuxerRequest {
        component,
        settings: &EMPTY,
        output,
        tracks,
        leading_audio,
    }
}

/// The muxers' default settings.
static EMPTY: Settings = Settings::new();

/// Writes `packets` into `output` with this plugin's muxer `component`, in
/// one `writePackets` for each of `writes`: the files `finish` answers.
fn mux(
    component: &str,
    output: &Path,
    tracks: &[Track],
    leading_audio: u64,
    writes: &[Vec<TrackPacket>],
) -> Vec<PathBuf> {
    let mut codec = AvfCodec::default();
    let muxer = codec.muxer().expect("muxers");
    let session = muxer
        .open_muxer(&request(component, output, tracks, leading_audio))
        .unwrap();
    for packets in writes {
        muxer.write_packets(session, packets).unwrap();
    }
    muxer.finish_muxer(session).unwrap()
}

/// The packets libavformat reads of each stream of `path`, by stream.
fn packet_counts(path: &Path) -> Vec<usize> {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: the context and packet are opened, read, and freed here.
    unsafe {
        let mut context = std::ptr::null_mut();
        assert!(
            ff::avformat_open_input(
                &mut context,
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut()
            ) >= 0
        );
        assert!(ff::avformat_find_stream_info(context, std::ptr::null_mut()) >= 0);
        let mut counts = vec![0; (*context).nb_streams as usize];
        let mut packet = ff::av_packet_alloc();
        while ff::av_read_frame(context, packet) >= 0 {
            counts[(*packet).stream_index as usize] += 1;
            ff::av_packet_unref(packet);
        }
        ff::av_packet_free(&mut packet);
        ff::avformat_close_input(&mut context);
        counts
    }
}

/// The bytes of every audio packet stored in the QuickTime file `path`,
/// read past its edit lists: what any reader finds, whether or not it
/// honours them.
fn stored_audio(path: &Path) -> Vec<u8> {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: the context, options, and packet are made, read, and freed
    // here; a packet's data is `size` bytes.
    unsafe {
        let mut options = std::ptr::null_mut();
        ff::av_dict_set(&mut options, c"ignore_editlist".as_ptr(), c"1".as_ptr(), 0);
        let mut context = std::ptr::null_mut();
        let opened =
            ff::avformat_open_input(&mut context, url.as_ptr(), std::ptr::null(), &mut options);
        ff::av_dict_free(&mut options);
        assert!(opened >= 0);
        assert!(ff::avformat_find_stream_info(context, std::ptr::null_mut()) >= 0);
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let audio = streams
            .iter()
            .position(|&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_AUDIO
            })
            .expect("an audio stream") as i32;
        let mut bytes = Vec::new();
        let mut packet = ff::av_packet_alloc();
        while ff::av_read_frame(context, packet) >= 0 {
            if (*packet).stream_index == audio {
                bytes.extend_from_slice(std::slice::from_raw_parts(
                    (*packet).data,
                    (*packet).size as usize,
                ));
            }
            ff::av_packet_unref(packet);
        }
        ff::av_packet_free(&mut packet);
        ff::avformat_close_input(&mut context);
        bytes
    }
}

/// The four bytes after the first `colr` in `path` (the colour box's type).
fn colr_atom(path: &Path) -> Option<[u8; 4]> {
    let bytes = std::fs::read(path).unwrap();
    let at = bytes.windows(4).position(|window| window == b"colr")?;
    bytes.get(at + 4..at + 8)?.try_into().ok()
}

/// Whether `path` has the four-character code `code` anywhere.
fn has_code(path: &Path, code: &[u8; 4]) -> bool {
    std::fs::read(path)
        .unwrap()
        .windows(4)
        .any(|window| window == code)
}

/// The first video frame of `path` as the FFmpeg plugin decodes it: its
/// size and RGBA pixels.
fn ffmpeg_frame(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut ffmpeg = FfmpegCodec::new();
    let frame = ffmpeg
        .decoder()
        .unwrap()
        .decode_video(
            celesta_codec_ffmpeg::DECODER_ID,
            &VideoRequest {
                path,
                time: 0.0,
                sequential: None,
            },
        )
        .unwrap();
    (frame.width, frame.height, frame.pixels.to_vec())
}

/// The pixel at `(column, row)` of a 64-wide RGBA frame.
fn pixel(pixels: &[u8], column: u32, row: u32) -> [u8; 4] {
    let start = ((row * WIDTH + column) * 4) as usize;
    pixels[start..start + 4].try_into().unwrap()
}

/// `path`'s audio as the FFmpeg plugin decodes it, at 48 kHz stereo.
fn ffmpeg_audio(path: &Path) -> Vec<f32> {
    let mut ffmpeg = FfmpegCodec::new();
    ffmpeg
        .decoder()
        .unwrap()
        .decode_audio(celesta_codec_ffmpeg::DECODER_ID, path, AUDIO_RATE, 2)
        .unwrap()
        .samples
}

/// `path`'s audio as this plugin's decoder (`AVAssetReader`) decodes it, at
/// 48 kHz stereo.
fn avfoundation_audio(path: &Path) -> Vec<f32> {
    let mut codec = AvfCodec::default();
    codec
        .decoder()
        .unwrap()
        .decode_audio(celesta_codec_avfoundation::DECODER_ID, path, AUDIO_RATE, 2)
        .unwrap()
        .samples
}

/// The frame of the loudest sample of `samples`' first channel (stereo).
fn peak(samples: &[f32]) -> usize {
    samples
        .iter()
        .step_by(2)
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .map(|(frame, _)| frame)
        .unwrap()
}

/// The loudest sample of `samples`' first `frames` frames (stereo).
fn loudest(samples: &[f32], frames: usize) -> f32 {
    samples[..frames * 2]
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max)
}

#[test]
fn h264_and_aac_in_mp4_read_back() {
    let (video, video_track) = h264(30);
    let (audio, audio_track) = aac(0, &click());
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.mp4");
    let tracks = [video_track, audio_track];
    let packets = interleaved(&[(TIMESCALE, &video), (AUDIO_RATE, &audio)]);
    assert_eq!(
        mux("mp4", &output, &tracks, 0, &[packets]),
        std::slice::from_ref(&output)
    );

    let mut ffmpeg = FfmpegCodec::new();
    let probe = ffmpeg
        .decoder()
        .unwrap()
        .probe(celesta_codec_ffmpeg::DECODER_ID, &output)
        .unwrap();
    let stream = probe.video.expect("a video stream");
    assert_eq!((stream.width, stream.height), (WIDTH, HEIGHT));
    assert_eq!(stream.frame_rate, Some(RATE));
    // The whole of the last frame: 30 × 1001 / 30000 s.
    let seconds = stream.duration.expect("a duration").as_seconds().unwrap();
    assert!((seconds - 1.001).abs() < 1e-6, "{seconds} s");
    assert_eq!(probe.audio.len(), 1);
    assert_eq!(packet_counts(&output), [30, audio.len()]);
    let (width, height, pixels) = ffmpeg_frame(&output);
    assert_eq!((width, height), (WIDTH, HEIGHT));
    for (column, row) in [(0, 0), (WIDTH / 2, HEIGHT / 2), (WIDTH - 1, HEIGHT - 1)] {
        let [r, g, b, a] = pixel(&pixels, column, row);
        assert!(
            r >= 252 && g <= 3 && b <= 3 && a == 255,
            "({column}, {row}): {:?}",
            [r, g, b, a]
        );
    }
    assert_eq!(colr_atom(&output), Some(*b"nclx"));
    assert!(has_code(&output, b"avc1") && has_code(&output, b"avcC"));
    // Fast start, by default: the movie before its media.
    let bytes = std::fs::read(&output).unwrap();
    let at = |code: &[u8]| bytes.windows(4).position(|window| window == code);
    assert!(
        at(b"moov") < at(b"mdat"),
        "{:?} {:?}",
        at(b"moov"),
        at(b"mdat")
    );
    assert_eq!(peak(&ffmpeg_audio(&output)), CLICK);
}

#[test]
fn aac_delay_and_preroll_are_hidden_for_every_reader() {
    // The pre-roll the exporter mixes before 0 when a composition loops:
    // constant 0.25, so a reader that shows any of it is easy to tell.
    let mut samples = vec![0.25; PRE_ROLL * 2];
    samples.extend(click());
    let (audio, track) = aac(-(PRE_ROLL as i64), &samples);
    assert_eq!(audio[0].pts, -(PRE_ROLL as i64) - AAC_DELAY as i64);
    let directory = tempfile::tempdir().unwrap();
    for component in ["mp4", "mov"] {
        let output = directory.path().join(format!("click.{component}"));
        let tracks = [track.clone()];
        mux(
            component,
            &output,
            &tracks,
            PRE_ROLL as u64,
            &[on_track(0, &audio)],
        );
        for (reader, decoded) in [
            ("FFmpeg", ffmpeg_audio(&output)),
            ("AVFoundation", avfoundation_audio(&output)),
        ] {
            // The last packet covers 960 frames and is trimmed by 64 at its
            // end: exactly the second the click track is.
            let frames = decoded.len() / 2;
            assert_eq!(frames, AUDIO_RATE as usize, "{component}, {reader}");
            assert_eq!(peak(&decoded), CLICK, "{component}, {reader}");
            let early = loudest(&decoded, 4_000);
            assert!(
                early < 0.1,
                "{component}, {reader}: {early} before the click"
            );
        }
    }
}

#[test]
fn an_aac_track_without_delay_is_written() {
    // AAC from an encoder that says it has no delay: packets from 0, so no
    // buffer has anything to trim at its start.
    let (mut audio, track) = aac(0, &click());
    for packet in &mut audio {
        packet.pts += AAC_DELAY as i64;
        packet.dts = packet.pts;
    }
    let Track::Audio { config, .. } = track else {
        unreachable!("an audio track");
    };
    let tracks = [audio_track(config, 0)];
    let directory = tempfile::tempdir().unwrap();
    for component in ["mp4", "mov"] {
        let output = directory.path().join(format!("undelayed.{component}"));
        mux(component, &output, &tracks, 0, &[on_track(0, &audio)]);
        assert_eq!(packet_counts(&output), [audio.len()], "{component}");
        for (reader, decoded) in [
            ("FFmpeg", ffmpeg_audio(&output)),
            ("AVFoundation", avfoundation_audio(&output)),
        ] {
            // The encoder's priming now plays: the click is that much late.
            assert_eq!(
                peak(&decoded),
                CLICK + AAC_DELAY as usize,
                "{component}, {reader}"
            );
        }
    }
}

#[test]
fn prores_and_lpcm_in_mov() {
    let mut ffmpeg = FfmpegCodec::new();
    let red = [65_535, 0, 0, 65_535];
    let half_green = [0, 65_535, 0, 32_768];
    let (video, video_track) = encode_video(
        &mut ffmpeg,
        "prores",
        &text_setting("profile", "4444"),
        &[PixelFormat::Rgba16],
        (0..10).map(|_| rgba16_halves(red, half_green)),
    );
    assert!(matches!(video_track, Track::Video { alpha: true, .. }));
    // A ramp, so every frame near the start differs from its neighbours.
    const LEAD: usize = 1_024;
    let mix: Vec<f32> = (0..LEAD + AUDIO_RATE as usize)
        .flat_map(|frame| {
            let value = (frame % 1_000) as f32 / 1_000.0 - 0.5;
            [value, -value]
        })
        .collect();
    let (audio, audio_track) = encode_audio(
        &mut ffmpeg,
        "pcm",
        &text_setting("format", "s24le"),
        -(LEAD as i64),
        &mix,
    );
    assert!(
        matches!(&audio_track, Track::Audio { config, delay: 0, .. } if config.codec == "lpcm.s24le")
    );
    assert!(audio[0].pts < 0);

    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("alpha.mov");
    let tracks = [video_track, audio_track];
    let packets = interleaved(&[(TIMESCALE, &video), (AUDIO_RATE, &audio)]);
    mux("mov", &output, &tracks, LEAD as u64, &[packets]);

    let (_, _, pixels) = ffmpeg_frame(&output);
    let [_, green, _, alpha] = pixel(&pixels, WIDTH - 5, HEIGHT / 2);
    assert!(alpha.abs_diff(128) <= 2 && green >= 250, "{alpha} {green}");
    assert_eq!(pixel(&pixels, 4, HEIGHT / 2)[3], 255);

    let decoded = ffmpeg_audio(&output);
    assert_eq!(decoded.len(), AUDIO_RATE as usize * 2);
    for frame in 0..16 {
        for channel in 0..2 {
            let expected = mix[(LEAD + frame) * 2 + channel];
            let actual = decoded[frame * 2 + channel];
            assert!(
                (actual - expected).abs() < 1e-6,
                "frame {frame}, channel {channel}: {actual} is not {expected}"
            );
        }
    }
    // Cut, not edited out: the file stores no frame before 0.
    let stored = stored_audio(&output);
    assert_eq!(stored.len(), AUDIO_RATE as usize * 2 * 3);
    let first = &audio
        .iter()
        .flat_map(|packet| packet.bytes.iter().copied())
        .collect::<Vec<u8>>()[LEAD * 2 * 3..];
    assert_eq!(stored[..64], first[..64]);
}

#[test]
fn an_empty_staged_output_is_replaced() {
    let (video, track) = h264(5);
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("staged.mp4");
    std::fs::write(&output, b"").unwrap();
    let written = mux("mp4", &output, &[track], 0, &[on_track(0, &video)]);
    assert_eq!(written, std::slice::from_ref(&output));
    assert_eq!(packet_counts(&output), [5]);
}

#[test]
fn a_file_with_content_is_not_replaced() {
    let (_, track) = h264(1);
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("taken.mp4");
    std::fs::write(&output, b"x").unwrap();
    let mut codec = AvfCodec::default();
    let tracks = [track];
    match codec
        .muxer()
        .unwrap()
        .open_muxer(&request("mp4", &output, &tracks, 0))
    {
        Err(MediaError::Codec(message)) => {
            assert!(message.contains("already exists"), "{message}");
        }
        other => panic!("opened over a file with content: {other:?}"),
    }
    assert_eq!(std::fs::read(&output).unwrap(), b"x");
}

#[test]
fn long_runs_of_one_track_do_not_stall() {
    let (video, video_track) = h264(300);
    // As long as the video: ten seconds and a frame.
    let frames = 300 * FRAME as usize * AUDIO_RATE as usize / TIMESCALE as usize;
    let samples: Vec<f32> = (0..frames * 2)
        .map(|index| ((index / 2) as f32 * 0.01).sin() * 0.1)
        .collect();
    let (audio, audio_track) = aac(0, &samples);
    let tracks = [video_track, audio_track];
    let directory = tempfile::tempdir().unwrap();
    for (name, writes) in [
        ("video-first", [on_track(0, &video), on_track(1, &audio)]),
        ("audio-first", [on_track(1, &audio), on_track(0, &video)]),
    ] {
        let output = directory.path().join(format!("{name}.mp4"));
        let started = Instant::now();
        mux("mp4", &output, &tracks, 0, &writes);
        let took = started.elapsed();
        assert!(took < Duration::from_secs(10), "{name} took {took:?}");
        assert_eq!(packet_counts(&output), [300, audio.len()], "{name}");
    }
}

#[test]
fn an_unknown_codec_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("opus.mp4");
    let tracks = [audio_track(
        TrackConfig {
            codec: "opus".to_owned(),
            description: None,
        },
        312,
    )];
    let mut codec = AvfCodec::default();
    match codec
        .muxer()
        .unwrap()
        .open_muxer(&request("mp4", &output, &tracks, 0))
    {
        Err(MediaError::Codec(message)) => assert_eq!(message, "mp4 does not take opus"),
        other => panic!("opened opus in mp4: {other:?}"),
    }
    // AAC other than LC (HE-AAC here), whose packets are not 1024 frames.
    let tracks = [audio_track(
        TrackConfig {
            codec: "mp4a.40.5".to_owned(),
            description: Some(vec![0x2b, 0x92, 0x08, 0x00]),
        },
        2_112,
    )];
    for muxer in ["mp4", "mov"] {
        match codec
            .muxer()
            .unwrap()
            .open_muxer(&request(muxer, &output, &tracks, 0))
        {
            Err(MediaError::Codec(message)) => {
                assert_eq!(message, format!("{muxer} does not take mp4a.40.5"));
            }
            other => panic!("opened HE-AAC in {muxer}: {other:?}"),
        }
    }
}

#[test]
fn a_failed_writer_takes_no_more_packets() {
    let (video, track) = h264(10);
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("failed.mp4");
    let mut codec = AvfCodec::default();
    let muxer = codec.muxer().unwrap();
    let tracks = [track];
    let session = muxer
        .open_muxer(&request("mp4", &output, &tracks, 0))
        .unwrap();
    muxer
        .write_packets(session, &on_track(0, &video[..5]))
        .unwrap();
    // A decode time before the last one written: AVAssetWriter takes the
    // buffer, then fails on its own thread.
    muxer
        .write_packets(session, &on_track(0, &video[1..2]))
        .unwrap();
    let started = Instant::now();
    let error = loop {
        match muxer.write_packets(session, &[]) {
            Err(error) => break error,
            Ok(()) => {
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "the failed writer still takes packets"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    assert!(
        matches!(&error, MediaError::Codec(message) if message.starts_with("AVAssetWriter could not write the file")),
        "{error:?}"
    );
    // The rest is refused, not queued.
    assert!(
        muxer
            .write_packets(session, &on_track(0, &video[5..]))
            .is_err()
    );
    assert!(muxer.finish_muxer(session).is_err());
}

#[test]
fn abort_answers_the_output() {
    let (video, track) = h264(3);
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("aborted.mov");
    let mut codec = AvfCodec::default();
    let muxer = codec.muxer().unwrap();
    let tracks = [track];
    let session = muxer
        .open_muxer(&request("mov", &output, &tracks, 0))
        .unwrap();
    muxer
        .write_packets(session, &on_track(0, &video[..1]))
        .unwrap();
    assert_eq!(muxer.abort_muxer(session), std::slice::from_ref(&output));
    assert!(muxer.finish_muxer(session).is_err());
}
