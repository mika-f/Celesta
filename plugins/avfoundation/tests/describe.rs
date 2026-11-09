//! What the plugin describes on this Mac, in process and through the built
//! `celesta-codec-avfoundation` program as Celesta starts it.
#![cfg(target_os = "macos")]

use celesta_codec_avfoundation::AvfCodec;
use celesta_media::{ComponentKind, MediaProcess, MediaProvider, ProviderUse};
use std::path::PathBuf;

/// The components that may be missing on a Mac: VideoToolbox's HEVC and
/// ProRes encoders (a CI VM may have neither). Every other one runs on any
/// macOS: the AAC and H.264 encoders have software implementations, and the
/// decoder and muxers are AVFoundation's own.
const MAY_BE_MISSING: [&str; 2] = ["hevc", "prores"];

#[test]
fn this_mac_has_every_component() {
    let described = AvfCodec::default().describe().unwrap();
    assert!(!described.is_empty());
    let mut unavailable = Vec::new();
    for component in described.iter().filter(|component| !component.available) {
        let reason = component.reason.as_deref().unwrap_or("no reason");
        assert!(
            component.kind == ComponentKind::VideoEncoder
                && MAY_BE_MISSING.contains(&component.id.as_str()),
            "{:?} {} is unavailable on every Mac's frameworks: {reason}",
            component.kind,
            component.id
        );
        unavailable.push(format!("{} is unavailable: {reason}", component.id));
    }
    if !unavailable.is_empty() {
        println!(
            "skipping this_mac_has_every_component: {}",
            unavailable.join("; ")
        );
    }
}

#[test]
fn the_program_describes_what_the_library_does() {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-avfoundation"));
    let directory = executable.parent().unwrap().to_path_buf();
    let mut program =
        MediaProcess::factory(executable, directory)(&Default::default(), ProviderUse::Describe)
            .unwrap();
    assert_eq!(
        program.describe().unwrap(),
        AvfCodec::default().describe().unwrap()
    );
}
