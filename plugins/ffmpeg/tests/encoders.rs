//! The standard FFmpeg plugin's encoders and muxers, in this process and
//! linked against the standard (LGPL) build: the input ProRes and VP9 choose
//! of those offered, the rate and delay each audio encoder answers, the
//! packets and track config they give, and what `describe` says of every
//! component. libx264's are the x264 plugin's tests.

#[path = "support/encoding.rs"]
mod encoding;
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoderOpened, AudioEncoderRequest, Color, ComponentKind, MediaError, MediaProvider,
    MuxerRequest, Packet, PixelFormat, SampleRates, SettingValue, Settings, Track, TrackConfig,
};
use encoding::{FRAMES, decode, encode_all, frame, open, open_colored, settings, text};
use ffmpeg_sys_next::{
    AVColorPrimaries, AVColorRange, AVColorSpace, AVColorTransferCharacteristic,
};
use std::collections::BTreeSet;

#[test]
fn prores_takes_only_rgba16() {
    let mut codec = FfmpegCodec::new();
    assert_eq!(
        refused(open(
            &mut codec,
            "prores",
            &Settings::new(),
            &[PixelFormat::Rgba8],
        )),
        ("input".to_owned(), "needs rgba16".to_owned())
    );
}

// Unix only: the test makes the trailer fail by removing the file the
// muxer still has open, which Windows refuses ("being used by another
// process").
#[cfg(unix)]
#[test]
fn a_muxer_whose_finish_failed_can_still_be_aborted() {
    let mut codec = FfmpegCodec::new();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("film.mp4");
    let faststart = settings(&[("faststart", SettingValue::Boolean(true))]);
    let muxer = codec.muxer().unwrap();
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &faststart,
            output: &output,
            tracks: &[aac_track()],
            leading_audio: 0,
        })
        .unwrap();
    // `+faststart` reopens the file by name to move the index to its
    // start, so the trailer fails once the file is gone.
    std::fs::remove_file(&output).unwrap();
    assert!(muxer.finish_muxer(session).is_err());
    // The session is still there for the host's abort, which names the
    // file to remove.
    assert_eq!(muxer.abort_muxer(session), [output]);
    assert!(muxer.finish_muxer(session).is_err());
    assert_eq!(muxer.abort_muxer(session), Vec::<std::path::PathBuf>::new());
}

#[test]
fn sessions_are_unique_across_kinds() {
    let mut codec = FfmpegCodec::new();
    let offered = [PixelFormat::Rgba16];
    let first = open(
        &mut codec,
        "prores",
        &Settings::new(),
        &[PixelFormat::Rgba16],
    )
    .unwrap();
    let second = open(&mut codec, "prores", &Settings::new(), &offered).unwrap();
    let audio = open_audio(&mut codec, "aac", &Settings::new(), 48_000, 2).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("film.mp4");
    let muxer = codec
        .muxer()
        .unwrap()
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output: &output,
            tracks: &[aac_track()],
            leading_audio: 0,
        })
        .unwrap();
    let sessions: BTreeSet<u64> = [first.session, second.session, audio.session, muxer]
        .iter()
        .map(|session| session.0)
        .collect();
    assert_eq!(sessions.len(), 4, "{sessions:?}");
    let encoder = codec.video_encoder().unwrap();
    encoder.abort_video(first.session);
    encoder.abort_video(second.session);
    // An aborted session is gone.
    assert!(
        encoder
            .encode_video(first.session, 0, frame(PixelFormat::Rgba8, 0))
            .is_err()
    );
    let audio_encoder = codec.audio_encoder().unwrap();
    audio_encoder.abort_audio(audio.session);
    assert!(
        audio_encoder
            .encode_audio(audio.session, 0, &[0.0; 2])
            .is_err()
    );
    // An aborted muxer leaves its file for Celesta to remove.
    assert_eq!(codec.muxer().unwrap().abort_muxer(muxer), [output]);
    assert!(codec.muxer().unwrap().write_packets(muxer, &[]).is_err());
}

