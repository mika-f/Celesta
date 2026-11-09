//! What the plugin describes on this Mac, in process and through the built
//! `celesta-codec-avfoundation` program as Celesta starts it.
#![cfg(target_os = "macos")]

use celesta_codec_avfoundation::AvfCodec;
use celesta_media::{MediaProcess, MediaProvider, ProviderUse};
use std::path::PathBuf;

#[test]
fn this_mac_has_every_component() {
    let described = AvfCodec::default().describe().unwrap();
    assert!(!described.is_empty());
    let unavailable: Vec<String> = described
        .iter()
        .filter(|component| !component.available)
        .map(|component| {
            format!(
                "{} is unavailable: {}",
                component.id,
                component.reason.as_deref().unwrap_or("no reason")
            )
        })
        .collect();
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
