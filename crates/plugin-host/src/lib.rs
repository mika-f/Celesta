//! Celesta's side of plugins: finds plugin directories
//! (`celesta-plugin.json`) in the locations every part of Celesta shares
//! ([`search_paths`], [`InstalledPlugins`]), decides which may run without
//! asking ([`UserSettings`], [`InstalledPlugins::allowed_by`]), and runs
//! each plugin a renderer needs in a process of its own, speaking
//! `celesta_plugin::protocol` with it. Install a [`PluginHost`] with
//! `GpuRenderer::with_content_provider`. Media plugins
//! ([`InstalledPlugins::media`]) are started by `celesta_media` instead.

mod approval;
mod child;
mod console;
mod host;
mod kill;
mod locations;
mod manifest;
mod media;
mod project;
mod settings;
pub mod stderr;

pub use approval::{
    Allowed, PendingPlugin, fingerprint, from_the_internet, zone_is_internet, zone_text,
};
pub use child::{HELLO_TIMEOUT, PluginChild, RequestError};
pub use console::without_console_window;
pub use host::PluginHost;
pub use kill::{PluginKillHandle, PluginKillSwitch, kill_every_plugin};
pub use locations::{
    PluginSource, SearchPath, bundled_plugin_dir, search_paths, unsafe_reason, user_plugin_dir,
};
pub use manifest::{
    CELESTA_VERSION, ComponentInfo, ComponentSource, DiscoveryError, InstalledPlugins, MANIFEST,
    Manifest, PLUGIN_PATH_ENV, PluginEntry, PluginRole, RuntimePlugins, WAITING_PLUGIN_PATH_ENV,
    celesta_range_for, discover, discover_in, is_contained_path, is_valid_id, manifest_path,
    platform, plugin_paths,
};
pub use media::{
    CODEC_PROGRAM, MANIFEST_FLAG, MediaDecoderEntry, MediaManifest, MediaOrigin, MediaPlugin,
    RESERVED_PLUGIN_ID,
};
pub use project::{
    PROJECT_CONFIG, ProjectConfigError, has_project_config, mark_project_root,
    project_plugin_paths, project_root,
};
pub use settings::{
    Allowance, AllowedPlugin, SETTINGS_FILE, SettingsError, UserSettings, create_private_dir_all,
    user_settings_dir, user_settings_path,
};
