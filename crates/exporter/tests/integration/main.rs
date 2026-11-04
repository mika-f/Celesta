//! One test binary for every integration test, so each links once.

mod cli_formats;
mod export;
mod inspect;
mod stills;

use celesta_exporter::{ExportOptions, Exporter};

/// An exporter that reads and writes media with the FFmpeg codec in this
/// process, so the tests need no codec plugin installed.
pub fn exporter(options: ExportOptions) -> Exporter {
    celesta_codec_ffmpeg::install();
    Exporter::new(options)
}
