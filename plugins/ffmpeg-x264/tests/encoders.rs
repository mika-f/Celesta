//! The FFmpeg plugin's encoders and muxers, in this process: the input each
//! video encoder chooses of those offered, the rate and delay each audio
//! encoder answers, the packets and track config they give, and what
//! `describe` says of every component.

mod support;
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    AudioEncoderOpened, AudioEncoderRequest, Color, ComponentKind, Encoded, MediaError,
    MediaProvider, MuxerRequest, Packet, PixelFormat, SampleRates, SessionId, SettingValue,
    Settings, Track, TrackConfig, VideoEncoderOpened, VideoEncoderRequest,
};
use ffmpeg_sys_next::{
    AVColorPrimaries, AVColorRange, AVColorSpace, AVColorTransferCharacteristic, AVERROR,
    AVERROR_EOF, AVPixelFormat, EAGAIN, av_frame_alloc, av_frame_free, av_malloc, av_packet_alloc,
    av_packet_free, av_pix_fmt_desc_get, avcodec_alloc_context3, avcodec_find_decoder_by_name,
    avcodec_free_context, avcodec_open2, avcodec_receive_frame, avcodec_send_packet,
};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::ptr::null_mut;

const WIDTH: u32 = 32;
const HEIGHT: u32 = 16;
const FRAMES: i64 = 10;

fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> SettingValue {
    SettingValue::String(value.to_owned())
}

/// Opens `component` at 32x16 and 10 fps.
fn open(
    codec: &mut dyn MediaProvider,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
) -> Result<VideoEncoderOpened, MediaError> {
    open_colored(codec, component, given, offered, &Color::bt709_limited())
}

/// [`open`] with `color`.
fn open_colored(
    codec: &mut dyn MediaProvider,
    component: &str,
    given: &Settings,
    offered: &[PixelFormat],
    color: &Color,
) -> Result<VideoEncoderOpened, MediaError> {
    codec
        .video_encoder()
        .expect("the plugin has video encoders")
        .open_video(&VideoEncoderRequest {
            component,
            settings: given,
            width: WIDTH,
            height: HEIGHT,
            frame_rate: Rational::new(10, 1),
            timescale: 10,
            offered,
            color,
        })
}

/// The Y, U, and V planes of frame `index` at 32x16, in 8-bit codes or, with
/// `ten`, 10-bit ones: each moves with `index` and along its rows and
/// columns, and U and V differ, so a mixed-up plane or chroma order shows.
fn planes(index: i64, ten: bool) -> [Vec<u16>; 3] {
    let (width, height) = (WIDTH as u16, HEIGHT as u16);
    let step = index as u16 * 20;
    let plane = |width: u16, height: u16, sample: &dyn Fn(u16, u16) -> u16| {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let code = sample(x, y);
                if ten { code * 4 + (x + y) % 4 } else { code }
            })
            .collect::<Vec<u16>>()
    };
    [
        plane(width, height, &|x, y| 16 + step / 2 + x * 4 + y),
        plane(width / 2, height / 2, &|x, y| 64 + step / 4 + x * 3 + y * 2),
        plane(width / 2, height / 2, &|x, y| 192 - step / 4 - x * 3 - y),
    ]
}

