//! The FFmpeg media plugin's process: serves the media role of the plugin
//! protocol on stdin and stdout until stdin closes, or, given
//! `--manifest`, prints its manifest and exits.

use celesta_codec_ffmpeg::{FfmpegCodec, PLUGIN_ID, printed_manifest};
use celesta_plugin_host::MANIFEST_FLAG;

fn main() {
    if std::env::args().nth(1).as_deref() == Some(MANIFEST_FLAG) {
        println!("{}", printed_manifest());
        return;
    }
    if let Err(error) = celesta_media::codec::protocol::serve(
        &mut FfmpegCodec::new(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}
