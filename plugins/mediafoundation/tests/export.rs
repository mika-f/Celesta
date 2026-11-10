//! Exports through the real exporter that combine this plugin's components
//! with the FFmpeg plugin's, both in this process and registered as Celesta
//! finds them (this plugin's folder first): what each file holds, and what
//! FFmpeg decodes of it.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use celesta_codec_ffmpeg::FfmpegBackend;
use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::report::{self, HintStyle};
use celesta_exporter::{
    AudioChoice, EncoderChoice, ExportError, ExportOptions, Exporter, OutputChoice,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{AudioDecoder, ComponentRef, MediaRegistry, VideoFrameDecoder};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};
use serde_json::Value;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;
/// 29.97 fps, so the software encoder's B-frames shift decode times by 1001s.
const RATE: Rational = Rational {
    numerator: 30_000,
    denominator: 1_001,
};
const SAMPLE_RATE: u32 = 48_000;
/// The AAC encoder's lead-in (its `delay`), in sample frames.
const AAC_DELAY: usize = 1_024;
/// The sample frames of the mix's end a looping export encodes before it
/// (the exporter's `LOOP_LEAD_IN_FRAMES`).
const LOOP_LEAD_IN: usize = 16 * 1024;

/// The colours of the two-colour project's left and right halves; full
/// channels, so every reader's transfer conversion keeps them.
const LEFT: [u8; 4] = [255, 0, 0, 255];
const RIGHT: [u8; 4] = [0, 0, 255, 255];
/// The transparent project's left half, straight; its right is undrawn.
const HALF_RED: [u8; 4] = [255, 0, 0, 128];

/// One exporter renders at a time, as in the FFmpeg plugin's export tests.
static GPU: Mutex<()> = Mutex::new(());

/// The GPU lock; a test that panicked while holding it does not fail the
/// rest.
fn gpu() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Makes both plugins, in this process, the media plugins of every export:
/// This plugin first, as its folder sorts before FFmpeg's, both bundled.
fn install() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(vec![
            celesta_codec_mediafoundation::registered(),
            celesta_codec_ffmpeg::registered(),
        ]))));
    });
}

/// Reports a test skipped for want of a GPU adapter, or fails it when
/// `CELESTA_REQUIRE_GPU=1` says the machine has one.
fn skip_without_gpu(error: &impl std::fmt::Display) {
    assert!(
        std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
        "GPU tests require an adapter: {error}"
    );
    eprintln!("skipping live export test: no GPU adapter is available: {error}");
}

/// `plugin/component` as a reference.
fn component(text: &str) -> ComponentRef {
    text.parse().unwrap()
}

/// The output choice of `--muxer`, `--video-encoder`, and `--audio-encoder`,
/// with no options; an encoder of `None` is the muxer's default.
fn choice(muxer: &str, video: Option<&str>, audio: Option<&str>) -> OutputChoice {
    let encoder = |name: Option<&str>| EncoderChoice {
        component: name.map(component),
        options: Default::default(),
    };
    OutputChoice {
        preset: None,
        muxer: Some(component(muxer)),
        video: encoder(video),
        audio: AudioChoice::Encode(encoder(audio)),
    }
}

/// Exports `project` to `name` in `dir` with `options`: the file and the
/// JSON report `Celesta-export --json` prints, or `None` without a GPU.
fn export(
    project: &Project,
    dir: &Path,
    name: &str,
    options: ExportOptions,
) -> Option<(PathBuf, Value)> {
    install();
    let output = dir.join(name);
    let exporter = Exporter::new(options);
    let mut no_gpu = None;
    let (report, _) = report::build(
        &dir.join("project.json"),
        None,
        HintStyle::Cli,
        |progress| {
            let result = exporter.export_project_with_progress(project, dir, &output, progress);
            if let Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) = &result {
                no_gpu = Some(error.to_string());
            }
            result
        },
    );
    if let Some(error) = no_gpu {
        skip_without_gpu(&error);
        return None;
    }
    let report = serde_json::to_value(&report).unwrap();
    assert_eq!(report["status"], "ok", "{name}: {report:#}");
    Some((output, report))
}

/// The muxer and the video and audio encoders the report names for its
/// only output.
fn reported(report: &Value) -> [&str; 3] {
    let output = &report["outputs"][0];
    [
        output["muxer"].as_str().unwrap(),
        output["video"]["component"].as_str().unwrap(),
        output["audio"]["component"].as_str().unwrap(),
    ]
}

