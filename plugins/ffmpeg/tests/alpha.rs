//! VP9 alpha is decoded here, where libvpx is linked into the standard
//! build; the x264 build's tests must not rely on libvpx being present.
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_composition::Rational;
use celesta_media::{
    Color, MediaProvider, MuxerRequest, PixelFormat, Settings, Track, TrackPacket,
    VideoEncoderRequest,
};
use ez_ffmpeg::{FfmpegContext, Input, Output};

#[test]
fn alpha_survives_encoding_and_webm_muxing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("alpha.webm");
    let raw = directory.path().join("alpha.rgba");
    let color = Color::bt709_limited();
    let mut codec = FfmpegCodec::new();
    let encoder = codec.video_encoder().unwrap();
    let opened = encoder
        .open_video(&VideoEncoderRequest {
            component: "vp9",
            settings: &Settings::new(),
            width: 64,
            height: 32,
            frame_rate: Rational::new(1, 1),
            timescale: 1,
            offered: &[PixelFormat::Rgba16],
            color: &color,
        })
        .unwrap();
    // Distinct alpha values and positions catch inversion, opacity loss,
    // and orientation mistakes that a uniform half-alpha frame cannot.
    let pixels: Vec<_> = (0..32)
        .flat_map(|y| {
            (0..64).flat_map(move |x| {
                let pixel: [u16; 4] = match (x < 32, y < 16) {
                    (true, true) => [65535, 0, 0, 128 * 257],
                    (false, true) => [0, 0, 0, 0],
                    (true, false) => [0, 0, 65535, 65535],
                    (false, false) => [0, 65535, 0, 64 * 257],
                };
                pixel.into_iter().flat_map(u16::to_le_bytes)
            })
        })
        .collect();
    let mut encoded = encoder.encode_video(opened.session, 0, pixels).unwrap();
    let finished = encoder.finish_video(opened.session).unwrap();
    encoded.config = encoded.config.or(finished.config);
    encoded.packets.extend(finished.packets);
    let tracks = [Track::Video {
        config: encoded.config.unwrap(),
        timescale: 1,
        width: 64,
        height: 32,
        frame_rate: Rational::new(1, 1),
        color,
        alpha: true,
    }];
    let muxer = codec.muxer().unwrap();
    let session = muxer
        .open_muxer(&MuxerRequest {
            component: "webm",
            settings: &Settings::new(),
            output: &path,
            tracks: &tracks,
            leading_audio: 0,
        })
        .unwrap();
    let packets: Vec<_> = encoded
        .packets
        .into_iter()
        .map(|packet| TrackPacket { track: 0, packet })
        .collect();
    muxer.write_packets(session, &packets).unwrap();
    muxer.finish_muxer(session).unwrap();

    // Check the container's track declaration as well as the decoded pixels:
    // players use alpha_mode to recognize the separate VP9 alpha payload.
    let url = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: libavformat owns the context and streams until close_input.
    // Copy the tag before closing the context; no borrowed pointer escapes.
    unsafe {
        use ffmpeg_sys_next as ff;
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
        let stream = streams
            .iter()
            .copied()
            .find(|&stream| (*(*stream).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO)
            .expect("video stream");
        let tag = ff::av_dict_get(
            (*stream).metadata,
            c"alpha_mode".as_ptr(),
            std::ptr::null(),
            0,
        );
        let alpha_mode =
            (!tag.is_null()).then(|| std::ffi::CStr::from_ptr((*tag).value).to_bytes().to_vec());
        ff::avformat_close_input(&mut context);
        assert_eq!(alpha_mode.as_deref(), Some(b"1".as_slice()));
    }

    FfmpegContext::builder()
        .input(Input::from(path.to_string_lossy().into_owned()).set_video_codec("libvpx-vp9"))
        .output(
            Output::from(raw.to_string_lossy().into_owned())
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
    let bytes = std::fs::read(raw).unwrap();
    assert_eq!(bytes.len(), 64 * 32 * 8);
    for (x, y, expected, channel) in [
        (8, 8, 128 * 257, 0),
        (48, 8, 0, 0),
        (8, 24, 65535, 2),
        (48, 24, 64 * 257, 1),
    ] {
        let offset = (y * 64 + x) * 8;
        let alpha = u16::from_le_bytes([bytes[offset + 6], bytes[offset + 7]]);
        assert!(
            alpha.abs_diff(expected) <= 2 * 257,
            "({x}, {y}): {alpha}, expected {expected}"
        );
        if expected != 0 {
            let color =
                u16::from_le_bytes([bytes[offset + 2 * channel], bytes[offset + 2 * channel + 1]]);
            assert!(color.abs_diff(65535) <= 6 * 257, "({x}, {y}): {color}");
        }
    }
}
