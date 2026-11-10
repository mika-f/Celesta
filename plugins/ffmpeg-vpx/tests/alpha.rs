//! VP9 alpha is decoded here, where libvpx is actually linked; other plugin
//! tests must not rely on libvpx being present in their own FFmpeg build.
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
    let mut codec = FfmpegCodec::vpx();
    let encoder = codec.video_encoder().unwrap();
    let opened = encoder
        .open_video(&VideoEncoderRequest {
            component: "vp9",
            settings: &Settings::new(),
            width: 32,
            height: 16,
            frame_rate: Rational::new(1, 1),
            timescale: 1,
            offered: &[PixelFormat::Rgba16],
            color: &color,
        })
        .unwrap();
    let pixel: Vec<_> = [65535_u16, 0, 0, 128 * 257]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut encoded = encoder
        .encode_video(opened.session, 0, pixel.repeat(32 * 16))
        .unwrap();
    let finished = encoder.finish_video(opened.session).unwrap();
    encoded.config = encoded.config.or(finished.config);
    encoded.packets.extend(finished.packets);
    let tracks = [Track::Video {
        config: encoded.config.unwrap(),
        timescale: 1,
        width: 32,
        height: 16,
        frame_rate: Rational::new(1, 1),
        color,
        alpha: true,
    }];
    let mut standard = FfmpegCodec::new();
    let muxer = standard.muxer().unwrap();
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
    assert_eq!(bytes.len(), 32 * 16 * 8);
    for pixel in bytes.as_chunks::<8>().0 {
        let alpha = u16::from_le_bytes([pixel[6], pixel[7]]);
        assert!(alpha.abs_diff(128 * 257) <= 2 * 257, "{alpha}");
        let red = u16::from_le_bytes([pixel[0], pixel[1]]);
        assert!(red.abs_diff(65535) <= 6 * 257, "{red}");
    }
}