/// An empty 64x36, 29.97 fps, one-second project at 48 kHz.
fn empty_project() -> Project {
    let mut project = Project::from_json(
        r#"{"version": 0, "settings": {"width": 1920, "height": 1080,
            "frameRate": {"numerator": 60, "denominator": 1}, "sampleRate": 48000},
            "assets": {}, "characters": {}, "tracks": [], "properties": {}}"#,
    )
    .unwrap();
    project.settings.width = WIDTH;
    project.settings.height = HEIGHT;
    project.settings.frame_rate = RATE;
    project.settings.sample_rate = SAMPLE_RATE;
    project.settings.duration = Some(Time::new(1, 1));
    project
}

/// A one-second item from 0 s.
fn item(id: &str, content: TimelineContent) -> TimelineItem {
    TimelineItem {
        id: id.to_owned(),
        name: None,
        range: TimeRange {
            start: Time::ZERO,
            duration: Time::new(1, 1),
        },
        content,
        enabled: None,
        transform: None,
        opacity: None,
        blend_mode: None,
        effects: None,
    }
}

/// A track of `kind` holding `item`.
fn track(id: &str, kind: TrackKind, item: TimelineItem) -> Track {
    Track {
        id: id.to_owned(),
        name: id.to_owned(),
        kind,
        enabled: None,
        locked: None,
        muted: None,
        solo: None,
        items: vec![item],
    }
}

/// [`empty_project`] showing, for its whole second, `left` over the
/// canvas's left half and `right` over its right half (straight RGBA).
///
/// Images are centred on the canvas origin, so the image is twice the
/// canvas and its bottom-right quarter is what shows.
fn halves_project(dir: &Path, left: [u8; 4], right: [u8; 4]) -> Project {
    let (width, height) = (WIDTH * 2, HEIGHT * 2);
    let file = std::fs::File::create(dir.join("halves.png")).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let pixels: Vec<u8> = (0..height)
        .flat_map(|_| {
            (0..width).flat_map(|column| {
                if column < WIDTH + WIDTH / 2 {
                    left
                } else {
                    right
                }
            })
        })
        .collect();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    let mut project = empty_project();
    project.assets.insert(
        "halves".to_owned(),
        Asset::Image {
            name: None,
            source: AssetSource::File {
                path: "halves.png".to_owned(),
            },
        },
    );
    project.tracks.push(track(
        "images",
        TrackKind::Video,
        item(
            "halves",
            TimelineContent::Image {
                asset: "halves".to_owned(),
                width: None,
                height: None,
                fit: None,
            },
        ),
    ));
    project
}

/// The two-colour project: [`LEFT`] and [`RIGHT`].
fn two_color_project(dir: &Path) -> Project {
    halves_project(dir, LEFT, RIGHT)
}

/// The transparent project: [`HALF_RED`] on the left, nothing on the right.
fn transparent_project(dir: &Path) -> Project {
    halves_project(dir, HALF_RED, [0; 4])
}

/// The click: one second of mono silence at 48 kHz with a full-scale
/// sample at frame 4800.
fn click() -> Vec<f32> {
    let mut samples = vec![0.0; SAMPLE_RATE as usize];
    samples[4_800] = 1.0;
    samples
}

