//! Serves this FFmpeg build or runs its explicit validation commands.
use celesta_codec_ffmpeg::{FfmpegCodec, run_plugin};
use celesta_codec_ffmpeg_x264::{PLUGIN_ID, printed_manifest};

fn main() -> std::process::ExitCode {
    run_plugin(
        FfmpegCodec::x264(),
        PLUGIN_ID,
        env!("CARGO_PKG_VERSION"),
        &printed_manifest(),
    )
}