/// The key and message of the refusal `result` holds.
fn refused<T: std::fmt::Debug>(result: Result<T, MediaError>) -> (String, String) {
    match result {
        Err(MediaError::InvalidSettings { key, message }) => (key, message),
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

/// Opens audio encoder `component` for a mix at `sample_rate`.
fn open_audio(
    codec: &mut dyn MediaProvider,
    component: &str,
    given: &Settings,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioEncoderOpened, MediaError> {
    codec
        .audio_encoder()
        .expect("the plugin has audio encoders")
        .open_audio(&AudioEncoderRequest {
            component,
            settings: given,
            sample_rate,
            channels,
        })
}

/// A stereo 48 kHz AAC track, as `aac` describes it.
fn aac_track() -> Track {
    Track::Audio {
        config: TrackConfig {
            codec: "mp4a.40.2".to_owned(),
            description: Some(vec![0x11, 0x90]),
        },
        timescale: 48_000,
        sample_rate: 48_000,
        channels: 2,
        delay: 1024,
    }
}

/// `frames` sample frames of a quiet stereo tone through `opened`, in the
/// exporter's chunks, then the finish: every packet and the config.
fn encode_audio(
    codec: &mut dyn MediaProvider,
    opened: &AudioEncoderOpened,
    frames: usize,
) -> (Vec<Packet>, TrackConfig) {
    let channels = usize::from(opened.channels);
    let samples: Vec<f32> = (0..frames * channels)
        .map(|index| ((index / channels) as f32 * 0.05).sin() * 0.25)
        .collect();
    let encoder = codec.audio_encoder().unwrap();
    let mut packets = Vec::new();
    let mut config = None;
    let mut pts = 0;
    for chunk in samples.chunks(1024 * 16 * channels) {
        let answer = encoder.encode_audio(opened.session, pts, chunk).unwrap();
        pts += (chunk.len() / channels) as i64;
        if config.is_none() && !answer.packets.is_empty() {
            config = Some(
                answer
                    .config
                    .expect("the first packets come with the config"),
            );
        }
        packets.extend(answer.packets);
    }
    let answer = encoder.finish_audio(opened.session).unwrap();
    config = config.or(answer.config);
    packets.extend(answer.packets);
    (packets, config.expect("the encoder gave its config"))
}

#[test]
fn aac_takes_the_mixs_rate_when_it_can() {
    let mut codec = FfmpegCodec::new();
    for (asked, answered) in [(8_000, 8_000), (44_100, 44_100), (12_345, 48_000)] {
        let opened = open_audio(&mut codec, "aac", &Settings::new(), asked, 1).unwrap();
        assert_eq!(
            (opened.sample_rate, opened.channels, opened.delay),
            (answered, 1, 1024),
            "{asked} Hz"
        );
        codec.audio_encoder().unwrap().abort_audio(opened.session);
    }
    let opened = open_audio(&mut codec, "aac", &Settings::new(), 48_000, 2).unwrap();
    let (packets, config) = encode_audio(&mut codec, &opened, 48_000);
    assert_eq!(config.codec, "mp4a.40.2");
    // AudioSpecificConfig: AAC-LC (2), 48 kHz (index 3), two channels.
    let description = config.description.unwrap();
    assert_eq!(description[..2], [0x11, 0x90]);
    // The first packet starts at the delay before 0; each lasts a frame,
    // the last only the samples left.
    assert_eq!(packets[0].pts, -1024);
    assert!(packets.iter().all(|packet| packet.key));
    let total: i64 = packets.iter().map(|packet| packet.duration).sum();
    assert_eq!(total, 48_000 + 1024);
}

#[test]
fn opus_always_takes_48_khz() {
    let mut codec = FfmpegCodec::new();
    let opened = open_audio(&mut codec, "opus", &Settings::new(), 44_100, 2).unwrap();
    assert_eq!((opened.sample_rate, opened.channels), (48_000, 2));
    assert!(opened.delay > 0, "libopus primes its output");
    let (packets, config) = encode_audio(&mut codec, &opened, 48_000);
    assert_eq!(config.codec, "opus");
    assert_eq!(&config.description.unwrap()[..8], b"OpusHead");
    assert_eq!(packets[0].pts, -(opened.delay as i64));
    let total: i64 = packets.iter().map(|packet| packet.duration).sum();
    assert_eq!(total, 48_000 + opened.delay as i64);
}

#[test]
fn pcm_packs_each_chunk_as_one_packet() {
    let mut codec = FfmpegCodec::new();
    for (format, bytes) in [("s16le", 2), ("s24le", 3), ("f32le", 4)] {
        let opened = open_audio(
            &mut codec,
            "pcm",
            &settings(&[("format", text(format))]),
            22_050,
            2,
        )
        .unwrap();
        assert_eq!(
            (opened.sample_rate, opened.channels, opened.delay),
            (22_050, 2, 0)
        );
        let (packets, config) = encode_audio(&mut codec, &opened, 20_000);
        assert_eq!(config.codec, format!("lpcm.{format}"));
        assert_eq!(config.description, None);
        let times: Vec<(i64, i64)> = packets
            .iter()
            .map(|packet| (packet.pts, packet.duration))
            .collect();
        assert_eq!(times, [(0, 16_384), (16_384, 3_616)]);
        assert_eq!(packets[1].bytes.len(), 3_616 * 2 * bytes);
    }
}

#[test]
fn audio_and_muxer_settings_are_checked() {
    let mut codec = FfmpegCodec::new();
    let low = settings(&[("bitrate", SettingValue::Number(1_000.0))]);
    assert_eq!(
        refused(open_audio(&mut codec, "aac", &low, 48_000, 2)).0,
        "bitrate"
    );
    let unknown = settings(&[("format", text("s32le"))]);
    assert_eq!(
        refused(open_audio(&mut codec, "pcm", &unknown, 48_000, 2)).0,
        "format"
    );
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("clip.webm");
    let request = |given| MuxerRequest {
        component: "webm",
        settings: given,
        output: &output,
        tracks: &[],
        leading_audio: 0,
    };
    let faststart = settings(&[("faststart", SettingValue::Boolean(true))]);
    assert_eq!(
        refused(codec.muxer().unwrap().open_muxer(&request(&faststart))).0,
        "faststart"
    );
    let empty = Settings::new();
    // A codec the muxer has no mapping for is refused before any file is
    // made.
    let theora = Track::Video {
        config: TrackConfig {
            codec: "theora".to_owned(),
            description: None,
        },
        timescale: 10,
        width: 2,
        height: 2,
        frame_rate: Rational::new(10, 1),
        color: Color::bt709_limited(),
        alpha: false,
    };
    let error = codec
        .muxer()
        .unwrap()
        .open_muxer(&MuxerRequest {
            tracks: &[theora],
            ..request(&empty)
        })
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        MediaError::Codec("webm does not take theora".into()).to_string()
    );
    assert!(!output.exists());
    // So is a track color no file may carry (libavutil's `reserved`, H.273's
    // reserved matrix), by its field, in every container.
    for (component, file, codec_string) in [
        ("mp4", "clip.mp4", "avc1.42c00a"),
        ("mov", "clip.mov", "apch"),
        ("webm", "clip.webm", "vp09.00.10.08"),
    ] {
        let output = directory.path().join(file);
        let track = Track::Video {
            config: TrackConfig {
                codec: codec_string.to_owned(),
                description: None,
            },
            timescale: 10,
            width: 2,
            height: 2,
            frame_rate: Rational::new(10, 1),
            color: Color {
                matrix: "reserved".to_owned(),
                ..Color::bt709_limited()
            },
            alpha: false,
        };
        let error = codec
            .muxer()
            .unwrap()
            .open_muxer(&MuxerRequest {
                component,
                output: &output,
                tracks: &[track],
                ..request(&empty)
            })
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            MediaError::Codec("unknown color matrix `reserved`".into()).to_string(),
            "{component}"
        );
        assert!(!output.exists(), "{component}");
    }
}

#[test]
fn prores_4444_keeps_alpha_and_hq_does_not() {
    let mut codec = FfmpegCodec::new();
    let offered = [
        PixelFormat::Yuv420p,
        PixelFormat::Rgba16,
        PixelFormat::Rgba8,
    ];
    for (profile, alpha, family) in [("4444", true, "ap4h"), ("hq", false, "apch")] {
        let opened = open(
            &mut codec,
            "prores",
            &settings(&[("profile", text(profile))]),
            &offered,
        )
        .unwrap();
        assert_eq!(opened.input, PixelFormat::Rgba16);
        assert_eq!(opened.alpha, alpha, "{profile}");
        let (packets, config) = encode_all(&mut codec, opened);
        assert_eq!(config.codec, family);
        assert_eq!(config.description, None);
        assert_eq!(packets.len(), FRAMES as usize);
        assert!(packets.iter().all(|packet| packet.key));
    }
}

#[test]
fn vp9_alpha_rides_as_block_additions() {
    let mut codec = FfmpegCodec::new();
    let offered = [PixelFormat::Rgba16, PixelFormat::Rgba8];
    for alpha in [true, false] {
        let opened = open(
            &mut codec,
            "vp9",
            &settings(&[("alpha", SettingValue::Boolean(alpha))]),
            &offered,
        )
        .unwrap();
        assert_eq!(opened.input, PixelFormat::Rgba16);
        assert_eq!(opened.alpha, alpha);
        let (packets, config) = encode_all(&mut codec, opened);
        assert_eq!(config.codec, "vp09.00.10.08");
        assert_eq!(config.description, None);
        assert!(!packets.is_empty());
        for packet in &packets {
            if alpha {
                assert_eq!(packet.additional.len(), 1, "{packet:?}");
                assert_eq!(packet.additional[0].id, 1);
                assert!(!packet.additional[0].bytes.is_empty());
            } else {
                assert!(packet.additional.is_empty());
            }
        }
    }
}

#[test]
fn prores_and_vp9_are_tagged_bt709() {
    let mut codec = FfmpegCodec::new();
    let opened = open(
        &mut codec,
        "prores",
        &settings(&[("profile", text("4444"))]),
        &[PixelFormat::Rgba16],
    )
    .unwrap();
    let (packets, _) = encode_all(&mut codec, opened);
    let frames = decode("prores", None, &packets);
    assert_eq!(frames.len(), FRAMES as usize);
    for frame in &frames {
        assert_eq!(
            (frame.colorspace, frame.primaries, frame.transfer),
            (
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorPrimaries::AVCOL_PRI_BT709,
                AVColorTransferCharacteristic::AVCOL_TRC_BT709
            )
        );
    }

    let opened = open(&mut codec, "vp9", &Settings::new(), &[PixelFormat::Rgba16]).unwrap();
    let (packets, _) = encode_all(&mut codec, opened);
    let frames = decode("libvpx-vp9", None, &packets);
    assert_eq!(frames.len(), FRAMES as usize);
    for frame in &frames {
        assert_eq!(
            (frame.colorspace, frame.range),
            (
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorRange::AVCOL_RANGE_MPEG
            )
        );
    }
    // A profile 0 key frame's header: one byte of marker, profile, and
    // flags, the three-byte sync code, then `color_space`'s three bits,
    // CS_BT_709 being 2.
    assert_eq!(packets[0].bytes[1..4], [0x49, 0x83, 0x42]);
    assert_eq!(packets[0].bytes[4] >> 5, 2);
}

#[test]
fn an_unknown_color_name_is_refused() {
    let colors = [
        (
            Color {
                matrix: "nonsense".into(),
                ..Color::bt709_limited()
            },
            "unknown color matrix `nonsense`",
        ),
        (
            Color {
                range: "wide".into(),
                ..Color::bt709_limited()
            },
            "unknown color range `wide`",
        ),
    ];
    let mut codec = FfmpegCodec::new();
    for component in ["prores", "vp9"] {
        for (color, expected) in &colors {
            let result = open_colored(
                &mut codec,
                component,
                &Settings::new(),
                &[PixelFormat::Rgba16],
                color,
            );
            match result {
                Err(MediaError::Codec(message)) => {
                    assert!(message.contains(expected), "{component}: {message}");
                }
                other => panic!("{component} took {color:?}: {:?}", other.map(|_| ())),
            }
        }
    }
}

#[test]
fn an_rgb_matrix_libswscale_cannot_convert_to_is_refused() {
    let mut codec = FfmpegCodec::new();
    let ycgco = Color {
        matrix: "ycgco".into(),
        ..Color::bt709_limited()
    };
    for component in ["prores", "vp9"] {
        let result = open_colored(
            &mut codec,
            component,
            &Settings::new(),
            &[PixelFormat::Rgba16],
            &ycgco,
        );
        match result {
            Err(MediaError::Codec(message)) => assert_eq!(
                message, "libswscale cannot convert RGB to the `ycgco` matrix",
                "{component}"
            ),
            other => panic!("{component} took ycgco: {:?}", other.map(|_| ())),
        }
    }
}

/// The ids `plugin.json` declares under `key`.
fn manifest_ids(key: &str) -> BTreeSet<String> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    manifest["media"][key]
        .as_array()
        .unwrap_or_else(|| panic!("plugin.json declares {key}"))
        .iter()
        .map(|entry| entry["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn describe_names_every_manifest_component() {
    let described = FfmpegCodec::new().describe().unwrap();
    for (kind, key) in [
        (ComponentKind::Decoder, "decoders"),
        (ComponentKind::VideoEncoder, "videoEncoders"),
        (ComponentKind::AudioEncoder, "audioEncoders"),
        (ComponentKind::Muxer, "muxers"),
    ] {
        let ids: BTreeSet<String> = described
            .iter()
            .filter(|component| component.kind == kind)
            .map(|component| component.id.clone())
            .collect();
        assert_eq!(ids, manifest_ids(key), "{key}");
    }
    for id in ["prores", "vp9"] {
        let encoder = described
            .iter()
            .find(|component| component.kind == ComponentKind::VideoEncoder && component.id == id)
            .unwrap();
        assert!(encoder.available, "{id}: {:?}", encoder.reason);
        assert_eq!(encoder.transports, ["sharedMemory", "pipe"], "{id}");
    }
    let audio = |id: &str| {
        described
            .iter()
            .find(|component| component.kind == ComponentKind::AudioEncoder && component.id == id)
            .unwrap()
    };
    for id in ["aac", "opus", "pcm"] {
        assert!(audio(id).available, "{id}: {:?}", audio(id).reason);
        assert_eq!(audio(id).channels, [1, 2], "{id}");
    }
    assert_eq!(
        audio("opus").sample_rates,
        Some(SampleRates::Only(vec![48_000]))
    );
    assert_eq!(audio("pcm").sample_rates, Some(SampleRates::Any));
    let Some(SampleRates::Only(rates)) = &audio("aac").sample_rates else {
        panic!("aac lists its rates");
    };
    assert!(
        rates.contains(&44_100) && rates.contains(&8_000),
        "{rates:?}"
    );
    let muxer = |id: &str| {
        described
            .iter()
            .find(|component| component.kind == ComponentKind::Muxer && component.id == id)
            .unwrap()
    };
    for id in ["mp4", "mov"] {
        assert!(muxer(id).available, "{id}");
        let keys: Vec<&str> = muxer(id)
            .settings
            .iter()
            .map(|field| field.key.as_str())
            .collect();
        assert_eq!(keys, ["faststart"], "{id}");
    }
    assert!(muxer("webm").available);
    assert!(muxer("webm").settings.is_empty());
}
