//! The built `celesta-codec-ffmpeg` program, driven through
//! `celesta_media::MediaProcess` as Celesta drives it: its answers equal the
//! in-process codec's, its errors keep their kind, an export written through
//! it reads back, and `--manifest` gives its manifest.

use celesta_codec_ffmpeg::{DECODER_ID, FfmpegCodec, registered};
use celesta_composition::Rational;
use celesta_media::{
    AudioDecoder, AudioEncoderRequest, Color, ComponentKind, Decoder, Encoded, MediaBackend,
    MediaError, MediaProcess, MediaProvider, MediaRegistry, MuxerRequest, Packet, PixelFormat,
    ProviderFactory, ProviderUse, RegisteredPlugin, SettingValue, Settings, Track, TrackConfig,
    TrackPacket, VideoEncoderOpened, VideoEncoderRequest, VideoFrameDecoder,
};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
}

/// The program, started as Celesta starts a media plugin for `purpose`.
fn started(purpose: ProviderUse) -> Box<dyn MediaProvider> {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    MediaProcess::factory(executable, directory)(&Default::default(), purpose).unwrap()
}

/// The program, started to decode.
fn provider() -> Box<dyn MediaProvider> {
    started(ProviderUse::Decode)
}

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// Opens libx264 on `program` for `width`x`height` RGBA8 frames at 10 fps.
fn open_x264(
    program: &mut dyn MediaProvider,
    given: &Settings,
    (width, height): (u32, u32),
) -> Result<VideoEncoderOpened, MediaError> {
    let color = Color::bt709_limited();
    program
        .video_encoder()
        .expect("the program has video encoders")
        .open_video(&VideoEncoderRequest {
            component: "libx264",
            settings: given,
            width,
            height,
            frame_rate: Rational::new(10, 1),
            timescale: 10,
            offered: &[PixelFormat::Rgba8],
            color: &color,
        })
}

/// The packets of `answers`, and the config the first of them gave.
fn gathered(answers: impl IntoIterator<Item = Encoded>) -> (Vec<Packet>, TrackConfig) {
    let mut packets = Vec::new();
    let mut config = None;
    for answer in answers {
        packets.extend(answer.packets);
        config = config.or(answer.config);
    }
    (packets, config.expect("the encoder gave its config"))
}

/// `frames` RGBA8 frames of `size` from red-less blue to red, encoded
/// with libx264 `ultrafast` on `program`: the video track and its packets.
fn encode_video(
    program: &mut dyn MediaProvider,
    size: (u32, u32),
    frames: u8,
) -> (Track, Vec<Packet>) {
    let opened = open_x264(program, &settings(&[("preset", text("ultrafast"))]), size).unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    let encoder = program.video_encoder().unwrap();
    let mut answers: Vec<Encoded> = (0..frames)
        .map(|frame| {
            let pixels = [frame * 20, 0, 255 - frame * 20, 255].repeat((size.0 * size.1) as usize);
            encoder
                .encode_video(opened.session, i64::from(frame), pixels)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_video(opened.session).unwrap());
    let (packets, config) = gathered(answers);
    let track = Track::Video {
        config,
        timescale: 10,
        width: size.0,
        height: size.1,
        frame_rate: Rational::new(10, 1),
        color: Color::bt709_limited(),
        alpha: false,
    };
    (track, packets)
}

/// Writes `tracks` and their packets, each track's in turn, to `output`
/// with `program`'s MP4 muxer.
fn mux_mp4(program: &mut dyn MediaProvider, output: &Path, tracks: Vec<(Track, Vec<Packet>)>) {
    let (tracks, packets): (Vec<Track>, Vec<Vec<Packet>>) = tracks.into_iter().unzip();
    let muxer = program.muxer().expect("the program has muxers");
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "mp4",
            settings: &Settings::new(),
            output,
            tracks: &tracks,
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<TrackPacket> = packets
        .into_iter()
        .enumerate()
        .flat_map(|(track, packets)| {
            packets
                .into_iter()
                .map(move |packet| TrackPacket { track, packet })
        })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    let written = muxer.finish_muxer(session).unwrap();
    assert_eq!(written.len(), 1, "{written:?}");
}

/// A backend on the FFmpeg plugin made by `factory`.
fn backend(factory: ProviderFactory) -> MediaBackend {
    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![RegisteredPlugin {
        factory,
        ..registered()
    }])))
}

/// A backend on the program.
fn program_backend() -> MediaBackend {
    let executable = executable();
    let directory = executable.parent().unwrap().to_path_buf();
    backend(MediaProcess::factory(executable, directory))
}

fn generate(path: &Path, lavfi: &str, codec: &str) {
    FfmpegContext::builder()
        .input(Input::from(lavfi).set_format("lavfi"))
        .output(Output::from(path.to_string_lossy().into_owned()).set_video_codec(codec))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
}

