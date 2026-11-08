//! Choosing the media plugins Celesta reads and writes media with.

use celesta_media::{MediaProcess, MediaRegistry, RegisteredPlugin, registry, set_registry};
use celesta_plugin_host::InstalledPlugins;
use std::sync::Arc;

/// A registry of `plugins`' media plugins ([`InstalledPlugins::media`]), in
/// search order, each started as a [`MediaProcess`] when a backend first
/// needs it.
pub fn media_registry(plugins: &InstalledPlugins) -> Arc<MediaRegistry> {
    let registered = plugins
        .media()
        .into_iter()
        .map(|plugin| RegisteredPlugin {
            id: plugin.id,
            version: plugin.version,
            origin: plugin.origin,
            directory: plugin.directory.clone(),
            executable: Some(plugin.executable.clone()),
            media: plugin.media,
            factory: MediaProcess::factory(plugin.executable, plugin.directory),
        })
        .collect();
    Arc::new(MediaRegistry::new(registered))
}

/// Makes [`media_registry`] of `plugins` the registry every media backend
/// made from now on uses. Returns it.
pub fn use_media(plugins: &InstalledPlugins) -> Arc<MediaRegistry> {
    let media = media_registry(plugins);
    set_registry(Some(media.clone()));
    media
}

/// [`use_media`] with the plugins in every location Celesta searches, for a
/// tool that discovers none of its own, at start-up.
pub fn use_installed_media() {
    use_media(&InstalledPlugins::find_reporting(&[], &[]));
}

/// [`use_media`], unless a registry was chosen already: by the application,
/// or by a test that runs a plugin in its own process.
pub(crate) fn ensure_media(plugins: &InstalledPlugins) {
    if registry().is_none() {
        use_media(plugins);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_plugin_host::{CELESTA_VERSION, MANIFEST, UserSettings};

    #[test]
    fn registry_lists_media_plugins_in_order() {
        let root = tempfile::tempdir().unwrap();
        for id in ["first-media", "second-media"] {
            let directory = root.path().join(id);
            std::fs::create_dir_all(&directory).unwrap();
            let manifest = serde_json::json!({
                "id": id,
                "version": "1.0.0",
                "protocol": 2,
                "celesta": format!("={CELESTA_VERSION}"),
                "roles": ["media"],
                "executable": { std::env::consts::OS: "bin/p" },
                "media": { "decoders": [{ "id": "main", "name": "Main", "extensions": ["wav"] }] },
            });
            std::fs::write(directory.join(MANIFEST), manifest.to_string()).unwrap();
        }
        let (plugins, errors) = InstalledPlugins::find_with(
            &[root.path().to_path_buf()],
            &[],
            &UserSettings::default(),
        );
        assert!(errors.is_empty(), "{errors:?}");

        let media = media_registry(&plugins);
        assert_eq!(media.plugin(0).id, "first-media");
        assert_eq!(media.plugin(1).id, "second-media");
        assert!(media.has_decoders());
    }
}
