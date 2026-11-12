//! The standard FFmpeg plugin checked through the real exporter, in this
//! process and linked against the standard (LGPL) build: what it writes for
//! ProRes MOV and VP9 WebM (codecs, colors, alpha, audio streams), and what
//! its decoder reads back. The exporter's own tests run on the test codec;
//! these hold the contract with FFmpeg. MP4, whose H.264 is the x264
//! plugin's, is tested there.

#[path = "support/exporting.rs"]
mod exporting;
#[path = "support/files.rs"]
mod files;

use celesta_codec_ffmpeg::{FfmpegBackend, FfmpegCodec};
use celesta_exporter::report::{self, HintStyle};
use celesta_exporter::{
    AudioChoice, EncoderChoice, ExportError, ExportOptions, LoudnessTarget, OutputChoice,
};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{AudioDecoder, ComponentKind, MediaProvider, measure_loudness};
use exporting::{
    component, export_as, export_reporting, export_with, exporter, gpu, hits_project, mix_frames,
    skip_without_gpu, transparent_project, with_audio,
};
use ez_ffmpeg::stream_info::{StreamInfo, find_audio_stream_info, find_video_stream_info};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use files::{audio_stream, colr_atom, decoded_audio_seconds, stream_color};
use std::path::Path;

/// Registers the standard plugin alone, in this process, as Celesta does
/// when no x264 download is installed.
fn install() {
    celesta_codec_ffmpeg::install();
}

#[test]
fn the_ffmpeg_codec_has_every_format_encoder() {
    let described = FfmpegCodec::new().describe().unwrap();
    for (kind, id) in [
        (ComponentKind::VideoEncoder, "prores"),
        (ComponentKind::VideoEncoder, "vp9"),
        (ComponentKind::AudioEncoder, "aac"),
        (ComponentKind::AudioEncoder, "opus"),
        (ComponentKind::AudioEncoder, "pcm"),
        (ComponentKind::Muxer, "mp4"),
        (ComponentKind::Muxer, "mov"),
        (ComponentKind::Muxer, "webm"),
    ] {
        let component = described
            .iter()
            .find(|component| component.kind == kind && component.id == id)
            .unwrap_or_else(|| panic!("{id} is described"));
        assert!(component.available, "{id}: {:?}", component.reason);
    }
}

/// The first video frame of `path` as `rgba64le` samples, decoded with
/// `decoder` when given (FFmpeg's native VP9 decoder drops the alpha plane).
fn decode_rgba64(path: &Path, decoder: Option<&str>) -> Vec<u16> {
    let raw = path.with_extension("rgba64");
    let mut input = Input::from(path.to_string_lossy().into_owned());
    if let Some(decoder) = decoder {
        input = input.set_video_codec(decoder);
    }
    FfmpegContext::builder()
        .input(input)
        .output(
            Output::from(raw.to_string_lossy().into_owned())
                .add_stream_map("0:v:0")
                .set_format("rawvideo")
                .set_pix_fmt("rgba64le")
                .set_max_video_frames(1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    std::fs::read(raw)
        .unwrap()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&sample| u16::from_le_bytes(sample))
        .collect()
}

fn pixel(pixels: &[u16], x: usize, y: usize) -> [u16; 4] {
    let start = (y * 64 + x) * 4;
    pixels[start..start + 4].try_into().unwrap()
}

/// The video stream's codec, `(color space, primaries, transfer)` and
/// metadata.
fn video_stream(
    url: &str,
) -> (
    String,
    (i32, i32, i32),
    std::collections::HashMap<String, String>,
) {
    let Some(StreamInfo::Video {
        codec_name,
        color_space,
        color_primaries,
        color_transfer,
        metadata,
        ..
    }) = find_video_stream_info(url).unwrap()
    else {
        panic!("exported file has no video stream");
    };
    (
        codec_name,
        (color_space, color_primaries, color_transfer),
        metadata,
    )
}

/// Whether `path` has a Matroska `Colour` element (`55 B0`) before its first
/// Cluster (`1F 43 B6 75`).
fn webm_has_colour(path: &Path) -> bool {
    let bytes = std::fs::read(path).unwrap();
    let cluster = bytes
        .windows(4)
        .position(|window| window == [0x1F, 0x43, 0xB6, 0x75])
        .expect("a cluster");
    bytes[..cluster]
        .windows(2)
        .any(|window| window == [0x55, 0xB0])
}

/// The data of `path`'s first video packet.
fn first_video_packet(path: &Path) -> Vec<u8> {
    use ffmpeg_sys_next as ff;
    let url = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
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
        let streams =
            std::slice::from_raw_parts((*context).streams, (*context).nb_streams as usize);
        let video = streams
            .iter()
            .position(|&stream| {
                (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO
            })
            .expect("a video stream") as i32;
        let mut packet = ff::av_packet_alloc();
        let data = loop {
            assert!(ff::av_read_frame(context, packet) >= 0, "no video packet");
            if (*packet).stream_index == video {
                break std::slice::from_raw_parts((*packet).data, (*packet).size as usize).to_vec();
            }
            ff::av_packet_unref(packet);
        };
        ff::av_packet_free(&mut packet);
        ff::avformat_close_input(&mut context);
        data
    }
}

/// Every pixel of the red quadrant, the top-left 32x32, keeps its half
/// alpha and straight color (within `tolerance` 16-bit codes of the color);
/// every undrawn pixel is fully transparent. The quadrant's last
/// `blended_rows` rows decode with 4:2:0 chroma upsampled from the
/// background's below, so there red need only still dominate.
fn assert_transparent_quadrant(pixels: &[u16], tolerance: u16, blended_rows: usize) {
    assert_eq!(pixels.len(), 64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let sample = pixel(pixels, x, y);
            if x < 32 && y < 32 {
                assert!(
                    sample[3].abs_diff(128 * 257) <= 2 * 257,
                    "({x}, {y}): {sample:?}"
                );
                let color = if y < 32 - blended_rows {
                    sample[0].abs_diff(65535) <= tolerance && sample[1] <= tolerance
                } else {
                    sample[0] > 4 * sample[1].max(sample[2])
                };
                assert!(color, "({x}, {y}): {sample:?}");
            } else {
                assert_eq!(sample[3], 0, "({x}, {y}): {sample:?}");
            }
        }
    }
}