#[test]
fn the_program_answers_as_the_codec_in_process_does() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("clip.mkv");
    generate(&video, "testsrc2=size=16x8:rate=10:duration=1", "ffv1");
    let audio = directory.path().join("tone.wav");
    FfmpegContext::builder()
        .input(Input::from("sine=frequency=440:sample_rate=48000:duration=0.5").set_format("lavfi"))
        .output(Output::from(audio.to_string_lossy().into_owned()))
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();

    let mut remote = program_backend().with_sequential_video(Rational::new(10, 1));
    let mut local = backend(FfmpegCodec::factory()).with_sequential_video(Rational::new(10, 1));
    assert_eq!(remote.probe(&video).unwrap(), local.probe(&video).unwrap());
    assert_eq!(remote.probe(&audio).unwrap(), local.probe(&audio).unwrap());
    // Out of order, and a sequential run moving forward.
    for time in [0.5, 0.1, 0.2, 0.3, 0.9] {
        assert_eq!(
            remote.decode_frame(&video, time).unwrap(),
            local.decode_frame(&video, time).unwrap(),
            "{time}"
        );
        assert_eq!(
            remote.decode_frame_for("layer", &video, time).unwrap(),
            local.decode_frame_for("layer", &video, time).unwrap(),
            "{time}"
        );
    }
    assert_eq!(
        remote.decode_audio(&audio, 48_000, 2).unwrap(),
        local.decode_audio(&audio, 48_000, 2).unwrap()
    );
    assert_eq!(remote.video_duration(&video).unwrap(), Some(1.0));

    let missing = directory.path().join("missing.mp4");
    assert!(matches!(
        remote.probe(&missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    assert!(matches!(
        remote.decode_frame(&audio, 0.0),
        Err(MediaError::NoVideoStream(path)) if path == audio
    ));
}

#[test]
fn writes_an_export_through_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let (width, height) = (32, 16);
    let mut program = started(ProviderUse::Export);
    let described = program.describe().unwrap();
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap();
    assert!(libx264.available, "{:?}", libx264.reason);

    let video = encode_video(program.as_mut(), (width, height), 10);

    let encoder = program
        .audio_encoder()
        .expect("the program has audio encoders");
    let opened = encoder
        .open_audio(&AudioEncoderRequest {
            component: "aac",
            settings: &Settings::new(),
            sample_rate: 48_000,
            channels: 2,
        })
        .unwrap();
    assert_eq!(
        (opened.sample_rate, opened.channels, opened.delay),
        (48_000, 2, 1024)
    );
    let samples: Vec<f32> = (0..48_000)
        .flat_map(|frame| {
            let sample = (f64::from(frame) / 48_000.0 * 440.0 * std::f64::consts::TAU).sin() as f32;
            [sample * 0.5, sample * 0.5]
        })
        .collect();
    let mut answers: Vec<Encoded> = samples
        .chunks(1024 * 16 * 2)
        .enumerate()
        .map(|(chunk, samples)| {
            encoder
                .encode_audio(opened.session, chunk as i64 * 1024 * 16, samples)
                .unwrap()
        })
        .collect();
    answers.push(encoder.finish_audio(opened.session).unwrap());
    let (packets, config) = gathered(answers);
    assert_eq!(config.codec, "mp4a.40.2");
    // The first packet holds the priming samples, before 0.
    assert_eq!(packets[0].pts, -1024);
    let audio = Track::Audio {
        config,
        timescale: 48_000,
        sample_rate: 48_000,
        channels: 2,
        delay: opened.delay,
    };

    let output = directory.path().join("film.mp4");
    mux_mp4(program.as_mut(), &output, vec![video, (audio, packets)]);
    drop(program);

    let mut reader = program_backend();
    let probe = reader.probe(&output).unwrap().clone();
    let stream = probe.video.unwrap();
    assert_eq!((stream.width, stream.height), (width, height));
    assert_eq!(stream.codec.as_deref(), Some("h264"));
    assert_eq!(probe.audio[0].codec.as_deref(), Some("aac"));
    let last = reader.decode_frame(&output, 0.9).unwrap();
    let [red, _, blue, alpha] = last.pixels[..4] else {
        unreachable!()
    };
    assert!(red > 160 && blue < 90 && alpha == 255, "{red} {blue}");
    let decoded = reader.decode_audio(&output, 48_000, 2).unwrap();
    assert!((decoded.frame_count() as i64 - 48_000).abs() < 2_048);
}

/// The key and message of an [`MediaError::InvalidSettings`].
fn refused<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
    match result {
        Err(MediaError::InvalidSettings { key, .. }) => key,
        other => panic!("expected invalid settings, got {other:?}"),
    }
}

