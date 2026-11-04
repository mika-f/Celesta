//! The FFmpeg codec plugin's process: serves the codec protocol on stdin
//! and stdout until stdin closes.

use celesta_codec_ffmpeg::{FfmpegCodec, PLUGIN_ID};

fn main() {
    if let Err(error) = celesta_media::codec::protocol::serve(
        &mut FfmpegCodec::new(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}
