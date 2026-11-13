//! Serves this FFmpeg build or runs its explicit validation commands.
use celesta_codec_ffmpeg::{FfmpegCodec, run_plugin};
use celesta_codec_ffmpeg::{PLUGIN_ID, printed_manifest};

fn main() -> std::process::ExitCode {
    run_plugin(
        FfmpegCodec::new(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
        &printed_manifest(),
    )
}
