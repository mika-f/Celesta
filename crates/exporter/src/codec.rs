//! Choosing the codec plugin Celesta reads and writes media with.

use celesta_media::{CodecProcess, set_codec};
use celesta_plugin_host::{CodecPlugin, InstalledPlugins};

/// Makes `plugins`' codec ([`InstalledPlugins::codec`]) the one every media
/// backend made from now on starts, or leaves none when they have none.
/// Returns the codec chosen.
pub fn use_codec(plugins: &InstalledPlugins) -> Option<CodecPlugin> {
    let codec = plugins.codec();
    set_codec(
        codec
            .as_ref()
            .map(|codec| CodecProcess::factory(codec.executable.clone(), codec.directory.clone())),
    );
    codec
}

/// [`use_codec`] with the plugins in every location Celesta searches, for a
/// tool that discovers none of its own, at start-up.
pub fn use_installed_codec() {
    use_codec(&InstalledPlugins::find_reporting(&[], &[]));
}

/// [`use_codec`], unless a codec was chosen already: by the application, or
/// by a test that runs one in its own process.
pub(crate) fn ensure_codec(plugins: &InstalledPlugins) {
    if celesta_media::codec::codec().is_none() {
        use_codec(plugins);
    }
}
