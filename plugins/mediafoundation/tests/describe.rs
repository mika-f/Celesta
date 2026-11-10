//! What the plugin describes on this machine, in process and through the
//! built `celesta-codec-mediafoundation` program as Celesta starts it.
#![cfg(windows)]

use celesta_codec_mediafoundation::MfCodec;
use celesta_media::{ComponentKind, MediaProcess, MediaProvider, ProviderUse};
use std::path::PathBuf;

#[test]
fn this_machine_lists_the_basic_encoders() {
    let described = MfCodec::new().describe().unwrap();
    let state = |kind: ComponentKind, id: &str| {
        let component = described
            .iter()
            .find(|component| component.kind == kind && component.id == id)
            .unwrap_or_else(|| panic!("{id} is not described"));
        (component.available, component.reason.clone())
    };
    // Windows 10 and 11 ship Microsoft's software H.264 and AAC encoders.
    for (kind, id) in [
        (ComponentKind::VideoEncoder, "h264"),
        (ComponentKind::AudioEncoder, "aac"),
    ] {
        let (available, reason) = state(kind, id);
        assert!(available, "{id} is unavailable: {reason:?}");
    }
    let (available, reason) = state(ComponentKind::Decoder, "mediafoundation");
    assert!(available, "the decoder is unavailable: {reason:?}");
    // HEVC needs a GPU driver or the Store's extension, so it may be missing.
    let (available, reason) = state(ComponentKind::VideoEncoder, "hevc");
    println!("hevc: available = {available}, reason = {reason:?}");
}

#[test]
fn the_program_describes_what_the_library_does() {
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-mediafoundation"));
    let directory = executable.parent().unwrap().to_path_buf();
    let mut program =
        MediaProcess::factory(executable, directory)(&Default::default(), ProviderUse::Describe)
            .unwrap();
    assert_eq!(
        program.describe().unwrap(),
        MfCodec::new().describe().unwrap()
    );
}

#[test]
fn the_plugin_declares_no_muxer() {
    // Task 1 ruled B (deviation 71): the encoders go into `ffmpeg/mp4`.
    let mut codec = MfCodec::new();
    assert!(
        codec
            .describe()
            .unwrap()
            .iter()
            .all(|component| component.kind != ComponentKind::Muxer)
    );
    assert!(codec.muxer().is_none());
}
