//! Optional GPL FFmpeg H.264 video encoder (libx264), served in a separate process.
//! Shared FFmpeg plumbing has MIT/Apache source; this program links GPL FFmpeg.

use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_media::{MediaProvider, RegisteredPlugin};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use std::sync::Arc;

/// The plugin's identity in manifests and the protocol handshake.
pub const PLUGIN_ID: &str = "ffmpeg-x264";

/// Manifest for installed binaries and plugin registration.
pub fn printed_manifest() -> String {
    let mut manifest: serde_json::Value =
        serde_json::from_str(include_str!("../plugin.json")).expect("valid plugin.json");
    manifest
        .as_object_mut()
        .expect("manifest object")
        .retain(|key, _| matches!(key.as_str(), "id" | "version" | "roles" | "media"));
    manifest.to_string()
}

/// Registers only this build's encoder; the standard plugin owns decoding and muxing.
pub fn registered() -> RegisteredPlugin {
    let plugin: serde_json::Value =
        serde_json::from_str(&printed_manifest()).expect("valid manifest");
    let (media, problems) = MediaManifest::from_json(plugin["media"].clone());
    assert!(problems.is_empty(), "invalid manifest: {problems:?}");
    RegisteredPlugin {
        id: PLUGIN_ID.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        origin: MediaOrigin::Bundled,
        directory: Default::default(),
        executable: None,
        media,
        factory: Arc::new(|_, _| Ok(Box::new(FfmpegCodec::x264()) as Box<dyn MediaProvider>)),
    }
}