#[test]
fn settings_libx264_cannot_write_are_refused() {
    let mut program = started(ProviderUse::Export);
    let program = program.as_mut();
    let crf = settings(&[("crf", SettingValue::Number(99.0))]);
    assert_eq!(refused(open_x264(program, &crf, (2, 2))), "crf");
    let depth = settings(&[("bitDepth", text("12"))]);
    assert_eq!(refused(open_x264(program, &depth, (2, 2))), "bitDepth");
    // A CRF is not taken at an average bitrate, where its field is hidden.
    let hidden = settings(&[
        ("rateControl", text("vbr")),
        ("crf", SettingValue::Number(18.0)),
    ]);
    assert_eq!(refused(open_x264(program, &hidden, (2, 2))), "crf");
    // The bit depth is libx264's alone: ProRes has no such setting.
    let color = Color::bt709_limited();
    let prores = program
        .video_encoder()
        .unwrap()
        .open_video(&VideoEncoderRequest {
            component: "prores",
            settings: &depth,
            width: 2,
            height: 2,
            frame_rate: Rational::new(1, 1),
            timescale: 1,
            offered: &[PixelFormat::Rgba16],
            color: &color,
        });
    assert_eq!(refused(prores), "bitDepth");
}

#[test]
fn reads_and_writes_paths_relative_to_celestas_working_directory() {
    // Tests run in the package's folder; the program runs in its own.
    let directory = tempfile::tempdir_in(".").unwrap();
    let relative = Path::new(directory.path().file_name().unwrap());
    let video = relative.join("clip.mkv");
    generate(&video, "color=c=blue:s=4x4:r=5:d=1", "ffv1");
    let mut codec = program_backend();
    assert_eq!(
        codec.probe(&video).unwrap().video.as_ref().unwrap().width,
        4
    );
    assert_eq!(codec.decode_frame(&video, 0.2).unwrap().width, 4);
    let missing = relative.join("missing.mkv");
    assert!(matches!(
        codec.probe(&missing),
        Err(MediaError::NotFound(path)) if path == missing
    ));
    let mut program = started(ProviderUse::Export);
    let encoded = encode_video(program.as_mut(), (4, 4), 1);
    mux_mp4(program.as_mut(), &relative.join("out.mp4"), vec![encoded]);
    assert!(directory.path().join("out.mp4").is_file());
}

#[test]
fn the_program_answers_the_decoder_it_names() {
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("clip.mkv");
    generate(&video, "color=c=red:s=4x2:r=5:d=1", "ffv1");
    let mut process = provider();
    let process = process.decoder().unwrap();
    let mut local = FfmpegCodec::new();
    assert_eq!(
        process.probe(DECODER_ID, &video).unwrap(),
        local.probe(DECODER_ID, &video).unwrap()
    );
    let message = match process.probe("other", &video) {
        Err(MediaError::Codec(message)) => message,
        other => panic!("expected a codec error, got {other:?}"),
    };
    assert!(message.contains("no decoder"), "{message}");
}

#[test]
fn a_text_file_is_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    let text = directory.path().join("hello.mp4");
    std::fs::write(&text, "hello").unwrap();

    // The probe answers that FFmpeg does not read it, in the program as in
    // this process.
    assert!(matches!(
        provider().decoder().unwrap().probe(DECODER_ID, &text),
        Err(MediaError::Unsupported { path, .. }) if path == text
    ));
    let mut local = FfmpegCodec::new();
    assert!(matches!(
        local.probe(DECODER_ID, &text),
        Err(MediaError::Unsupported { path, .. }) if path == text
    ));
    // A decode does not: `unsupported` answers only a probe.
    let request = celesta_media::VideoRequest {
        path: &text,
        time: 0.0,
        sequential: None,
    };
    assert!(matches!(
        local.decode_video(DECODER_ID, &request),
        Err(MediaError::Codec(_))
    ));
    // No other decoder is installed, so Celesta says none could read it.
    assert!(matches!(
        program_backend().probe(&text),
        Err(MediaError::Unsupported { path, tried }) if path == text && tried == ["ffmpeg/ffmpeg"]
    ));
}

#[test]
fn manifest_flag_prints_the_media_manifest() {
    let output = std::process::Command::new(executable())
        .arg(celesta_plugin_host::MANIFEST_FLAG)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8(output.stdout).unwrap();
    assert_eq!(printed.lines().count(), 1, "{printed}");
    let manifest: serde_json::Value = serde_json::from_str(&printed).unwrap();
    assert_eq!(manifest["id"], "ffmpeg");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["roles"], serde_json::json!(["media"]));
    assert_eq!(manifest["media"]["decoders"][0]["id"], DECODER_ID);
    assert_eq!(manifest["media"]["decoders"][0]["name"], "FFmpeg");
    let extensions = manifest["media"]["decoders"][0]["extensions"]
        .as_array()
        .unwrap();
    assert!(extensions.contains(&"mp4".into()), "{extensions:?}");
}

#[test]
fn the_manifest_lists_every_kind() {
    let output = std::process::Command::new(executable())
        .arg(celesta_plugin_host::MANIFEST_FLAG)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let printed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    let ids = |manifest: &serde_json::Value, key: &str| -> Vec<String> {
        manifest["media"][key]
            .as_array()
            .unwrap_or_else(|| panic!("the manifest lists {key}"))
            .iter()
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect()
    };
    for key in ["videoEncoders", "audioEncoders", "muxers"] {
        let listed = ids(&printed, key);
        assert!(!listed.is_empty(), "{key}");
        assert_eq!(listed, ids(&source, key), "{key}");
    }
    assert_eq!(printed["media"]["muxers"][0]["id"], "mp4");
}
