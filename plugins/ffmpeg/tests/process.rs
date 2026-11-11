//! The default process must expose LGPL components only: everything but
//! libx264, with VP9 and Opus through the BSD-licensed libvpx and libopus.
use celesta_codec_ffmpeg::{FfmpegCodec, registered};
use celesta_composition::Rational;
use celesta_media::{
    Color, MediaProvider, PixelFormat, Settings, VideoEncoder, VideoEncoderRequest,
};

#[test]
fn manifest_and_describe_exclude_x264() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"))
        .arg(celesta_plugin_host::MANIFEST_FLAG)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(manifest["id"], "ffmpeg");
    assert_eq!(manifest["name"], "FFmpeg");
    let mut codec = FfmpegCodec::new();
    let components = codec.describe().unwrap();
    assert_eq!(components.len(), 9);
    assert!(components.iter().all(|component| component.id != "libx264"));
    for id in ["vp9", "opus"] {
        assert!(
            components.iter().any(|component| component.id == id),
            "{id} is served by the standard build"
        );
    }
    let declared = registered();
    assert_eq!(declared.media.video_encoders.len(), 2);
    assert_eq!(declared.media.audio_encoders.len(), 3);
    assert!(
        codec
            .open_video(&VideoEncoderRequest {
                component: "libx264",
                settings: &Settings::new(),
                width: 2,
                height: 2,
                frame_rate: Rational::new(1, 1),
                timescale: 1,
                offered: &[PixelFormat::Rgba8],
                color: &Color::bt709_limited(),
            })
            .is_err()
    );
}

#[test]
fn the_x264_provider_exposes_only_its_encoder() {
    let mut provider = FfmpegCodec::x264();
    let described = provider.describe().unwrap();
    assert_eq!(described.len(), 1);
    assert_eq!(described[0].id, "libx264");
    assert!(provider.decoder().is_none());
    assert!(provider.audio_encoder().is_none());
    assert!(provider.muxer().is_none());
    for component in ["vp9", "prores"] {
        assert!(
            provider
                .open_video(&VideoEncoderRequest {
                    component,
                    settings: &Settings::new(),
                    width: 2,
                    height: 2,
                    frame_rate: Rational::new(1, 1),
                    timescale: 1,
                    offered: &[PixelFormat::Rgba16],
                    color: &Color::bt709_limited(),
                })
                .is_err(),
            "{component} is not the x264 build's"
        );
    }
}
