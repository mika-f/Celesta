use super::*;
use celesta_codec_ffmpeg::FfmpegBackend;
use celesta_media::{
    DecodedColor, DecodedMatrix, DecodedPixelFormat as F, DecodedRange, Decoder, FrameTransport,
    MediaProcess, ProviderUse, SettingValue, VideoFrameDecoder,
};
use ffmpeg_sys_next as av;
use std::ffi::CString;

// ffmpeg-sys-next does not include libavcodec/bsf.h. This test-only layout
// and these functions match that public header in FFmpeg 8.1.
mod bsf {
    use ffmpeg_sys_next::{AVCodecParameters, AVPacket, AVRational};
    use std::ffi::{c_char, c_int, c_void};

    #[repr(C)]
    pub struct Context {
        av_class: *const c_void,
        filter: *const c_void,
        pub priv_data: *mut c_void,
        pub par_in: *mut AVCodecParameters,
        pub par_out: *mut AVCodecParameters,
        pub time_base_in: AVRational,
        pub time_base_out: AVRational,
    }

    unsafe extern "C" {
        pub fn av_bsf_get_by_name(name: *const c_char) -> *const c_void;
        pub fn av_bsf_alloc(filter: *const c_void, context: *mut *mut Context) -> c_int;
        pub fn av_bsf_init(context: *mut Context) -> c_int;
        pub fn av_bsf_send_packet(context: *mut Context, packet: *mut AVPacket) -> c_int;
        pub fn av_bsf_receive_packet(context: *mut Context, packet: *mut AVPacket) -> c_int;
        pub fn av_bsf_free(context: *mut *mut Context);
    }
}

fn generated(output: &Path, component: &str, ten: bool) -> Result<(), MediaError> {
    let mut codec = AvfCodec::default();
    let settings = if ten {
        [("bitDepth".into(), SettingValue::String("10".into()))]
            .into_iter()
            .collect()
    } else {
        Settings::new()
    };
    let encoder = codec.video_encoder().unwrap();
    let opened = encoder.open_video(&VideoEncoderRequest {
        component,
        settings: &settings,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: Rational::new(4, 1),
        timescale: 4,
        offered: &[if ten {
            PixelFormat::P010
        } else {
            PixelFormat::Nv12
        }],
        color: &Color::bt709_limited(),
    })?;
    let mut answers = Vec::new();
    for index in 0..8 {
        let mut pixels = Vec::new();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                pixels.push((40 + x + y + index * 10) as u8);
            }
        }
        for y in 0..HEIGHT / 2 {
            for x in 0..WIDTH / 2 {
                pixels.extend([100 + x as u8 / 4, 130 + y as u8 / 2]);
            }
        }
        let pixels = if ten {
            pixels
                .into_iter()
                .flat_map(|v| (u16::from(v) << 8).to_le_bytes())
                .collect()
        } else {
            pixels
        };
        answers.push(encoder.encode_video(opened.session, i64::from(index), pixels)?);
    }
    answers.push(encoder.finish_video(opened.session)?);
    let (packets, config) = packets_and_config(answers);
    let track = Track::Video {
        config,
        timescale: 4,
        width: WIDTH,
        height: HEIGHT,
        frame_rate: Rational::new(4, 1),
        color: Color::bt709_limited(),
        alpha: false,
    };
    mux(
        &mut codec,
        "mp4",
        output,
        &[track],
        0,
        &interleaved(&[(4, &packets)]),
    );
    Ok(())
}

/// Retags both H.264 VUI and container metadata without requiring a CLI or x264.
fn retag(input: &Path, output: &Path, tagged: bool, full: bool) {
    retag_colors(
        input,
        output,
        tagged,
        full,
        if tagged {
            av::AVColorPrimaries::AVCOL_PRI_BT709
        } else {
            av::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED
        },
        if tagged {
            av::AVColorTransferCharacteristic::AVCOL_TRC_BT709
        } else {
            av::AVColorTransferCharacteristic::AVCOL_TRC_UNSPECIFIED
        },
    );
}

