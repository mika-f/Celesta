//! Celesta's side of content plugins: finds plugin directories
//! (`celesta-plugin.json`) and runs each plugin a renderer needs in a
//! process of its own, speaking `celesta_plugin::protocol` with it. Install
//! a [`PluginHost`] with `GpuRenderer::with_content_provider`.

mod host;
mod manifest;

pub use host::PluginHost;
pub use manifest::{
    DiscoveryError, MANIFEST, Manifest, PLUGIN_PATH_ENV, PluginEntry, discover, platform,
    plugin_paths,
};