/// `project` with `samples` (mono, 48 kHz, one second) as a 16-bit WAV
/// `name` in `dir`, played from 0 s.
fn with_audio(mut project: Project, dir: &Path, samples: &[f32], name: &str) -> Project {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16).to_le_bytes())
        .collect();
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes()); // PCM
    wav.extend(1u16.to_le_bytes()); // mono
    wav.extend(SAMPLE_RATE.to_le_bytes());
    wav.extend((SAMPLE_RATE * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(dir.join(name), wav).unwrap();
    project.assets.insert(
        "sound".to_owned(),
        Asset::Audio {
            name: None,
            source: AssetSource::File {
                path: name.to_owned(),
            },
        },
    );
    project.tracks.push(track(
        "audio",
        TrackKind::Audio,
        item(
            "sound",
            TimelineContent::Audio {
                asset: "sound".to_owned(),
                source_range: None,
                playback_rate: None,
                volume: None,
                muted: None,
                pitch: None,
                preserve_pitch: None,
                fade_in: None,
                fade_out: None,
                duck_group: None,
                duck: None,
            },
        ),
    ));
    project
}

/// The type of each track's first sample entry in `path` (the entry after
/// every `stsd`'s version, flags, count, and entry size).
fn sample_entries(path: &Path) -> Vec<[u8; 4]> {
    let bytes = std::fs::read(path).unwrap();
    bytes
        .windows(4)
        .enumerate()
        .filter(|(_, window)| *window == b"stsd")
        .filter_map(|(at, _)| bytes.get(at + 16..at + 20)?.try_into().ok())
        .collect()
}

/// The packets libavformat reads of `path`'s video stream.
fn video_packets(path: &Path) -> usize {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: the context and packet are opened, read, and freed here; the
    // stream pointers are valid while the context is open.
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
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let video = streams
            .iter()
            .position(|&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO
            })
            .expect("a video stream") as i32;
        let mut count = 0;
        let mut packet = ff::av_packet_alloc();
        while ff::av_read_frame(context, packet) >= 0 {
            if (*packet).stream_index == video {
                count += 1;
            }
            ff::av_packet_unref(packet);
        }
        ff::av_packet_free(&mut packet);
        ff::avformat_close_input(&mut context);
        count
    }
}

/// `path`'s audio stream's `(start_time, duration, time_base)` as
/// libavformat reads them, past the edit list.
fn audio_timing(path: &Path) -> (i64, i64, (i32, i32)) {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: as in `video_packets`.
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
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let audio = streams
            .iter()
            .find(|&&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_AUDIO
            })
            .expect("an audio stream");
        let timing = (
            (**audio).start_time,
            (**audio).duration,
            ((**audio).time_base.num, (**audio).time_base.den),
        );
        ff::avformat_close_input(&mut context);
        timing
    }
}

/// The first channel of `path`'s audio as libavcodec decodes every packet
/// stored, the edit list ignored: what precedes the mix included.
fn stored_audio(path: &Path) -> Vec<f32> {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: the context, options, codec context, packet, and frame are
    // made, read, and freed here; a decoded AAC frame is planar `f32`
    // (`fltp`), `nb_samples` of them in its first plane.
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
        let index = streams
            .iter()
            .position(|&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_AUDIO
            })
            .expect("an audio stream");
        let parameters = (*streams[index]).codecpar;
        let decoder = ff::avcodec_find_decoder((*parameters).codec_id);
        let mut codec = ff::avcodec_alloc_context3(decoder);
        assert!(ff::avcodec_parameters_to_context(codec, parameters) >= 0);
        assert_eq!(ff::avcodec_open2(codec, decoder, std::ptr::null_mut()), 0);
        let mut packet = ff::av_packet_alloc();
        let mut frame = ff::av_frame_alloc();
        let mut samples = Vec::new();
        let receive = |samples: &mut Vec<f32>| {
            while ff::avcodec_receive_frame(codec, frame) == 0 {
                assert_eq!(
                    (*frame).format,
                    ff::AVSampleFormat::AV_SAMPLE_FMT_FLTP as i32
                );
                samples.extend_from_slice(std::slice::from_raw_parts(
                    (*frame).data[0].cast::<f32>(),
                    (*frame).nb_samples as usize,
                ));
            }
        };
        while ff::av_read_frame(context, packet) >= 0 {
            if (*packet).stream_index == index as i32 {
                assert_eq!(ff::avcodec_send_packet(codec, packet), 0);
                receive(&mut samples);
            }
            ff::av_packet_unref(packet);
        }
        assert_eq!(ff::avcodec_send_packet(codec, std::ptr::null()), 0);
        receive(&mut samples);
        ff::av_frame_free(&mut frame);
        ff::av_packet_free(&mut packet);
        ff::avcodec_free_context(&mut codec);
        ff::avformat_close_input(&mut context);
        samples
    }
}

/// Every frame of `path` as the FFmpeg plugin decodes it, RGBA.
fn decoded_frames(path: &Path) -> Vec<Vec<u8>> {
    let mut decoder = FfmpegBackend::new();
    (0..video_packets(path))
        .map(|index| {
            let time = index as f64 * f64::from(RATE.denominator) / f64::from(RATE.numerator);
            let frame = decoder.decode_frame(path, time).unwrap();
            assert_eq!(
                (frame.width, frame.height),
                (WIDTH, HEIGHT),
                "frame {index}"
            );
            frame.pixels.to_vec()
        })
        .collect()
}