fn retag_colors(
    input: &Path,
    output: &Path,
    tagged: bool,
    full: bool,
    primaries: av::AVColorPrimaries,
    transfer: av::AVColorTransferCharacteristic,
) {
    let input = CString::new(input.to_str().unwrap()).unwrap();
    let output = CString::new(output.to_str().unwrap()).unwrap();
    // SAFETY: libavformat owns the contexts and packet storage; every allocation
    // is checked, used while live, and released after the trailer is written.
    unsafe {
        let mut source = null_mut();
        assert_eq!(
            av::avformat_open_input(&mut source, input.as_ptr(), null_mut(), null_mut()),
            0
        );
        assert!(av::avformat_find_stream_info(source, null_mut()) >= 0);
        assert_eq!((*source).nb_streams, 1);
        let stream = *(*source).streams;
        let mut filter = null_mut();
        let bsf = bsf::av_bsf_get_by_name(c"h264_metadata".as_ptr());
        assert!(!bsf.is_null());
        assert_eq!(bsf::av_bsf_alloc(bsf, &mut filter), 0);
        assert_eq!(
            av::avcodec_parameters_copy((*filter).par_in, (*stream).codecpar),
            0
        );
        (*filter).time_base_in = (*stream).time_base;
        for (name, value) in [
            (c"colour_primaries", primaries as i64),
            (c"transfer_characteristics", transfer as i64),
            (c"matrix_coefficients", if tagged { 1 } else { 2 }),
        ] {
            assert_eq!(
                av::av_opt_set_int((*filter).priv_data, name.as_ptr(), value, 0),
                0
            );
        }
        assert_eq!(
            av::av_opt_set_int(
                (*filter).priv_data,
                c"video_full_range_flag".as_ptr(),
                i64::from(full),
                0
            ),
            0
        );
        assert_eq!(bsf::av_bsf_init(filter), 0);
        let mut destination = null_mut();
        assert_eq!(
            av::avformat_alloc_output_context2(
                &mut destination,
                null_mut(),
                c"mp4".as_ptr(),
                output.as_ptr()
            ),
            0
        );
        assert!(!destination.is_null());
        let out = av::avformat_new_stream(destination, null_mut());
        assert!(!out.is_null());
        assert_eq!(
            av::avcodec_parameters_copy((*out).codecpar, (*filter).par_out),
            0
        );
        let parameters = &mut *(*out).codecpar;
        parameters.color_primaries = primaries;
        parameters.color_trc = transfer;
        parameters.color_space = if tagged {
            av::AVColorSpace::AVCOL_SPC_BT709
        } else {
            av::AVColorSpace::AVCOL_SPC_UNSPECIFIED
        };
        parameters.color_range = if full {
            av::AVColorRange::AVCOL_RANGE_JPEG
        } else {
            av::AVColorRange::AVCOL_RANGE_MPEG
        };
        (*out).time_base = (*filter).time_base_out;
        assert_eq!(
            av::avio_open(&mut (*destination).pb, output.as_ptr(), av::AVIO_FLAG_WRITE),
            0
        );
        assert_eq!(av::avformat_write_header(destination, null_mut()), 0);
        let mut packet = av::av_packet_alloc();
        let mut filtered = av::av_packet_alloc();
        assert!(!packet.is_null() && !filtered.is_null());
        loop {
            let read = av::av_read_frame(source, packet);
            assert!(read >= 0 || read == av::AVERROR_EOF);
            assert_eq!(
                bsf::av_bsf_send_packet(
                    filter,
                    if read == av::AVERROR_EOF {
                        null_mut()
                    } else {
                        packet
                    }
                ),
                0
            );
            loop {
                let status = bsf::av_bsf_receive_packet(filter, filtered);
                if status == av::AVERROR(av::EAGAIN) || status == av::AVERROR_EOF {
                    break;
                }
                assert_eq!(status, 0);
                av::av_packet_rescale_ts(filtered, (*filter).time_base_out, (*out).time_base);
                (*filtered).stream_index = 0;
                assert_eq!(av::av_interleaved_write_frame(destination, filtered), 0);
            }
            if read == av::AVERROR_EOF {
                break;
            }
        }
        assert_eq!(av::av_write_trailer(destination), 0);
        av::av_packet_free(&mut packet);
        av::av_packet_free(&mut filtered);
        bsf::av_bsf_free(&mut filter);
        assert_eq!(av::avio_closep(&mut (*destination).pb), 0);
        av::avformat_free_context(destination);
        av::avformat_close_input(&mut source);
    }
}

fn request<'a>(path: &'a Path, time: f64, accepted: &'a [F]) -> VideoRequest<'a> {
    VideoRequest {
        path,
        time,
        accepted_pixel_formats: accepted,
        sequential: Some(Sequential {
            session: "native-yuv",
            frame_rate: Rational::new(4, 1),
        }),
    }
}

fn ffmpeg(path: &Path, time: f64) -> VideoFrame {
    let mut decoder = FfmpegBackend::new();
    decoder.set_accepted_pixel_formats(&[F::Yuv420p, F::Nv12]);
    decoder.decode_frame(path, time).unwrap()
}

