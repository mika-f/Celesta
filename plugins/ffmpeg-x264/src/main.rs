//! Process boundary for the optional GPL encoders.
use celesta_codec_ffmpeg::FfmpegCodec;
use celesta_codec_ffmpeg_x264::{PLUGIN_ID, printed_manifest};
use celesta_plugin_host::MANIFEST_FLAG;

fn main() {
    if std::env::args().nth(1).as_deref() == Some(MANIFEST_FLAG) {
        println!("{}", printed_manifest());
        return;
    }
    if let Err(error) = celesta_media::protocol::serve(
        &mut FfmpegCodec::x264(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}
