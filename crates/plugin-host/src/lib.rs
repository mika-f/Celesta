//! Celesta's side of content plugins: finds plugin directories
//! (`celesta-plugin.json`) in the locations every part of Celesta shares
//! ([`search_paths`], [`InstalledPlugins`]) and runs each plugin a renderer
//! needs in a process of its own, speaking `celesta_plugin::protocol` with
//! it. Install a [`PluginHost`] with `GpuRenderer::with_content_provider`.

mod host;
mod locations;
mod manifest;

pub use host::PluginHost;
pub use locations::{
    PluginSource, SearchPath, bundled_plugin_dir, search_paths, unsafe_reason, user_plugin_dir,
};
pub use manifest::{
    DiscoveryError, InstalledPlugins, MANIFEST, Manifest, PLUGIN_PATH_ENV, PluginEntry, discover,
    discover_in, is_contained_path, is_valid_id, platform, plugin_paths,
};