fn exact_planes(label: &str, actual: &VideoFrame, reference: &VideoFrame) {
    assert_eq!(actual.pixel_format, F::Nv12);
    assert_eq!(
        (actual.width, actual.height),
        (reference.width, reference.height)
    );
    assert_eq!(actual.color, reference.color);
    let expected = if reference.pixel_format == F::Yuv420p {
        let y = (reference.width * reference.height) as usize;
        let mut pixels = reference.pixels[..y].to_vec();
        for index in 0..y / 4 {
            pixels.extend([
                reference.pixels[y + index],
                reference.pixels[y + y / 4 + index],
            ]);
        }
        pixels
    } else {
        assert_eq!(reference.pixel_format, F::Nv12);
        reference.pixels.as_ref().clone()
    };
    assert_eq!(actual.pixels.len(), expected.len());
    let mut max = 0u8;
    let mut sum = 0u64;
    let mut differing = 0usize;
    for (actual, reference) in actual.pixels.iter().zip(&expected) {
        let delta = actual.abs_diff(*reference);
        max = max.max(delta);
        sum += u64::from(delta);
        if delta != 0 {
            differing += 1;
        }
    }
    println!(
        "planes {label}: max={max} mean={:.9} differing_bytes={differing}",
        sum as f64 / expected.len() as f64
    );
    assert_eq!(
        *actual.pixels, expected,
        "AVFoundation/FFmpeg decoded planes differ"
    );
}

fn rgba_gate(label: &str, actual: &VideoFrame, reference: &VideoFrame) {
    let actual = actual.to_rgba().unwrap();
    assert_eq!(
        (actual.width, actual.height),
        (reference.width, reference.height)
    );
    let (mut max, mut sum, mut differing) = (0, 0u64, 0);
    for (a, b) in actual
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.pixels.as_chunks::<4>().0)
    {
        assert_eq!(a[3], b[3], "{label}: alpha");
        differing += usize::from(a != b);
        for channel in 0..3 {
            let d = a[channel].abs_diff(b[channel]);
            max = max.max(d);
            sum += u64::from(d);
        }
    }
    println!(
        "{label}: max={max} mean={:.9} differing={differing}",
        sum as f64 / (f64::from(actual.width) * f64::from(actual.height) * 3.)
    );
    assert!(
        max <= 4,
        "{label}: AVFoundation's own RGBA differs by {max}"
    );
}

fn exercise(path: &Path, expected: DecodedColor) {
    for transport in [FrameTransport::SharedMemory, FrameTransport::Pipe] {
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-avfoundation"));
        let mut process = MediaProcess::start_with(
            &executable,
            executable.parent().unwrap(),
            &Default::default(),
            transport,
            ProviderUse::Decode,
        )
        .unwrap();
        assert_eq!(
            process.shares_frames(),
            transport == FrameTransport::SharedMemory
        );
        process.set_decode_prefetch(true);
        for (index, time) in [0., 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 5., 0., 0.25]
            .into_iter()
            .enumerate()
        {
            let accepted = match index {
                3 => &[F::Rgba8][..],
                7 => &[][..],
                _ => &[F::Nv12][..],
            };
            let answer = process
                .decode_video(DECODER_ID, &request(path, time, accepted))
                .unwrap();
            let rgba = AvfCodec::default()
                .decode_video(DECODER_ID, &request(path, time, &[F::Rgba8]))
                .unwrap();
            assert_eq!(rgba.pixel_format, F::Rgba8);
            if !accepted.contains(&F::Nv12) {
                assert_eq!(answer, rgba);
            } else {
                assert_eq!(answer.color, Some(expected));
                exact_planes(
                    &format!("{} {transport:?} t={time}", path.display()),
                    &answer,
                    &ffmpeg(path, time),
                );
                rgba_gate(
                    &format!("{} {transport:?} t={time}", path.display()),
                    &answer,
                    &rgba,
                );
            }
        }
    }
}

#[test]
fn h264_nv12_matches_ffmpeg_and_our_rgba_over_both_transports_and_prefetch() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("source.mp4");
    generated(&original, "h264", false).unwrap();
    for (name, tagged, full) in [
        ("709-limited", true, false),
        ("untagged", false, false),
        ("709-full", true, true),
    ] {
        let output = directory.path().join(format!("{name}.mp4"));
        retag(&original, &output, tagged, full);
        exercise(
            &output,
            DecodedColor {
                matrix: if tagged {
                    DecodedMatrix::Bt709
                } else {
                    DecodedMatrix::Bt601
                },
                range: if full {
                    DecodedRange::Full
                } else {
                    DecodedRange::Limited
                },
                chroma_location: Default::default(),
            },
        );
    }
}