/// The pixel at `(column, row)` of a 64-wide RGBA frame.
fn pixel(pixels: &[u8], column: u32, row: u32) -> [u8; 4] {
    let start = ((row * WIDTH + column) * 4) as usize;
    pixels[start..start + 4].try_into().unwrap()
}

/// Panics unless every frame of `path` shows `left` and `right` (within
/// `tolerance`) away from the halves' edge, and it has the 30 frames of a
/// second.
fn assert_halves_within(path: &Path, left: [u8; 4], right: [u8; 4], tolerance: u8) {
    let frames = decoded_frames(path);
    assert_eq!(frames.len(), 30, "{}", path.display());
    for (index, frame) in frames.iter().enumerate() {
        for row in [2, HEIGHT / 2, HEIGHT - 3] {
            for (column, expected) in [(4, left), (24, left), (40, right), (WIDTH - 5, right)] {
                let actual = pixel(frame, column, row);
                let distance = actual
                    .iter()
                    .zip(expected)
                    .map(|(actual, expected)| actual.abs_diff(expected))
                    .max()
                    .unwrap();
                assert!(
                    distance <= tolerance,
                    "{} frame {index} at ({column}, {row}): {actual:?}, not {expected:?}",
                    path.display()
                );
            }
        }
    }
}

/// [`assert_halves_within`] 3, as every export test but HEVC's.
fn assert_halves(path: &Path, left: [u8; 4], right: [u8; 4]) {
    assert_halves_within(path, left, right, 3);
}

/// `path`'s audio as the FFmpeg plugin decodes it, at 48 kHz in `channels`.
fn decoded_audio(path: &Path, channels: u16) -> Vec<f32> {
    FfmpegBackend::new()
        .decode_audio(path, SAMPLE_RATE, channels)
        .unwrap()
        .samples
}

/// Panics unless `path`'s audio decodes to the video's second, give or take
/// the last packet's padding.
fn assert_audio_lasts_the_video(path: &Path) {
    let frames = decoded_audio(path, 2).len() / 2;
    assert!(
        (47_952..=49_024).contains(&frames),
        "{}: {frames} frames of audio",
        path.display()
    );
}

/// Whether this system's Media Foundation has the HEVC encoder, which needs
/// a GPU driver's or the Store's extension; prints why `test` skips when it
/// has not.
fn hevc_available(test: &str) -> bool {
    use celesta_media::{ComponentKind, MediaProvider};
    let described = celesta_codec_mediafoundation::MfCodec::new()
        .describe()
        .unwrap();
    let hevc = described
        .iter()
        .find(|component| component.kind == ComponentKind::VideoEncoder && component.id == "hevc")
        .expect("hevc is described");
    if !hevc.available {
        eprintln!(
            "skipping {test}: {}",
            hevc.reason.as_deref().unwrap_or("no reason")
        );
    }
    hevc.available
}

#[test]
fn mediafoundation_h264_with_ffmpeg_aac_in_ffmpeg_mp4() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = with_audio(
        two_color_project(directory.path()),
        directory.path(),
        &click(),
        "click.wav",
    );
    let Some((output, report)) = export(
        &project,
        directory.path(),
        "film.mp4",
        ExportOptions {
            output: choice(
                "ffmpeg/mp4",
                Some("mediafoundation/h264"),
                Some("ffmpeg/aac"),
            ),
            ..ExportOptions::default()
        },
    ) else {
        return;
    };
    assert_eq!(
        reported(&report),
        ["ffmpeg/mp4", "mediafoundation/h264", "ffmpeg/aac"]
    );
    assert_eq!(sample_entries(&output), [*b"avc1", *b"mp4a"]);
    assert_halves(&output, LEFT, RIGHT);
    assert_audio_lasts_the_video(&output);
}

