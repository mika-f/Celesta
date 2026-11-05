//! One test binary for every integration test, so each links once.

mod cli_formats;
mod export;
mod inspect;
mod stills;

use celesta_exporter::{ExportOptions, Exporter};

/// An exporter that reads and writes media with the lossless test codec in
/// this process, so the tests need no codec plugin installed and can compare
/// what the encoder was given byte for byte.
pub fn exporter(options: ExportOptions) -> Exporter {
    celesta_codec_test::install();
    Exporter::new(options)
}
