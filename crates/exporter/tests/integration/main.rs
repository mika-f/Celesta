//! One test binary for every integration test, so each links once.

mod cli_formats;
mod export;
mod inspect;
mod stills;

use std::path::Path;
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

/// Whether the React runtime `cli` (`packages/cli/dist/cli.js`) is built.
/// If not, the test skips, or fails when `CELESTA_REQUIRE_RUNTIME=1` says
/// the runtime was built.
pub fn runtime_built(cli: &Path, test: &str) -> bool {
    let built = cli.exists();
    if !built {
        assert!(
            std::env::var("CELESTA_REQUIRE_RUNTIME").as_deref() != Ok("1"),
            "React tests require the built runtime: {} is missing",
            cli.display()
        );
        eprintln!("skipping {test}: build packages/cli first");
    }
    built
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