#[test]
fn hevc_nv12_and_ten_bit_fallback_when_available() {
    let directory = tempfile::tempdir().unwrap();
    for ten in [false, true] {
        let output = directory.path().join(format!("hevc-{ten}.mp4"));
        if let Err(error) = generated(&output, "hevc", ten) {
            println!("skipping HEVC ten={ten}: cannot encode: {error}");
            continue;
        }
        if let Err(error) = AvfCodec::default().decode_video(DECODER_ID, &request(&output, 0., &[]))
        {
            println!("skipping HEVC ten={ten}: cannot decode: {error}");
            continue;
        }
        if ten {
            fallback(&output);
        } else {
            exercise(
                &output,
                DecodedColor {
                    matrix: DecodedMatrix::Bt709,
                    range: DecodedRange::Limited,
                    chroma_location: Default::default(),
                },
            );
        }
    }
}

fn fallback(path: &Path) {
    let expected = AvfCodec::default()
        .decode_video(DECODER_ID, &request(path, 0., &[]))
        .unwrap();
    assert_eq!(expected.pixel_format, F::Rgba8);
    for transport in [FrameTransport::SharedMemory, FrameTransport::Pipe] {
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-avfoundation"));
        let mut process = MediaProcess::start_with(
            &executable,
            executable.parent().unwrap(),
            &Default::default(),
            transport,
            ProviderUse::Decode,
        )
        .unwrap();
        process.set_decode_prefetch(true);
        for accepted in [&[F::Nv12][..], &[F::Rgba8][..], &[][..]] {
            assert_eq!(
                process
                    .decode_video(DECODER_ID, &request(path, 0., accepted))
                    .unwrap(),
                expected
            );
        }
    }
}

#[test]
fn rotation_uses_unchanged_rgba_even_when_nv12_is_offered() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("rotated.mp4");
    rotated(&output);
    fallback(&output);
}

#[test]
fn odd_prores_and_alpha_prores_use_rgba() {
    let directory = tempfile::tempdir().unwrap();
    for (width, height, profile) in [(63, 35, "hq"), (64, 36, "4444")] {
        let mut codec = AvfCodec::default();
        let settings = [("profile".into(), SettingValue::String(profile.into()))]
            .into_iter()
            .collect();
        let encoder = codec.video_encoder().unwrap();
        let opened = match encoder.open_video(&VideoEncoderRequest {
            component: "prores",
            settings: &settings,
            width,
            height,
            frame_rate: Rational::new(4, 1),
            timescale: 4,
            offered: &[PixelFormat::Rgba16],
            color: &Color::bt709_limited(),
        }) {
            Ok(opened) => opened,
            Err(error) => {
                println!("skipping ProRes {profile}: cannot encode: {error}");
                continue;
            }
        };
        let pixels = [
            50_000u16,
            10_000,
            20_000,
            if profile == "4444" { 32_768 } else { 65_535 },
        ]
        .repeat((width * height) as usize)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
        let answers = vec![
            encoder.encode_video(opened.session, 0, pixels).unwrap(),
            encoder.finish_video(opened.session).unwrap(),
        ];
        let (packets, config) = packets_and_config(answers);
        let output = directory.path().join(format!("{profile}.mov"));
        let track = Track::Video {
            config,
            timescale: 4,
            width,
            height,
            frame_rate: Rational::new(4, 1),
            color: Color::bt709_limited(),
            alpha: opened.alpha,
        };
        mux(
            &mut codec,
            "mov",
            &output,
            &[track],
            0,
            &interleaved(&[(4, &packets)]),
        );
        fallback(&output);
        let answer = AvfCodec::default()
            .decode_video(DECODER_ID, &request(&output, 0., &[F::Nv12]))
            .unwrap();
        assert_eq!((answer.width, answer.height), (width, height));
        if opened.alpha {
            assert!(
                answer
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[3] < 255)
            );
        }
    }
}

#[test]
fn wide_primaries_and_linear_transfer_keep_rgba() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.mp4");
    generated(&source, "h264", false).unwrap();
    for (name, primaries, transfer, primary_name, transfer_name) in [
        (
            "p3",
            av::AVColorPrimaries::AVCOL_PRI_SMPTE432,
            av::AVColorTransferCharacteristic::AVCOL_TRC_BT709,
            "smpte432",
            "bt709",
        ),
        (
            "linear",
            av::AVColorPrimaries::AVCOL_PRI_BT709,
            av::AVColorTransferCharacteristic::AVCOL_TRC_LINEAR,
            "bt709",
            "linear",
        ),
    ] {
        let output = directory.path().join(format!("{name}.mp4"));
        retag_colors(&source, &output, true, false, primaries, transfer);
        let tags = celesta_codec_ffmpeg::FfmpegCodec::default()
            .decoder()
            .unwrap()
            .probe(celesta_codec_ffmpeg::DECODER_ID, &output)
            .unwrap()
            .video
            .unwrap()
            .color;
        assert_eq!(tags.primaries.as_deref(), Some(primary_name));
        assert_eq!(tags.transfer.as_deref(), Some(transfer_name));
        fallback(&output);
    }
}