/// `samples` as 16-bit little-endian bytes.
fn little_endian(samples: impl IntoIterator<Item = u16>) -> Vec<u8> {
    samples
        .into_iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

/// U and V interleaved, as NV12 and P010 store chroma.
fn interleaved(u: &[u16], v: &[u16]) -> Vec<u16> {
    u.iter().zip(v).flat_map(|(u, v)| [*u, *v]).collect()
}

/// One frame of `format` at 32x16 whose colors move with `index`; the YUV
/// formats hold the same [`planes`].
fn frame(format: PixelFormat, index: i64) -> Vec<u8> {
    let (width, height) = (WIDTH as usize, HEIGHT as usize);
    let step = index as u8 * 20;
    match format {
        PixelFormat::Rgba8 => [step, 64, 255 - step, 255].repeat(width * height),
        PixelFormat::Rgba16 => [step, 64, 255 - step, 128]
            .map(|channel| u16::from(channel) * 257)
            .iter()
            .flat_map(|channel| channel.to_le_bytes())
            .collect::<Vec<u8>>()
            .repeat(width * height),
        PixelFormat::Yuv420p => planes(index, false)
            .concat()
            .iter()
            .map(|&code| code as u8)
            .collect(),
        PixelFormat::Nv12 => {
            let [y, u, v] = planes(index, false);
            [y, interleaved(&u, &v)]
                .concat()
                .iter()
                .map(|&code| code as u8)
                .collect()
        }
        PixelFormat::Yuv420p10 => little_endian(planes(index, true).concat()),
        PixelFormat::P010 => {
            let [y, u, v] = planes(index, true);
            little_endian(
                [y, interleaved(&u, &v)]
                    .concat()
                    .iter()
                    .map(|code| code << 6),
            )
        }
    }
}

/// A frame of RGBA `format` of one color, `rgb` in 8 bits.
fn solid(format: PixelFormat, rgb: [u8; 3]) -> Vec<u8> {
    let [r, g, b] = rgb;
    let pixels = (WIDTH * HEIGHT) as usize;
    match format {
        PixelFormat::Rgba8 => [r, g, b, 255].repeat(pixels),
        PixelFormat::Rgba16 => {
            little_endian([r, g, b, 255].map(|channel| u16::from(channel) * 257)).repeat(pixels)
        }
        other => panic!("no solid frames of {other}"),
    }
}

/// Ten frames through `opened`'s session and its finish: every packet, and
/// the config of the first answer with packets.
fn encode_all(
    codec: &mut dyn MediaProvider,
    opened: VideoEncoderOpened,
) -> (Vec<Packet>, TrackConfig) {
    let frames = (0..FRAMES).map(|index| frame(opened.input, index));
    encode_frames(codec, opened.session, frames)
}

/// `frames` through `session` and its finish: every packet, and the config
/// of the first answer with packets.
fn encode_frames(
    codec: &mut dyn MediaProvider,
    session: SessionId,
    frames: impl IntoIterator<Item = Vec<u8>>,
) -> (Vec<Packet>, TrackConfig) {
    let encoder = codec.video_encoder().unwrap();
    let mut packets = Vec::new();
    let mut config = None;
    let mut take = |answer: Encoded, packets: &mut Vec<Packet>| {
        if config.is_none() && !answer.packets.is_empty() {
            config = Some(
                answer
                    .config
                    .expect("the first packets come with the config"),
            );
        }
        packets.extend(answer.packets);
    };
    for (index, pixels) in frames.into_iter().enumerate() {
        let answer = encoder.encode_video(session, index as i64, pixels).unwrap();
        take(answer, &mut packets);
    }
    let answer = encoder.finish_video(session).unwrap();
    take(answer, &mut packets);
    (packets, config.expect("the encoder made packets"))
}

#[test]
fn libx264_packets_carry_an_avcc_and_a_codec_string() {
    let mut codec = FfmpegCodec::x264();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("preset", text("ultrafast"))]),
        &[
            PixelFormat::Yuv420p,
            PixelFormat::Rgba16,
            PixelFormat::Rgba8,
        ],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Yuv420p);
    assert!(!opened.alpha);
    let (packets, config) = encode_all(&mut codec, opened);
    assert!(config.codec.starts_with("avc1."), "{}", config.codec);
    let description = config.description.expect("H.264 has an avcC");
    assert_eq!(description[0], 1);
    assert_eq!(description[5] & 0x1F, 1, "one SPS");
    assert_eq!(packets.len(), FRAMES as usize);
    assert!(packets[0].key);
    for packet in &packets {
        let length = u32::from_be_bytes(packet.bytes[..4].try_into().unwrap()) as usize;
        assert!(
            length + 4 <= packet.bytes.len(),
            "{length} of {}",
            packet.bytes.len()
        );
        assert_eq!(packet.duration, 1);
    }
}

#[test]
fn ten_bit_h264_picks_yuv420p10_when_offered() {
    let mut codec = FfmpegCodec::x264();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10")), ("preset", text("ultrafast"))]),
        &[
            PixelFormat::Yuv420p,
            PixelFormat::Yuv420p10,
            PixelFormat::Rgba16,
        ],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Yuv420p10);
    let (_, config) = encode_all(&mut codec, opened);
    // High 10 is profile 110.
    assert!(config.codec.starts_with("avc1.6E"), "{}", config.codec);
}