#[test]
fn ffmpeg_x264_with_mediafoundation_aac_in_ffmpeg_mp4() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = with_audio(
        two_color_project(directory.path()),
        directory.path(),
        &click(),
        "click.wav",
    );
    let Some((output, report)) = export(
        &project,
        directory.path(),
        "film.mp4",
        ExportOptions {
            output: choice(
                "ffmpeg/mp4",
                Some("ffmpeg/libx264"),
                Some("mediafoundation/aac"),
            ),
            ..ExportOptions::default()
        },
    ) else {
        return;
    };
    assert_eq!(
        reported(&report),
        ["ffmpeg/mp4", "ffmpeg/libx264", "mediafoundation/aac"]
    );
    assert_eq!(sample_entries(&output), [*b"avc1", *b"mp4a"]);
    assert_halves(&output, LEFT, RIGHT);
    assert_audio_lasts_the_video(&output);
}

#[test]
fn mediafoundation_hevc_with_ffmpeg_opus_in_ffmpeg_mp4() {
    const TEST: &str = "mediafoundation_hevc_with_ffmpeg_opus_in_ffmpeg_mp4";
    if !hevc_available(TEST) {
        return;
    }
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = with_audio(
        two_color_project(directory.path()),
        directory.path(),
        &click(),
        "click.wav",
    );
    let Some((output, report)) = export(
        &project,
        directory.path(),
        "film.mp4",
        ExportOptions {
            output: choice(
                "ffmpeg/mp4",
                Some("mediafoundation/hevc"),
                Some("ffmpeg/opus"),
            ),
            ..ExportOptions::default()
        },
    ) else {
        return;
    };
    assert_eq!(
        reported(&report),
        ["ffmpeg/mp4", "mediafoundation/hevc", "ffmpeg/opus"]
    );
    let codec = report["outputs"][0]["video"]["codec"].as_str().unwrap();
    assert!(codec.starts_with("hvc1."), "{codec}");
    assert_eq!(sample_entries(&output), [*b"hvc1", *b"Opus"]);
    // Microsoft's HEVC encoders write no colour description into the SPS,
    // only the muxer's `colr` box says BT.709, and libavcodec's HEVC decoder
    // ignores that box and converts to RGB as BT.601: full red decodes to
    // 231 here, not 255. The decoded colours are therefore only checked to
    // within 32.
    assert_halves_within(&output, LEFT, RIGHT, 32);
    assert_audio_lasts_the_video(&output);
}

#[test]
fn mediafoundation_alone_with_ffmpeg_mp4() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = with_audio(
        two_color_project(directory.path()),
        directory.path(),
        &click(),
        "click.wav",
    );
    let Some((output, report)) = export(
        &project,
        directory.path(),
        "film.mp4",
        ExportOptions {
            output: choice(
                "ffmpeg/mp4",
                Some("mediafoundation/h264"),
                Some("mediafoundation/aac"),
            ),
            ..ExportOptions::default()
        },
    ) else {
        return;
    };
    assert_eq!(
        reported(&report),
        ["ffmpeg/mp4", "mediafoundation/h264", "mediafoundation/aac"]
    );
    assert_eq!(sample_entries(&output), [*b"avc1", *b"mp4a"]);
    assert_halves(&output, LEFT, RIGHT);
    assert_audio_lasts_the_video(&output);
}

