//! Serves this FFmpeg build or runs its explicit validation commands.
use celesta_codec_ffmpeg::{FfmpegCodec, run_plugin};
use celesta_codec_ffmpeg_opus::{PLUGIN_ID, printed_manifest};

fn main() {
    if let Err(error) = run_plugin(
        FfmpegCodec::opus(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
        &printed_manifest(),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
