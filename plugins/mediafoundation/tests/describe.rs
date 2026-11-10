//! What the plugin describes on this machine, in process and through the
//! built `celesta-codec-mediafoundation` program as Celesta starts it.
#![cfg(windows)]

use celesta_codec_mediafoundation::{DECODER_ID, MfCodec, gate};
use celesta_media::{ComponentKind, MediaProcess, MediaProvider, ProviderUse};
use std::path::PathBuf;

#[test]
fn this_machine_lists_the_basic_encoders() {
    // Windows 10 and 11 ship Microsoft's software H.264 and AAC encoders, so
    // with `CELESTA_REQUIRE_MEDIA_FOUNDATION=1` they must be available; a
    // system without Media Foundation skips.
    let mut codec = MfCodec::new();
    if !gate::available(
        &mut codec,
        &[DECODER_ID, "h264", "aac"],
        "this_machine_lists_the_basic_encoders",
    ) {
        return;
    }
    // HEVC needs a GPU driver or the Store's extension, so it may be missing.
    let described = codec.describe().unwrap();
    let hevc = described
        .iter()
        .find(|component| component.kind == ComponentKind::VideoEncoder && component.id == "hevc")
        .expect("hevc is described");
    println!(
        "hevc: available = {}, reason = {:?}",
        hevc.available, hevc.reason
    );
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
