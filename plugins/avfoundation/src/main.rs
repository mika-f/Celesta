//! The AVFoundation media plugin's process: on macOS, serves the media role
//! of the plugin protocol on stdin and stdout until stdin closes; elsewhere,
//! says it runs on macOS only and fails.

use celesta_codec_avfoundation::PLUGIN_ID;

#[cfg(target_os = "macos")]
fn main() {
    use celesta_codec_avfoundation::AvfCodec;

    if let Err(error) = celesta_media::protocol::serve(
        &mut AvfCodec::default(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("{PLUGIN_ID}: this media plugin runs on macOS only");
    std::process::exit(1);
}