/// A frame a decoder gave: its size, color tags, and samples.
#[derive(Debug, PartialEq)]
struct Decoded {
    size: (i32, i32),
    colorspace: AVColorSpace,
    primaries: AVColorPrimaries,
    transfer: AVColorTransferCharacteristic,
    range: AVColorRange,
    /// Each plane's samples, tightly packed, 16 bits or fewer each.
    planes: Vec<Vec<u16>>,
}

impl Decoded {
    /// The distinct samples of each of the first three planes.
    fn plane_values(&self) -> Vec<BTreeSet<u16>> {
        self.planes[..3]
            .iter()
            .map(|plane| plane.iter().copied().collect())
            .collect()
    }
}

/// Decodes `packets` with libavcodec's decoder `name`, given `extradata`
/// (an avcC for H.264): every frame it gives.
fn decode(name: &str, extradata: Option<&[u8]>, packets: &[Packet]) -> Vec<Decoded> {
    let mut frames = Vec::new();
    let name = CString::new(name).unwrap();
    // SAFETY: every pointer comes from the allocator it is freed with; the
    // extradata carries libavcodec's input padding, and libavcodec copies
    // the packets' data, which is not reference counted, into padded
    // buffers of its own. A frame's planes hold `linesize` bytes a row for
    // the rows its format's descriptor gives.
    unsafe {
        let decoder = avcodec_find_decoder_by_name(name.as_ptr());
        assert!(!decoder.is_null(), "no decoder {name:?}");
        let mut context = avcodec_alloc_context3(decoder);
        if let Some(bytes) = extradata {
            let padding = 64;
            let copy = av_malloc(bytes.len() + padding).cast::<u8>();
            std::ptr::write_bytes(copy, 0, bytes.len() + padding);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), copy, bytes.len());
            (*context).extradata = copy;
            (*context).extradata_size = bytes.len() as i32;
        }
        assert_eq!(avcodec_open2(context, decoder, null_mut()), 0);
        let mut packet = av_packet_alloc();
        let mut frame = av_frame_alloc();
        let receive = |frames: &mut Vec<Decoded>| loop {
            let result = avcodec_receive_frame(context, frame);
            if result == AVERROR(EAGAIN) || result == AVERROR_EOF {
                break;
            }
            assert_eq!(result, 0, "the decoder failed");
            let (width, height) = ((*frame).width, (*frame).height);
            let format: AVPixelFormat = std::mem::transmute((*frame).format);
            let descriptor = av_pix_fmt_desc_get(format);
            let wide = (*descriptor).comp[0].depth > 8;
            let mut planes = Vec::new();
            for plane in 0..4 {
                let data = (*frame).data[plane];
                if data.is_null() {
                    break;
                }
                let (columns, rows) = if plane == 1 || plane == 2 {
                    (
                        -((-width) >> (*descriptor).log2_chroma_w),
                        -((-height) >> (*descriptor).log2_chroma_h),
                    )
                } else {
                    (width, height)
                };
                let stride = (*frame).linesize[plane] as isize;
                let mut samples = Vec::new();
                for row in 0..rows as isize {
                    let start = data.offset(row * stride);
                    for column in 0..columns as usize {
                        samples.push(if wide {
                            u16::from_le_bytes([*start.add(column * 2), *start.add(column * 2 + 1)])
                        } else {
                            u16::from(*start.add(column))
                        });
                    }
                }
                planes.push(samples);
            }
            frames.push(Decoded {
                size: (width, height),
                colorspace: (*frame).colorspace,
                primaries: (*frame).color_primaries,
                transfer: (*frame).color_trc,
                range: (*frame).color_range,
                planes,
            });
        };
        for encoded in packets {
            let copy = encoded.bytes.clone();
            (*packet).data = copy.as_ptr().cast_mut();
            (*packet).size = copy.len() as i32;
            (*packet).pts = encoded.pts;
            (*packet).dts = encoded.dts;
            assert_eq!(avcodec_send_packet(context, packet), 0);
            (*packet).data = null_mut();
            (*packet).size = 0;
            receive(&mut frames);
        }
        assert_eq!(avcodec_send_packet(context, null_mut()), 0);
        receive(&mut frames);
        av_frame_free(&mut frame);
        av_packet_free(&mut packet);
        avcodec_free_context(&mut context);
    }
    frames
}

/// [`decode`] with libavcodec's H.264 decoder, given the track's avcC.
fn decode_h264(config: &TrackConfig, packets: &[Packet]) -> Vec<Decoded> {
    decode("h264", config.description.as_deref(), packets)
}