#[test]
fn exports_transparent_prores_4444() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.mov") else {
        return;
    };

    let url = output.to_string_lossy().into_owned();
    // AVCOL_SPC_BT709, AVCOL_PRI_BT709 and AVCOL_TRC_BT709 are all 1.
    let (codec_name, colors, _) = video_stream(&url);
    assert_eq!(codec_name, "prores");
    assert_eq!(colors, (1, 1, 1));
    assert_eq!(audio_stream(&url), ("pcm_s24le".to_owned(), 8_000));
    assert_transparent_quadrant(&decode_rgba64(&output, None), 3 * 257, 0);
}

#[test]
fn every_format_muxes_audio_as_long_as_the_video() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    // MP4's is the x264 plugin's test.
    for name in ["clip.mov", "clip.webm"] {
        let Some(output) = export_as(&project, directory.path(), name) else {
            return;
        };
        // As long as MP4's, whose AAC rounds the last frame up to 1024
        // samples (0.128 s at 8 kHz).
        let seconds = decoded_audio_seconds(&output);
        assert!(
            (0.999..1.129).contains(&seconds),
            "{name}: {seconds} s of audio"
        );
    }
}

#[test]
fn brings_the_mix_to_a_loudness_target() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let mut project = transparent_project(directory.path());
    project.settings.sample_rate = 48_000;
    // A quiet tone under short, loud hits: at -14 LUFS the hits would go
    // over the -1 dBTP ceiling, so the limiter holds them there.
    let project = with_audio(
        project,
        directory.path(),
        "aevalsrc=exprs='0.02*sin(2*PI*440*t)+0.5*sin(2*PI*2000*t)*lt(mod(t,0.25),0.005)':sample_rate=48000:duration=1",
        "tone.wav",
    );
    let options = || ExportOptions {
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    };
    let Some((first, reported, warnings)) =
        export_reporting(&project, directory.path(), options(), "first.mov")
    else {
        return;
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    let reported = reported.expect("a video with sound reports its loudness");
    let decode = |path: &Path| FfmpegBackend::new().decode_audio(path, 48_000, 2).unwrap();
    let decoded = decode(&first);
    let measured = measure_loudness(&decoded);

    // MOV keeps the mix as 24-bit PCM, so the file measures as reported.
    let integrated = measured.integrated_lufs.unwrap();
    assert!((integrated + 14.0).abs() <= 0.1, "{integrated} LUFS");
    assert!((integrated - reported.integrated_lufs.unwrap()).abs() <= 0.01);
    let peak = measured.true_peak_dbtp.unwrap();
    assert!(peak <= -1.0 + 1e-3, "{peak} dBTP");
    // The gain alone would have taken the hits over the ceiling.
    let source = measure_loudness(&decode(&directory.path().join("tone.wav")));
    let gain = reported.gain_db.unwrap();
    assert!(source.true_peak_dbtp.unwrap() + gain > 0.0, "{gain} dB");

    // The same project comes out the same.
    let (second, ..) =
        export_reporting(&project, directory.path(), options(), "second.mov").unwrap();
    assert!(decode(&second).samples == decoded.samples);
    // AAC in MP4 is the x264 plugin's test.
}

