//! Celesta's macOS media plugin: H.264, HEVC, and ProRes encoders on
//! VideoToolbox, an AAC encoder on AudioToolbox, MP4 and QuickTime muxers on
//! `AVAssetWriter`, and a decoder on `AVAssetReader`.
//!
//! The `celesta-codec-avfoundation` program serves [`AvfCodec`] over the
//! media role of the plugin protocol (`celesta_media::protocol`). It links
//! Apple's system frameworks only. Its components, their settings, and what
//! `describe` says of them are platform-independent (`components`), so the
//! crate builds everywhere; everything that calls a framework is macOS only,
//! and elsewhere the program only says so. Tests and tools may also use
//! [`AvfCodec`] in their own process, through [`install`].

/// The id the plugin's manifest and `hello` give.
pub const PLUGIN_ID: &str = "avfoundation";

/// The id of the plugin's one decoder.
pub const DECODER_ID: &str = "avfoundation";

#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS provider uses the components outside their tests"
    )
)]
mod components;

#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS video encoders check colours outside their tests"
    )
)]
mod color;

// Platform-independent: read on every system, tests included.
#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS AAC encoder and muxers read and write cookies outside their tests"
    )
)]
mod esds;
#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS decoder names codecs and colours outside their tests"
    )
)]
mod names;
#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS VideoToolbox encoders read parameter sets outside their tests"
    )
)]
mod params;

#[cfg(target_os = "macos")]
mod apple;
#[cfg(target_os = "macos")]
mod audio;
#[cfg(target_os = "macos")]
mod decoder;
#[cfg(target_os = "macos")]
mod formats;
#[cfg(target_os = "macos")]
mod muxer;
#[cfg(target_os = "macos")]
mod provider;
#[cfg(target_os = "macos")]
mod video;

#[cfg(target_os = "macos")]
pub use provider::{AvfCodec, install, registered};
