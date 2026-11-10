//! Celesta's Windows media plugin: H.264 and HEVC encoders and an AAC
//! encoder on Media Foundation transforms, and a decoder on its source
//! reader. It declares no muxer.
//!
//! The `celesta-codec-mediafoundation` program serves [`MfCodec`] over the
//! media role of the plugin protocol (`celesta_media::protocol`). It links
//! Windows' system libraries only. Its components, their settings, and what
//! `describe` says of them are platform-independent (`components`), so the
//! crate builds everywhere; everything that calls Media Foundation is
//! Windows only, and elsewhere the program only says so. Tests and tools may
//! also use [`MfCodec`] in their own process, through [`install`].

/// The id the plugin's manifest and `hello` give.
pub const PLUGIN_ID: &str = "mediafoundation";

/// The id of the plugin's one decoder.
pub const DECODER_ID: &str = "mediafoundation";

#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows provider uses the components outside their tests"
    )
)]
mod components;

// Nothing outside its tests calls it yet: Task 5's Windows AAC encoder
// does, and then this narrows to `not(any(windows, test))`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only the Windows AAC encoder uses it outside its tests"
    )
)]
mod aac_user_data;

#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows encoders use it outside its tests"
    )
)]
mod annexb;

#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows encoders use it outside its tests"
    )
)]
mod color;

#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows encoders use it outside its tests"
    )
)]
mod convert;

#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows encoders use it outside its tests"
    )
)]
mod hns;

#[cfg_attr(
    not(any(windows, test)),
    expect(
        dead_code,
        reason = "only the Windows encoders use it outside its tests"
    )
)]
mod params;

// Named `mf`, not `windows`, which would shadow the `windows` crate.
#[cfg(windows)]
mod mf;

#[cfg(windows)]
pub use mf::provider::{MfCodec, install, registered};
