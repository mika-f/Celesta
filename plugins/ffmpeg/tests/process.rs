//! The default process must expose LGPL components only.
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
    assert_eq!(components.len(), 7);
    assert!(components.iter().all(|component| component.id != "libx264"));
    let declared = registered();
    assert_eq!(declared.media.video_encoders.len(), 1);
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
fn optional_providers_expose_only_their_encoder() {
    for (mut provider, id) in [
        (FfmpegCodec::x264(), "libx264"),
        (FfmpegCodec::opus(), "opus"),
        (FfmpegCodec::vpx(), "vp9"),
    ] {
        let described = provider.describe().unwrap();
        assert_eq!(described.len(), 1);
        assert_eq!(described[0].id, id);
        assert!(provider.decoder().is_none());
        assert!(provider.muxer().is_none());
    }
}
