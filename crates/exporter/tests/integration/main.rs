//! One test binary for every integration test that needs nothing but the
//! Rust build, so each links once. The tests of React entries, which need
//! the built React runtime, are the `runtime` target (`tests/runtime/`).

mod cli_formats;
#[path = "../common/mod.rs"]
mod common;
mod export;
mod inspect;
mod stills;

use std::sync::{Arc, OnceLock};

use celesta_exporter::{ExportOptions, Exporter};
use celesta_media::MediaRegistry;

/// An exporter that reads and writes media with the lossless test codec in
/// this process, so the tests need no codec plugin installed and can compare
/// what the encoder was given byte for byte.
pub fn exporter(options: ExportOptions) -> Exporter {
    install_media();
    Exporter::new(options)
}

/// Makes [`media`] the registry every export in this process uses. Tests
/// run in parallel and each sets it, so it is always the same one.
pub fn install_media() {
    celesta_media::set_registry(Some(media().clone()));
}

/// The test codec, in this process, and on Unix a media plugin `hang`
/// beside it (`export::hanging_media_plugin`), which only an export naming
/// its video encoder uses.
fn media() -> &'static Arc<MediaRegistry> {
    static MEDIA: OnceLock<Arc<MediaRegistry>> = OnceLock::new();
    MEDIA.get_or_init(|| {
        #[allow(unused_mut)]
        let mut plugins = vec![celesta_codec_test::registered()];
        #[cfg(unix)]
        plugins.push(export::hanging_media_plugin());
        Arc::new(MediaRegistry::new(plugins))
    })
}