#[test]
fn rgba_input_is_converted() {
    let mut codec = FfmpegCodec::x264();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("preset", text("ultrafast"))]),
        &[PixelFormat::Rgba8],
    )
    .unwrap();
    assert_eq!(opened.input, PixelFormat::Rgba8);
    assert!(!opened.alpha);
    let (packets, config) = encode_all(&mut codec, opened);
    let sizes: Vec<(i32, i32)> = decode_h264(&config, &packets)
        .iter()
        .map(|frame| frame.size)
        .collect();
    assert_eq!(sizes, vec![(WIDTH as i32, HEIGHT as i32); FRAMES as usize]);
}

#[test]
fn software_offers_pick_rgba_by_bit_depth() {
    let offered = [PixelFormat::Rgba16, PixelFormat::Rgba8];
    let mut codec = FfmpegCodec::x264();
    let eight = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("8"))]),
        &offered,
    )
    .unwrap();
    assert_eq!(eight.input, PixelFormat::Rgba8);
    let ten = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10"))]),
        &offered,
    )
    .unwrap();
    assert_eq!(ten.input, PixelFormat::Rgba16);
    let (_, config) = encode_all(&mut codec, ten);
    assert!(config.codec.starts_with("avc1.6E"), "{}", config.codec);
    // Either RGBA serves either bit depth when the other is not offered.
    for (depth, offered) in [("8", PixelFormat::Rgba16), ("10", PixelFormat::Rgba8)] {
        let opened = open(
            &mut codec,
            "libx264",
            &settings(&[("bitDepth", text(depth))]),
            &[offered],
        )
        .unwrap();
        assert_eq!(opened.input, offered, "{depth}-bit");
    }

    let refused = |error: MediaError| match error {
        MediaError::InvalidSettings { key, message } => (key, message),
        other => panic!("expected invalid settings, got {other:?}"),
    };
    let error = open(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Yuv420p10, PixelFormat::P010],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        (
            "input".to_owned(),
            "needs yuv420p, nv12, rgba8, or rgba16".to_owned()
        )
    );
    let error = open(
        &mut codec,
        "libx264",
        &settings(&[("bitDepth", text("10"))]),
        &[PixelFormat::Yuv420p, PixelFormat::Nv12],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
        (
            "input".to_owned(),
            "needs yuv420p10, p010, rgba16, or rgba8".to_owned()
        )
    );
    let mut codec = FfmpegCodec::new();
    let error = open(
        &mut codec,
        "prores",
        &Settings::new(),
        &[PixelFormat::Rgba8],
    )
    .unwrap_err();
    assert_eq!(
        refused(error),
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
    let mut codec = support::process("ffmpeg-opus");
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
    let mut codec = support::process("ffmpeg-vpx");
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

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

#[test]
fn libx264_writes_bt709_vui() {
    let mut codec = FfmpegCodec::x264();
    let opened = open(
        &mut codec,
        "libx264",
        &settings(&[("preset", text("ultrafast"))]),
        &[PixelFormat::Yuv420p],
    )
    .unwrap();
    let (packets, config) = encode_all(&mut codec, opened);
    let frames = decode_h264(&config, &packets);
    assert_eq!(frames.len(), FRAMES as usize);
    for frame in &frames {
        assert_eq!(
            (
                frame.colorspace,
                frame.primaries,
                frame.transfer,
                frame.range
            ),
            (
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorPrimaries::AVCOL_PRI_BT709,
                AVColorTransferCharacteristic::AVCOL_TRC_BT709,
                AVColorRange::AVCOL_RANGE_MPEG
            )
        );
    }
}

#[test]
fn rgba_input_is_converted_with_bt709() {
    let mut codec = FfmpegCodec::x264();
    // libswscale's BT.709 limited-range codes of red, green, and blue, as
    // measured on arm64 (Apple M4); every decoded sample is within 1 of
    // them, since x86_64's SIMD paths may round differently. At 10 bits,
    // from 16-bit RGB, its fixed point gives these: within 2 of the
    // formula's (250, 409, 960), (691, 167, 105), (127, 960, 471), and far
    // from BT.601's (red's Y would be about 326).
    for (format, depth, expected) in [
        (
            PixelFormat::Rgba8,
            "8",
            [[63, 102, 240], [173, 42, 26], [32, 240, 118]],
        ),
        (
            PixelFormat::Rgba16,
            "10",
            [[251, 409, 962], [693, 165, 103], [128, 962, 471]],
        ),
    ] {
        let opened = open(
            &mut codec,
            "libx264",
            &settings(&[
                ("preset", text("ultrafast")),
                ("crf", SettingValue::Number(0.0)),
                ("bitDepth", text(depth)),
            ]),
            &[format],
        )
        .unwrap();
        assert_eq!(opened.input, format);
        let frames = [RED, GREEN, BLUE].map(|rgb| solid(format, rgb));
        let (packets, config) = encode_frames(&mut codec, opened.session, frames);
        let decoded = decode_h264(&config, &packets);
        assert_eq!(decoded.len(), 3, "{format}");
        for (frame, codes) in decoded.iter().zip(expected) {
            let planes = frame.plane_values();
            assert_eq!(planes.len(), codes.len(), "{format}");
            for (values, code) in planes.iter().zip(codes) {
                assert!(
                    values.iter().all(|value| value.abs_diff(code) <= 1),
                    "{format}: {values:?}, expected {code}"
                );
            }
        }
    }
}

#[test]
fn libx264_takes_nv12_and_p010() {
    let mut codec = FfmpegCodec::x264();
    for (depth, planar, interleaved) in [
        ("8", PixelFormat::Yuv420p, PixelFormat::Nv12),
        ("10", PixelFormat::Yuv420p10, PixelFormat::P010),
    ] {
        let given = settings(&[("preset", text("ultrafast")), ("bitDepth", text(depth))]);
        let mut run = |offered: PixelFormat| {
            let opened = open(&mut codec, "libx264", &given, &[offered]).unwrap();
            assert_eq!(opened.input, offered);
            let (packets, config) = encode_all(&mut codec, opened);
            (decode_h264(&config, &packets), config.codec)
        };
        let (expected, _) = run(planar);
        let (decoded, family) = run(interleaved);
        assert_eq!(decoded.len(), FRAMES as usize, "{interleaved}");
        assert!(decoded == expected, "{interleaved} decodes unlike {planar}");
        if depth == "10" {
            assert!(family.starts_with("avc1.6E"), "{family}");
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

    let mut codec = support::process("ffmpeg-vpx");
    let opened = open(&mut codec, "vp9", &Settings::new(), &[PixelFormat::Rgba16]).unwrap();
    let (packets, _) = encode_all(&mut codec, opened);
    let frames = decode("vp9", None, &packets);
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
    for (component, offered) in [
        ("libx264", PixelFormat::Yuv420p),
        ("prores", PixelFormat::Rgba16),
        ("vp9", PixelFormat::Rgba16),
    ] {
        let mut codec: Box<dyn MediaProvider> = if component == "libx264" {
            Box::new(FfmpegCodec::x264()) as Box<dyn MediaProvider>
        } else if component == "vp9" {
            Box::new(support::process("ffmpeg-vpx"))
        } else {
            Box::new(FfmpegCodec::new())
        };
        for (color, expected) in &colors {
            let result = open_colored(
                codec.as_mut(),
                component,
                &Settings::new(),
                &[offered],
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
    let mut codec = FfmpegCodec::x264();
    let ycgco = Color {
        matrix: "ycgco".into(),
        ..Color::bt709_limited()
    };
    for (component, offered) in [
        ("libx264", PixelFormat::Rgba8),
        ("prores", PixelFormat::Rgba16),
        ("vp9", PixelFormat::Rgba16),
    ] {
        let mut codec: Box<dyn MediaProvider> = if component == "libx264" {
            Box::new(FfmpegCodec::x264()) as Box<dyn MediaProvider>
        } else if component == "vp9" {
            Box::new(support::process("ffmpeg-vpx"))
        } else {
            Box::new(FfmpegCodec::new())
        };
        let result = open_colored(
            codec.as_mut(),
            component,
            &Settings::new(),
            &[offered],
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
    // YUV input needs no conversion, so the matrix is only a tag.
    let opened = open_colored(
        &mut codec,
        "libx264",
        &Settings::new(),
        &[PixelFormat::Yuv420p],
        &ycgco,
    )
    .unwrap();
    codec.video_encoder().unwrap().abort_video(opened.session);
}

#[test]
fn describe_lists_six_inputs_for_libx264() {
    let described = FfmpegCodec::x264().describe().unwrap();
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap();
    let six = [
        PixelFormat::Yuv420p,
        PixelFormat::Yuv420p10,
        PixelFormat::Nv12,
        PixelFormat::P010,
        PixelFormat::Rgba8,
        PixelFormat::Rgba16,
    ];
    assert_eq!(libx264.input_formats, six);
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    let entry = manifest["media"]["videoEncoders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "libx264")
        .unwrap();
    let listed: Vec<PixelFormat> = serde_json::from_value(entry["inputFormats"].clone()).unwrap();
    assert_eq!(listed, six);
}

/// The ids `plugin.json` declares under `key`.
fn manifest_ids(key: &str) -> BTreeSet<String> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
    let base: serde_json::Value =
        serde_json::from_str(include_str!("../../ffmpeg/plugin.json")).unwrap();
    let opus: serde_json::Value =
        serde_json::from_str(include_str!("../../ffmpeg-opus/plugin.json")).unwrap();
    let vpx: serde_json::Value =
        serde_json::from_str(include_str!("../../ffmpeg-vpx/plugin.json")).unwrap();
    manifest["media"][key]
        .as_array()
        .into_iter()
        .flatten()
        .chain(base["media"][key].as_array().into_iter().flatten())
        .chain(opus["media"][key].as_array().into_iter().flatten())
        .chain(vpx["media"][key].as_array().into_iter().flatten())
        .map(|entry| entry["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn describe_names_every_manifest_component() {
    let mut described = FfmpegCodec::new().describe().unwrap();
    described.extend(FfmpegCodec::x264().describe().unwrap());
    described.extend(support::process("ffmpeg-opus").describe().unwrap());
    described.extend(support::process("ffmpeg-vpx").describe().unwrap());
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
    let libx264 = described
        .iter()
        .find(|component| {
            component.kind == ComponentKind::VideoEncoder && component.id == "libx264"
        })
        .unwrap();
    assert!(libx264.available, "{:?}", libx264.reason);
    for encoder in described
        .iter()
        .filter(|component| component.kind == ComponentKind::VideoEncoder && component.available)
    {
        assert_eq!(
            encoder.transports,
            ["sharedMemory", "pipe"],
            "{}",
            encoder.id
        );
    }
    let keys: Vec<&str> = libx264
        .settings
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "preset",
            "rateControl",
            "crf",
            "bitrate",
            "bitDepth",
            "tune"
        ]
    );
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

#[test]
fn x264_ntsc_packets_use_the_requested_timescale() {
    let mut codec = FfmpegCodec::x264();
    let opened = codec
        .video_encoder()
        .unwrap()
        .open_video(&VideoEncoderRequest {
            component: "libx264",
            settings: &Settings::new(),
            width: WIDTH,
            height: HEIGHT,
            frame_rate: Rational::new(30_000, 1001),
            timescale: 30_000,
            offered: &[PixelFormat::Yuv420p],
            color: &Color::bt709_limited(),
        })
        .unwrap();
    let encoder = codec.video_encoder().unwrap();
    let mut packets = Vec::new();
    for index in 0..10 {
        packets.extend(
            encoder
                .encode_video(
                    opened.session,
                    index * 1001,
                    frame(PixelFormat::Yuv420p, index),
                )
                .unwrap()
                .packets,
        );
    }
    packets.extend(encoder.finish_video(opened.session).unwrap().packets);
    let mut times: Vec<_> = packets.iter().map(|packet| packet.pts).collect();
    times.sort_unstable();
    assert_eq!(times, (0..10).map(|index| index * 1001).collect::<Vec<_>>());
    assert!(
        packets
            .iter()
            .all(|packet| packet.duration == 1001 && packet.dts % 1001 == 0)
    );
}

#[test]
fn an_unknown_x264_tune_is_refused_before_encoding() {
    let mut codec = FfmpegCodec::x264();
    let result = open(
        &mut codec,
        "libx264",
        &settings(&[("tune", text("cinema"))]),
        &[PixelFormat::Rgba8],
    );
    assert!(matches!(result, Err(MediaError::InvalidSettings { key, .. }) if key == "tune"));
}

#[test]
fn the_gpl_build_cannot_serve_as_the_standard_plugin() {
    assert!(celesta_codec_ffmpeg::check_lgpl().is_err());
}
