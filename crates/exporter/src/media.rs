//! Choosing the media plugins Celesta reads and writes media with.

use celesta_media::{MediaProcess, MediaRegistry, RegisteredPlugin, registry, set_registry};
use celesta_plugin_host::{InstalledPlugins, RESERVED_PLUGIN_ID};
use std::sync::Arc;

/// A registry of `plugins`' media plugins ([`InstalledPlugins::media`]), in
/// search order, each started as a [`MediaProcess`] when a backend first
/// needs it, knowing the recognized programs discovery left out
/// ([`MediaRegistry::with_skipped`]).
pub fn media_registry(plugins: &InstalledPlugins) -> Arc<MediaRegistry> {
    let discovery = plugins.media_discovery();
    let registered: Vec<RegisteredPlugin> = discovery
        .plugins
        .into_iter()
        .map(|plugin| RegisteredPlugin {
            name: plugin.name,
            id: plugin.id,
            version: plugin.version,
            origin: plugin.origin,
            directory: plugin.directory.clone(),
            executable: Some(plugin.executable.clone()),
            media: plugin.media,
            factory: MediaProcess::factory(plugin.executable, plugin.directory),
        })
        .collect();
    Arc::new(builtin_registry(registered).with_skipped(discovery.skipped))
}

/// `plugins` after the built-in components, unless one of them already has
/// the reserved id.
pub fn with_builtin(plugins: Vec<RegisteredPlugin>) -> Arc<MediaRegistry> {
    Arc::new(builtin_registry(plugins))
}

fn builtin_registry(plugins: Vec<RegisteredPlugin>) -> MediaRegistry {
    let mut all = Vec::with_capacity(plugins.len() + 1);
    if !plugins.iter().any(|plugin| plugin.id == RESERVED_PLUGIN_ID) {
        all.push(celesta_media_image::registered());
    }
    all.extend(plugins);
    MediaRegistry::new(all)
}

/// Makes [`media_registry`] of `plugins` the registry every media backend
/// made from now on uses. Returns it.
pub fn use_media(plugins: &InstalledPlugins) -> Arc<MediaRegistry> {
    let media = media_registry(plugins);
    set_registry(Some(media.clone()));
    media
}

/// [`use_media`] with the plugins in every location Celesta searches, for a
/// tool that discovers none of its own, at start-up. Returns the registry.
pub fn use_installed_media() -> Arc<MediaRegistry> {
    use_media(&InstalledPlugins::find_reporting(&[], &[]))
}

/// The registry exports write through: the one chosen, or an empty one,
/// whose exports then find no muxer.
pub(crate) fn current() -> Arc<MediaRegistry> {
    registry().unwrap_or_else(|| with_builtin(Vec::new()))
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
        assert_eq!(media.plugin(0).id, "builtin");
        assert_eq!(
            media.plugin(0).origin,
            celesta_plugin_host::MediaOrigin::BuiltIn
        );
        assert_eq!(media.plugin(1).id, "first-media");
        assert_eq!(media.plugin(2).id, "second-media");
        assert_eq!(media.plugins().len(), 3);
        assert!(media.has_decoders());
    }

    #[test]
    fn with_builtin_adds_no_second_builtin() {
        let once = with_builtin(Vec::new());
        let plugins = vec![once.plugin(0).clone()];
        let twice = with_builtin(plugins);
        assert_eq!(twice.plugins().len(), 1);
    }
}