#[test]
fn a_looping_export_with_mediafoundation_aac_starts_from_its_end() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    // The FFmpeg plugin's hits: four a second, each a chord at full
    // strength from its first sample. Played on repeat, the last runs into
    // the first as each runs into the next. One second is not a whole
    // number of AAC frames.
    let hit = |t: f64| {
        let tau = std::f64::consts::TAU;
        0.2 * (-40.0 * t).exp()
            * ((tau * 1000.0 * t).cos()
                + (tau * 2500.0 * t).sin()
                + (tau * 4400.0 * t).sin()
                + (tau * 6000.0 * t).cos())
    };
    let hits: Vec<f32> = (0..SAMPLE_RATE as usize)
        .map(|n| hit((n % 12_000) as f64 / f64::from(SAMPLE_RATE)) as f32)
        .collect();
    let project = with_audio(
        transparent_project(directory.path()),
        directory.path(),
        &hits,
        "hits.wav",
    );
    let export_looping = |looping, name: &str| {
        export(
            &project,
            directory.path(),
            name,
            ExportOptions {
                looping,
                output: choice("ffmpeg/mp4", None, Some("mediafoundation/aac")),
                ..ExportOptions::default()
            },
        )
        .map(|(output, report)| {
            assert_eq!(
                reported(&report),
                ["ffmpeg/mp4", "ffmpeg/libx264", "mediafoundation/aac"]
            );
            output
        })
    };
    let (Some(once), Some(looped)) = (
        export_looping(false, "once.mp4"),
        export_looping(true, "looped.mp4"),
    ) else {
        return;
    };

    // The edit list hides the lead-in: the sound still starts at 0 and lasts
    // exactly the video's second.
    let (start_time, duration, (num, den)) = audio_timing(&looped);
    assert_eq!(start_time, 0);
    assert_eq!(
        duration * i64::from(num),
        i64::from(den),
        "the sound lasts {duration} x {num}/{den} s"
    );
    // Read past the edit list, the file keeps the lead-in: the mix's last
    // 16384 frames, after the encoder's priming and before the mix.
    let (once_stored, looped_stored) = (stored_audio(&once), stored_audio(&looped));
    assert_eq!(looped_stored.len() - once_stored.len(), LOOP_LEAD_IN);
    // The mix carries the mono source at -3 dB in each channel, which the
    // FFmpeg plugin's downmix to mono restores.
    let looped_stored: Vec<f32> = looped_stored
        .into_iter()
        .map(|sample| sample * std::f32::consts::SQRT_2)
        .collect();
    let (once, looped) = (decoded_audio(&once, 1), decoded_audio(&looped, 1));
    assert_eq!(looped.len(), once.len(), "the lead-in decodes to nothing");

    // Each hit's first 1024 samples, from `start` in `decoded`, against the
    // source from mix frame `mix`, relative to their energy.
    let hit_error = |decoded: &[f32], start: usize, mix: usize| -> f64 {
        let (mut error, mut energy) = (0.0, 0.0);
        for (n, &sample) in decoded.iter().skip(start).take(1024).enumerate() {
            let expected = hit(((mix + n) % 12_000) as f64 / f64::from(SAMPLE_RATE));
            error += (f64::from(sample) - expected).powi(2);
            energy += expected.powi(2);
        }
        error / energy
    };
    let later = [12_000, 24_000, 36_000].map(|start| hit_error(&looped, start, start));
    let worst_later = later.into_iter().fold(0.0, f64::max);
    // The mix's hit at 36000 is 12000 frames before the end, so in the
    // lead-in 4384 frames after the priming.
    let lead_in_hit = hit_error(&looped_stored, AAC_DELAY + LOOP_LEAD_IN - 12_000, 36_000);
    let (first_once, first_looped) = (hit_error(&once, 0, 0), hit_error(&looped, 0, 0));
    assert!(
        lead_in_hit < 3.0 * worst_later,
        "the lead-in's hit is off by {lead_in_hit}, the mix's by up to {worst_later}"
    );
    // From the end of the loop, the first hit comes out like the others.
    assert!(
        first_looped < worst_later,
        "with the lead-in the first hit is off by {first_looped}, the later ones by up to {worst_later}"
    );
    // The AAC encoder at a constant bitrate does not smear a hit encoded
    // from silence either (unlike FFmpeg's, whose test asserts that it
    // does), so the export without the lead-in starts about as well (here
    // 0.00021 against later hits up to 0.0003; the margin is for other
    // encoders, such as a machine's with another Media Foundation AAC).
    assert!(
        first_once <= 3.0 * worst_later,
        "without the lead-in the first hit is off by {first_once}, the later ones by up to {worst_later}"
    );
}

#[test]
fn ffmpeg_stays_the_default_beside_mediafoundation() {
    let _gpu = gpu();
    install();
    let registry = celesta_media::registry().expect("a registry");
    let ids: Vec<&str> = registry
        .plugins()
        .iter()
        .map(|plugin| plugin.id.as_str())
        .collect();
    assert_eq!(ids, ["mediafoundation", "ffmpeg"]);

    let directory = tempfile::tempdir().unwrap();
    let project = with_audio(
        two_color_project(directory.path()),
        directory.path(),
        &click(),
        "click.wav",
    );
    let Some((_, report)) = export(
        &project,
        directory.path(),
        "film.mp4",
        ExportOptions::default(),
    ) else {
        return;
    };
    assert_eq!(
        reported(&report),
        ["ffmpeg/mp4", "ffmpeg/libx264", "ffmpeg/aac"]
    );
}
