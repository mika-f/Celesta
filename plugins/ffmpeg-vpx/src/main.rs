//! Process boundary for the optional LGPL encoders.
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_codec_ffmpeg_vpx::{PLUGIN_ID, printed_manifest};
use celesta_plugin_host::MANIFEST_FLAG;

fn main() {
    if let Err(error) = celesta_codec_ffmpeg::check_lgpl() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    if std::env::args().nth(1).as_deref() == Some(MANIFEST_FLAG) {
        println!("{}", printed_manifest());
        return;
    }
    if let Err(error) = celesta_media::protocol::serve(
        &mut FfmpegCodec::vpx(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}