#[test]
fn mov_and_webm_are_tagged_bt709() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    // Primaries, transfer, matrix (all AVCOL_*_BT709, 1), and range
    // (AVCOL_RANGE_MPEG, 1), as libavformat reads them back. MP4's is the
    // x264 plugin's test.
    for name in ["clip.mov", "clip.webm"] {
        let Some(output) = export_as(&project, directory.path(), name) else {
            return;
        };
        assert_eq!(stream_color(&output), (1, 1, 1, 1), "{name}");
        match name {
            "clip.mov" => assert_eq!(colr_atom(&output), Some(*b"nclc")),
            _ => assert!(webm_has_colour(&output)),
        }
    }
}

#[test]
fn vp9_frames_carry_bt709_in_their_header() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.webm") else {
        return;
    };
    // The keyframe header's color_space, 2 for BT.709, so a player that
    // ignores the container's Colour still decodes BT.709.
    assert_eq!(first_video_packet(&output)[4] >> 5, 2);
}

#[test]
fn a_looping_webm_is_written_as_without_loop() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let export = |looping, name| {
        export_with(
            &project,
            directory.path(),
            name,
            ExportOptions {
                looping,
                ..ExportOptions::default()
            },
        )
    };
    let (Some(once), Some(looped)) = (export(false, "once.webm"), export(true, "looped.webm"))
    else {
        return;
    };
    let decode = |path: &Path| FfmpegBackend::new().decode_audio(path, 48_000, 2).unwrap();
    assert!(decode(&once).samples == decode(&looped).samples);
}

#[test]
fn a_looping_mov_decodes_as_without_loop() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let export = |looping, name| {
        export_with(
            &project,
            directory.path(),
            name,
            ExportOptions {
                looping,
                ..ExportOptions::default()
            },
        )
    };
    let (Some(once), Some(looped)) = (export(false, "once.mov"), export(true, "looped.mov")) else {
        return;
    };
    assert_eq!(
        audio_stream(&looped.to_string_lossy()),
        ("pcm_s24le".to_owned(), 48_000)
    );
    // The edit list hides the PCM lead-in: the sound still starts at 0 and
    // lasts exactly the video's second.
    let Some(StreamInfo::Audio {
        start_time,
        duration,
        time_base,
        ..
    }) = find_audio_stream_info(looped.to_string_lossy().into_owned()).unwrap()
    else {
        panic!("the looping export has no audio stream");
    };
    assert_eq!(start_time, 0);
    assert_eq!(
        duration * i64::from(time_base.num),
        i64::from(time_base.den),
        "the sound lasts {duration} x {}/{} s",
        time_base.num,
        time_base.den
    );
    // The looping file holds the lead-in, ...
    assert_ne!(
        std::fs::read(&once).unwrap(),
        std::fs::read(&looped).unwrap(),
        "the looping MOV has no lead-in"
    );
    // ... which decodes to nothing: PCM has no priming to smear, so the
    // rest is the same samples.
    let decode = |path: &Path| FfmpegBackend::new().decode_audio(path, 48_000, 2).unwrap();
    let (once, looped) = (decode(&once), decode(&looped));
    assert_eq!(looped.frame_count(), once.frame_count());
    assert!(looped.samples == once.samples);
}

#[test]
fn mov_starts_with_an_edit_list_hiding_the_delay() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    // The MOV with AAC, so it hides AAC's 1024 priming samples as MP4 does
    // (the x264 plugin's test).
    let aac = || ExportOptions {
        output: OutputChoice {
            audio: AudioChoice::Encode(EncoderChoice {
                component: Some(component("ffmpeg/aac")),
                options: Default::default(),
            }),
            ..OutputChoice::default()
        },
        ..ExportOptions::default()
    };
    let name = "clip.mov";
    let Some(output) = export_with(&project, directory.path(), name, aac()) else {
        return;
    };
    let Some(StreamInfo::Audio {
        codec_name,
        start_time,
        ..
    }) = find_audio_stream_info(output.to_string_lossy().into_owned()).unwrap()
    else {
        panic!("{name} has no audio stream");
    };
    assert_eq!(codec_name, "aac", "{name}");
    assert_eq!(start_time, 0, "{name}");
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48_000, 2)
        .unwrap();
    assert_eq!(decoded.frame_count(), mix.frame_count(), "{name}");
}

#[test]
fn opus_webm_declares_its_codec_delay() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    let Some(output) = export_as(&project, directory.path(), "clip.webm") else {
        return;
    };
    assert_eq!(
        audio_stream(&output.to_string_lossy()),
        ("opus".to_owned(), 48_000)
    );
    // Without CodecDelay the decoder would keep libopus's 312 priming
    // samples.
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48_000, 2)
        .unwrap();
    let difference = decoded.frame_count().abs_diff(mix.frame_count());
    assert!(
        difference < 960,
        "{} decoded frames for a mix of {}",
        decoded.frame_count(),
        mix.frame_count()
    );
}

#[test]
fn pcm_mov_decodes_to_the_mix_exactly() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = hits_project(directory.path());
    let mix = mix_frames(directory.path(), "hits.wav", 48_000);
    let Some(output) = export_as(&project, directory.path(), "clip.mov") else {
        return;
    };
    assert_eq!(
        audio_stream(&output.to_string_lossy()),
        ("pcm_s24le".to_owned(), 48_000)
    );
    let decoded = FfmpegBackend::new()
        .decode_audio(&output, 48_000, 2)
        .unwrap();
    assert_eq!(decoded.samples.len(), mix.samples.len());
    let worst = decoded
        .samples
        .iter()
        .zip(&mix.samples)
        .map(|(decoded, mixed)| (decoded - mixed).abs())
        .fold(0.0, f32::max);
    assert!(worst <= 1.0 / 8_388_608.0, "off by up to {worst}");
}

#[test]
fn prores_and_webm_export_without_the_x264_plugin() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    for name in ["without-x264.mov", "without-x264.webm"] {
        let Some(path) = export_as(&project, directory.path(), name) else {
            return;
        };
        assert!(path.is_file());
    }
}

/// With the standard plugin alone (no x264 download, no native plugin), an
/// MP4 has no H.264 encoder: the export fails as unavailable, and the hint
/// names the download and `.mov`.
#[test]
fn an_mp4_without_the_x264_plugin_hints_at_the_download_and_mov() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let path = directory.path().join("without-x264.mp4");
    // `exporter` registers `celesta_codec_ffmpeg::registered()` alone.
    let error = match exporter(ExportOptions::default()).export_project(
        &project,
        directory.path(),
        &path,
    ) {
        Ok(()) => panic!("an MP4 exported without an H.264 encoder"),
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => error,
    };
    assert!(!path.exists());
    for style in [HintStyle::Cli, HintStyle::Mcp] {
        let failure = serde_json::to_value(report::failure(&error, style)).unwrap();
        assert_eq!(
            failure["error"]["code"], "component_unavailable",
            "{style:?}: {failure}"
        );
        let hint = report::hint(&error, style).unwrap_or_else(|| panic!("{error} has a hint"));
        assert!(hint.contains("ffmpeg-x264"), "{style:?}: {hint}");
        assert!(hint.contains(".mov"), "{style:?}: {hint}");
        assert_eq!(failure["error"]["hint"], hint.as_ref(), "{style:?}");
    }
}

#[test]
fn a_rendered_vp9_webm_preserves_the_transparent_quadrant() {
    let _gpu = gpu();
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let Some(output) = export_as(&project, directory.path(), "clip.webm") else {
        return;
    };

    let url = output.to_string_lossy().into_owned();
    let (codec_name, colors, metadata) = video_stream(&url);
    assert_eq!(codec_name, "vp9");
    // Players use alpha_mode to recognize the separate VP9 alpha payload;
    // Matroska tags read back upper-case (`ALPHA_MODE`).
    assert!(
        metadata
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("alpha_mode") && value == "1"),
        "{metadata:?}"
    );
    // AVCOL_SPC_BT709, AVCOL_PRI_BT709 and AVCOL_TRC_BT709 are all 1, and
    // the range limited (AVCOL_RANGE_MPEG, 1).
    assert_eq!(colors, (1, 1, 1));
    assert_eq!(stream_color(&output), (1, 1, 1, 1));
    // `mux_audio` always resamples Opus to 48 kHz.
    assert_eq!(audio_stream(&url), ("opus".to_owned(), 48_000));
    // libvpx keeps the alpha plane FFmpeg's native VP9 decoder drops. 4:2:0
    // chroma blurs the quadrant's color a little more than ProRes 4444's,
    // and libswscale's upsampling blends the background's chroma into the
    // quadrant's last row.
    assert_transparent_quadrant(&decode_rgba64(&output, Some("libvpx-vp9")), 6 * 257, 1);
}
