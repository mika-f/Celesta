//! Plugin directories: a `celesta-plugin.json` and what it names.

use crate::locations::{PluginSource, SearchPath, search_paths, unsafe_reason};
use crate::media::{MediaManifest, RESERVED_PLUGIN_ID, deserialize_media, media_problems};
use crate::settings::UserSettings;
use celesta_plugin::protocol_for_roles;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// The file that makes a directory a plugin.
pub const MANIFEST: &str = "celesta-plugin.json";

/// The environment variable listing where plugins are, as the platform's
/// path list (`;` on Windows, `:` elsewhere).
pub const PLUGIN_PATH_ENV: &str = "CELESTA_PLUGIN_PATH";

/// The environment variable listing, like [`PLUGIN_PATH_ENV`], the plugins
/// that wait for the user's approval. The React runtime runs none of their
/// code: it resolves their imports to stand-ins or fails them, as
/// [`WAITING_IMPORTS_ENV`] says.
pub const WAITING_PLUGIN_PATH_ENV: &str = "CELESTA_WAITING_PLUGIN_PATH";

/// The environment variable saying what the React runtime does with an
/// import of a plugin that waits for approval ([`WaitingImports`]):
/// `stand-in`, or anything else (`fail`) to fail the import.
pub const WAITING_IMPORTS_ENV: &str = "CELESTA_WAITING_PLUGIN_IMPORTS";

/// What an entry's import of a plugin that waits for approval resolves to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaitingImports {
    /// The import fails, saying the plugin waits and how to allow it, so a
    /// render without the user watching never leaves its layers out
    /// unseen: what command-line tools and the MCP server do.
    #[default]
    Fail,
    /// A stand-in that draws nothing of its own and runs none of the
    /// plugin's code, so the app can open the project while it asks.
    StandIn,
}

impl WaitingImports {
    /// Its [`WAITING_IMPORTS_ENV`] value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fail => "fail",
            Self::StandIn => "stand-in",
        }
    }
}

/// This Celesta's version, which every Celesta crate shares: a plugin runs
/// only when its manifest's `celesta` range includes it.
pub const CELESTA_VERSION: &str = env!("CARGO_PKG_VERSION");

/// [`CELESTA_VERSION`], parsed.
fn celesta_version() -> semver::Version {
    semver::Version::parse(CELESTA_VERSION).expect("Cargo package versions are semver")
}

/// The range a plugin built against Celesta `version` declares: that
/// version (without build metadata) up to the next release that may break
/// it, as Cargo's `^` reads (the next patch below 0.1, the next minor below
/// 1.0, the next major from it), such as `">=0.9.0, <0.10.0"`.
/// `scripts/plugins/build.mts` writes the same.
pub fn celesta_range_for(version: &semver::Version) -> String {
    let lowest = semver::Version {
        build: semver::BuildMetadata::EMPTY,
        ..version.clone()
    };
    let next = if version.major == 0 && version.minor == 0 {
        format!("0.0.{}", version.patch + 1)
    } else if version.major == 0 {
        format!("0.{}.0", version.minor + 1)
    } else {
        format!("{}.0.0", version.major + 1)
    };
    format!(">={lowest}, <{next}")
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: String,
    /// The wire format its process speaks, which `hello` checks: features
    /// added on top of it are what `celesta` covers.
    pub protocol: u32,
    /// The Celesta versions the plugin works with, as a Cargo version
    /// requirement such as `">=0.9.0, <0.10.0"`. It covers the features its
    /// process relies on and the runtime packages (`@celesta.studio/*`) its
    /// module imports, which ship with Celesta at its version. Required:
    /// discovery skips a plugin without one, and reports one whose range
    /// leaves this Celesta ([`CELESTA_VERSION`]) out as incompatible.
    #[serde(default)]
    pub celesta: Option<String>,
    /// The plugin process, relative to the plugin directory, by platform:
    /// `"<os>-<arch>"` (such as `windows-x86_64` or `macos-aarch64`) or the
    /// OS alone for any architecture. See [`platform`].
    #[serde(default)]
    pub executable: BTreeMap<String, String>,
    /// The JavaScript module entries import as `@celesta-plugin/<id>`.
    #[serde(default)]
    pub module: Option<String>,
    /// Its declarations, for project TypeScript support.
    #[serde(default)]
    pub types: Option<String>,
    /// Components of `module` the editor's Add menu offers.
    #[serde(default)]
    pub components: Vec<ComponentInfo>,
    /// What the plugin's process does; content alone when absent. A plugin
    /// is either `content` or `media`, never both.
    #[serde(default = "content_roles")]
    pub roles: Vec<PluginRole>,
    /// The components a media plugin offers. Read leniently: a malformed
    /// component is left out, and discovery reports it
    /// ([`DiscoveryError::BadComponent`]).
    #[serde(default, deserialize_with = "deserialize_media")]
    pub media: Option<MediaManifest>,
}

/// What a plugin's process does, as its manifest and its `hello` say.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PluginRole {
    /// Draws plugin layers (`celesta_plugin::ContentProvider`).
    Content,
    /// Reads media files with the decoders its manifest's `media` declares,
    /// speaking plugin protocol 2 (`celesta_plugin::MEDIA_PROTOCOL_VERSION`).
    Media,
    /// A role this Celesta does not know, which it leaves alone, such as
    /// `codec`, which `media` replaced.
    #[serde(other)]
    Unknown,
}

impl PluginRole {
    /// Its name in manifests and `hello`, for a role this Celesta knows.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Content => Some("content"),
            Self::Media => Some("media"),
            Self::Unknown => None,
        }
    }
}

fn content_roles() -> Vec<PluginRole> {
    vec![PluginRole::Content]
}

impl Manifest {
    /// Its `celesta` range, or why it has none Celesta can read.
    fn celesta_range(&self) -> Result<semver::VersionReq, String> {
        let Some(range) = &self.celesta else {
            return Err(format!(
                "it does not say which Celesta versions it works with: its manifest needs a \
                 \"celesta\" range, such as {:?} for this Celesta",
                celesta_range_for(&celesta_version())
            ));
        };
        semver::VersionReq::parse(range).map_err(|error| {
            format!("its \"celesta\" range {range:?} is not a version range: {error}")
        })
    }

    /// Whether its `celesta` range includes Celesta `version`, as Cargo
    /// matches versions: a pre-release such as `0.10.0-beta.1` only when a
    /// bound of the range names a pre-release of `0.10.0`. False when it has
    /// no range Celesta can read.
    pub fn works_with(&self, version: &semver::Version) -> bool {
        self.celesta_range()
            .is_ok_and(|range| range.matches(version))
    }
}

/// A component a plugin's module exports, as the editor's Add menu offers
/// it: drawn on the canvas like an Image or a Video when it takes a file as
/// `src` (`src`, `x`, `y`, `width`), else like a Rect (`x`, `y`, `width`,
/// `height`), and written with `props` after those.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    /// The export's name: a JSX component name such as `Flag`.
    pub name: String,
    /// What the Add menu says; the name when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// The kind of file the component takes as `src`, chosen when it is
    /// added.
    #[serde(default)]
    pub src: Option<ComponentSource>,
    /// Props the component is added with: strings, numbers, and booleans.
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
    /// The file types a model component takes: lowercase, without the dot.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// A model component's layer width over its height, for drawing it:
    /// required of a model component, which is added with only a width,
    /// and refused on any other.
    #[serde(default)]
    pub aspect: Option<f64>,
}

/// The widest a model component's `aspect` may be, and one over it the
/// narrowest: a scene canvas side is at most `MAX_SCENE_CANVAS_SIDE`
/// pixels and at least 1, so no wider layer could be drawn.
const MAX_COMPONENT_ASPECT: f64 = celesta_plugin::scene::MAX_SCENE_CANVAS_SIDE as f64;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ComponentSource {
    Image,
    Video,
    /// A file the plugin reads itself, of the types `extensions` lists.
    Model,
}

impl ComponentInfo {
    /// What the Add menu says.
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// The props the editor writes from the drawn box, which `props` may not
    /// set again.
    pub fn drawn_props(&self) -> &'static [&'static str] {
        match self.src {
            Some(_) => &["src", "x", "y", "width"],
            None => &["x", "y", "width", "height"],
        }
    }

    /// Why the component cannot be offered, if it cannot.
    fn invalid_reason(&self) -> Option<String> {
        let name = &self.name;
        let mut chars = name.chars();
        if !(chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            return Some(format!(
                "component {name:?} is not a component name (a capital letter, then letters, digits, or '_')"
            ));
        }
        if self.src == Some(ComponentSource::Model) {
            if self.extensions.is_empty() {
                return Some(format!(
                    "component {name:?} takes a model file but lists no extensions"
                ));
            }
        } else if !self.extensions.is_empty() {
            return Some(format!(
                "component {name:?} lists extensions but does not take a model file"
            ));
        } else if self.aspect.is_some() {
            return Some(format!(
                "component {name:?} lists an aspect but does not take a model file"
            ));
        }
        for (index, extension) in self.extensions.iter().enumerate() {
            if let Some(bare) = extension.strip_prefix('.') {
                return Some(format!(
                    "component {name:?}'s extension {extension:?} is written with its dot; list it as {bare:?}"
                ));
            }
            if self.extensions[..index].contains(extension) {
                return Some(format!(
                    "component {name:?} lists the extension {:?} twice",
                    format!(".{extension}")
                ));
            }
            let plain = !extension.is_empty()
                && extension
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
            if !plain {
                return Some(format!(
                    "component {name:?}'s extension {:?} is not lowercase letters and digits",
                    format!(".{extension}")
                ));
            }
        }
        // Added with only a width, a model component without an aspect
        // would draw a box whose height the layer does not keep.
        if self.src == Some(ComponentSource::Model) && self.aspect.is_none() {
            return Some(format!(
                "component {name:?} takes a model file but gives no aspect"
            ));
        }
        if let Some(aspect) = self.aspect
            && !(MAX_COMPONENT_ASPECT.recip()..=MAX_COMPONENT_ASPECT).contains(&aspect)
        {
            return Some(format!(
                "component {name:?}'s aspect {aspect} is not from 1/{MAX_COMPONENT_ASPECT} to {MAX_COMPONENT_ASPECT}"
            ));
        }
        for (prop, value) in &self.props {
            let mut chars = prop.chars();
            let is_name = chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !is_name || self.drawn_props().contains(&prop.as_str()) {
                return Some(format!("component {name:?} cannot set the prop {prop:?}"));
            }
            if !(value.is_string() || value.is_number() || value.is_boolean()) {
                return Some(format!(
                    "component {name:?}'s prop {prop:?} is not a string, number, or boolean"
                ));
            }
        }
        None
    }
}

/// This platform as manifests name it: `(os, arch)` in Rust's names, such
/// as `windows`, `macos`, or `linux` and `x86_64` or `aarch64`.
pub const fn platform() -> (&'static str, &'static str) {
    (std::env::consts::OS, std::env::consts::ARCH)
}

/// A plugin found on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginEntry {
    pub manifest: Manifest,
    pub directory: PathBuf,
    /// Which search path it was found under.
    pub source: PluginSource,
}

impl PluginEntry {
    /// This platform's plugin process, if the plugin has one: the entry for
    /// this OS and architecture, else for this OS.
    pub fn executable(&self) -> Option<PathBuf> {
        let (os, arch) = platform();
        let executables = &self.manifest.executable;
        executables
            .get(&format!("{os}-{arch}"))
            .or_else(|| executables.get(os))
            .map(|relative| self.file(relative))
    }

    pub fn module(&self) -> Option<PathBuf> {
        self.manifest
            .module
            .as_ref()
            .map(|module| self.file(module))
    }

    pub fn types(&self) -> Option<PathBuf> {
        self.manifest.types.as_ref().map(|types| self.file(types))
    }

    /// The file a path in the manifest names (see [`manifest_path`]).
    pub fn file(&self, path: &str) -> PathBuf {
        manifest_path(&self.directory, path)
    }

    pub fn has_role(&self, role: PluginRole) -> bool {
        self.manifest.roles.contains(&role)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a plugin manifest: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} was skipped: {reason}")]
    Invalid { path: PathBuf, reason: String },
    #[error(
        "{path} was skipped: the plugin {id} works with Celesta {range}, and this is Celesta {celesta}"
    )]
    Incompatible {
        path: PathBuf,
        id: String,
        /// The plugin's `celesta` range.
        range: String,
        /// This Celesta's version.
        celesta: String,
    },
    #[error("plugin path {path} was skipped: it is not an absolute path")]
    RelativePath { path: PathBuf },
    #[error(
        "{path} was skipped: the plugin {id} {} in the Celesta app; allow it in the app's Plugins tab, or name its folder in CELESTA_PLUGIN_PATH; celesta-exporter also takes --allow-plugin {id} and --plugin-dir",
        if *.changed { "changed since you allowed it, and waits for your approval again" } else { "waits for your approval" }
    )]
    WaitsForApproval {
        path: PathBuf,
        id: String,
        /// Allowed once, but changed since.
        changed: bool,
    },
    #[error(
        "--allow-plugin {id} names no plugin that can run: no folder has one with that id, or it was skipped (see why above)"
    )]
    UnknownAllowance { id: String },
    /// A plugin whose files could not be read to see whether the user
    /// allowed it as it is now, with why.
    #[error("{0}")]
    Unreadable(String),
    #[error("the user's settings were not used: {0}")]
    Settings(String),
    /// A component the plugin's manifest declares that cannot be read: it
    /// is left out, and the plugin's other components are kept.
    #[error("{path}: a media component was skipped: {reason}")]
    BadComponent { path: PathBuf, reason: String },
}

/// Whether `id` can name a plugin: lowercase ASCII letters, digits, `-`,
/// `_`, and `.`, starting with a letter or digit. That keeps
/// `@celesta-plugin/<id>` one specifier segment and `<id>` one path
/// component wherever Celesta files things under it.
pub fn is_valid_id(id: &str) -> bool {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mut chars = id.chars();
    chars.next().is_some_and(allowed) && chars.all(|c| allowed(c) || matches!(c, '-' | '_' | '.'))
}

/// Whether a path a manifest names stays inside the plugin directory on
/// every platform: relative, with no `..` part, drive, or `:` (which would
/// also name an NTFS stream). Either separator counts, since one manifest
/// serves every platform.
pub fn is_contained_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains([':', '\0'])
        && path.split(['/', '\\']).all(|part| part != "..")
}

/// The file `path`, a path a manifest names, is under `directory`. It is
/// split on either separator, as [`is_contained_path`] reads it, and its
/// parts are joined with this platform's own: one manifest serves every
/// platform, so `js\\index.js` is `js/index.js` on macOS and Linux (where
/// it would otherwise be one file name), and `bin/p.exe` is `bin\\p.exe` on
/// Windows rather than a path of mixed separators. `.` and empty parts are
/// left out.
pub fn manifest_path(directory: &Path, path: &str) -> PathBuf {
    let mut file = directory.to_path_buf();
    file.extend(
        path.split(['/', '\\'])
            .filter(|part| !part.is_empty() && *part != "."),
    );
    file
}

/// Where to look for plugins: `extra` first, made absolute against the
/// current directory, then `CELESTA_PLUGIN_PATH`. [`discover`] skips a
/// relative entry of the latter, whose folder would depend on where Celesta
/// happens to be started, as `.` on `PATH` does.
pub fn plugin_paths(extra: &[PathBuf]) -> Vec<PathBuf> {
    let from_env = std::env::var_os(PLUGIN_PATH_ENV).unwrap_or_default();
    extra
        .iter()
        .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.clone()))
        .chain(std::env::split_paths(&from_env).filter(|path| !path.as_os_str().is_empty()))
        .collect()
}

/// Why `manifest` cannot be used from `directory`, if it cannot: a bad or
/// reserved id, both the content and media roles, no `celesta` range or one
/// that cannot be read, a component that cannot be offered (or components
/// without a module), or a path that leads outside the directory, by its
/// text or through a link. Files that do not exist yet are left to fail
/// where they are used.
fn invalid_reason(manifest: &Manifest, directory: &Path) -> Option<String> {
    if !is_valid_id(&manifest.id) {
        return Some(format!(
            "its id {:?} may use only lowercase letters, digits, '-', '_', and '.', \
             and must start with a letter or digit",
            manifest.id
        ));
    }
    if manifest.id == RESERVED_PLUGIN_ID {
        return Some(format!(
            "its id {RESERVED_PLUGIN_ID:?} is reserved for the components Celesta has built in"
        ));
    }
    if manifest.roles.contains(&PluginRole::Content) && manifest.roles.contains(&PluginRole::Media)
    {
        return Some(
            "it has both content and media roles; a plugin that does both ships as two plugins"
                .into(),
        );
    }
    if let Err(reason) = manifest.celesta_range() {
        return Some(reason);
    }
    if !manifest.components.is_empty() && manifest.module.is_none() {
        return Some("it lists components but has no module to import them from".into());
    }
    if let Some(reason) = manifest
        .components
        .iter()
        .find_map(ComponentInfo::invalid_reason)
    {
        return Some(reason);
    }
    let named = manifest
        .executable
        .values()
        .chain(&manifest.module)
        .chain(&manifest.types);
    // Without the folder's own resolved path, links out of it could not be
    // told apart, so such a folder is refused rather than trusted.
    let root = match std::fs::canonicalize(directory) {
        Ok(root) => root,
        Err(error) => return Some(format!("its folder could not be resolved: {error}")),
    };
    for path in named {
        if !is_contained_path(path) {
            return Some(format!(
                "{path:?} is not a relative path inside the plugin directory"
            ));
        }
        if let Ok(resolved) = std::fs::canonicalize(manifest_path(directory, path))
            && !resolved.starts_with(&root)
        {
            return Some(format!("{path:?} leads outside the plugin directory"));
        }
    }
    None
}

/// Whether `directory`'s path would break the path list the React runtime
/// gets (`CELESTA_PLUGIN_PATH`): it holds the list's separator (`;` on
/// Windows, `:` elsewhere), or a `"`, which Windows uses to quote such
/// entries and the runtime does not read. `join_paths` cannot be relied on
/// to say so: on Windows it quotes an entry with `;` instead of failing.
fn breaks_path_list(directory: &Path) -> bool {
    let text = directory.as_os_str().to_string_lossy();
    let separator = if cfg!(windows) { ';' } else { ':' };
    text.contains(separator) || text.contains('"')
}

/// Why a plugin from `source` cannot be used, if other users could change
/// it: `directory` or anything inside it, since a program loads libraries
/// beside it and a module imports files the manifest does not name (see
/// [`unsafe_reason`]). Only checked sources are judged, and only where
/// permissions are judged at all (Unix).
fn unsafe_plugin(directory: &Path, source: PluginSource) -> Option<String> {
    if !source.is_checked() || !cfg!(unix) {
        return None;
    }
    let mut folders = vec![directory.to_path_buf()];
    while let Some(folder) = folders.pop() {
        if let Some(reason) = unsafe_reason(&folder) {
            return Some(reason);
        }
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) => {
                return Some(format!("{} could not be read: {error}", folder.display()));
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    return Some(format!("{} could not be read: {error}", folder.display()));
                }
            };
            let path = entry.path();
            // Links are judged where they lead (`unsafe_reason` follows
            // them) but not walked into, so a link cannot loop the walk.
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => folders.push(path),
                _ => {
                    if let Some(reason) = unsafe_reason(&path) {
                        return Some(reason);
                    }
                }
            }
        }
    }
    None
}

/// The plugin in `directory`, if it has a manifest, with the components of
/// it that were skipped ([`DiscoveryError::BadComponent`]).
fn read_manifest(
    directory: &Path,
    source: PluginSource,
) -> Result<Option<(PluginEntry, Vec<DiscoveryError>)>, DiscoveryError> {
    let path = directory.join(MANIFEST);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(DiscoveryError::Io { path, source }),
    };
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|source| DiscoveryError::Manifest {
            path: path.clone(),
            source,
        })?;
    if let Some(reason) = invalid_reason(&manifest, directory) {
        return Err(DiscoveryError::Invalid { path, reason });
    }
    // Before the walk over its files, which a plugin this Celesta would not
    // run need not go through.
    let celesta = celesta_version();
    if !manifest.works_with(&celesta) {
        return Err(DiscoveryError::Incompatible {
            path,
            id: manifest.id,
            range: manifest.celesta.unwrap_or_default(),
            celesta: celesta.to_string(),
        });
    }
    // After the range, which says better what to do about a plugin made for
    // another Celesta: one whose range covers this Celesta should speak its
    // protocol, the one of its role. Its process would refuse `hello`; a
    // module alone is never asked.
    let roles: Vec<&str> = manifest
        .roles
        .iter()
        .filter_map(|role| role.name())
        .collect();
    let protocol = protocol_for_roles(&roles);
    if manifest.protocol != protocol {
        let kind = if manifest.roles.contains(&PluginRole::Media) {
            "media"
        } else {
            "content"
        };
        return Err(DiscoveryError::Invalid {
            path,
            reason: format!(
                "it speaks plugin protocol {}, and this Celesta speaks protocol {protocol} with {kind} plugins",
                manifest.protocol
            ),
        });
    }
    if let Some(reason) = unsafe_plugin(directory, source) {
        return Err(DiscoveryError::Invalid { path, reason });
    }
    // The React runtime gets each plugin directory in a path list, which a
    // directory with the list's separator in its name would split.
    if breaks_path_list(directory) {
        return Err(DiscoveryError::Invalid {
            path,
            reason: "its folder's path contains the path-list separator or a quote".into(),
        });
    }
    let skipped = media_problems(&text)
        .into_iter()
        .map(|reason| DiscoveryError::BadComponent {
            path: path.clone(),
            reason,
        })
        .collect();
    Ok(Some((
        PluginEntry {
            manifest,
            directory: directory.to_path_buf(),
            source,
        },
        skipped,
    )))
}

/// The plugins under `paths`, each an absolute path to a plugin directory
/// or a directory of them, all named explicitly (see [`discover_in`]).
pub fn discover(paths: &[PathBuf]) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
    let paths: Vec<_> = paths
        .iter()
        .map(|path| SearchPath {
            path: path.clone(),
            source: PluginSource::Explicit,
        })
        .collect();
    discover_in(&paths)
}

/// The plugins under `paths`, in order: each is an absolute path to a
/// plugin directory or a directory of them. The first plugin found with an
/// id wins, except that a project's plugin with the id of a media plugin in
/// the bundled folder is skipped and reported, so the bundled one is used
/// (see [`discover_in_with`]). Relative paths, folders and manifests that cannot be read,
/// manifests that name a bad id or a file outside their directory or give
/// no `celesta` range, plugins whose range leaves this Celesta out
/// ([`DiscoveryError::Incompatible`]), and, from a checked source, a folder
/// or plugin other users could change are reported, not fatal, as are the
/// components of a listed plugin that cannot be read
/// ([`DiscoveryError::BadComponent`]); a missing
/// folder is skipped quietly. A plugin skipped this way takes no id, so a
/// later one with the same id is found in its place.
pub fn discover_in(paths: &[SearchPath]) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
    discover_in_with(paths, |_| false)
}

/// [`discover_in`], where a plugin `turned_off` names is listed but, like a
/// skipped one, takes no id: a later plugin with its id is found too, to be
/// used in its place. One found after a plugin that took its id is not.
///
/// One exception to search order: a project's plugin with the id of a media
/// plugin Celesta bundles is skipped and reported, so a project cannot
/// replace the plugin that reads and writes media, although project folders
/// are searched before the bundled one.
pub(crate) fn discover_in_with(
    paths: &[SearchPath],
    turned_off: impl Fn(&PluginEntry) -> bool,
) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
    // Every plugin read and every error, in search order; which plugin
    // takes an id is settled once the bundled ones are known.
    let mut read: Vec<PluginRead> = Vec::new();
    let add = |entry: Result<Option<(PluginEntry, Vec<DiscoveryError>)>, DiscoveryError>,
               read: &mut Vec<PluginRead>| match entry {
        Ok(Some(entry)) => read.push(Ok(entry)),
        Ok(None) => {}
        Err(error) => read.push(Err(error)),
    };
    for SearchPath { path, source } in paths {
        if !path.is_absolute() {
            add(
                Err(DiscoveryError::RelativePath { path: path.clone() }),
                &mut read,
            );
            continue;
        }
        if !path.exists() {
            if source.reports_missing() {
                add(
                    Err(DiscoveryError::Io {
                        path: path.clone(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "the folder does not exist",
                        ),
                    }),
                    &mut read,
                );
            }
            continue;
        }
        if source.is_checked()
            && let Some(reason) = unsafe_reason(path)
        {
            add(
                Err(DiscoveryError::Invalid {
                    path: path.clone(),
                    reason,
                }),
                &mut read,
            );
            continue;
        }
        if path.join(MANIFEST).is_file() {
            add(read_manifest(path, *source), &mut read);
            continue;
        }
        let children = match std::fs::read_dir(path) {
            Ok(children) => children,
            Err(source) => {
                add(
                    Err(DiscoveryError::Io {
                        path: path.clone(),
                        source,
                    }),
                    &mut read,
                );
                continue;
            }
        };
        let mut children: Vec<PathBuf> = children
            .filter_map(|child| child.ok().map(|child| child.path()))
            .filter(|child| child.is_dir())
            .collect();
        children.sort();
        for child in children {
            add(read_manifest(&child, *source), &mut read);
        }
    }
    let bundled_media: HashSet<String> = read
        .iter()
        .filter_map(|read| read.as_ref().ok())
        .filter(|(entry, _)| {
            entry.source == PluginSource::Bundled && entry.has_role(PluginRole::Media)
        })
        .map(|(entry, _)| entry.manifest.id.clone())
        .collect();
    let mut found: Vec<PluginEntry> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    let mut errors = Vec::new();
    for read in read {
        match read {
            Ok((entry, _))
                if entry.source == PluginSource::Project
                    && bundled_media.contains(&entry.manifest.id) =>
            {
                errors.push(DiscoveryError::Invalid {
                    path: entry.directory.join(MANIFEST),
                    reason: format!(
                        "a project plugin cannot replace Celesta's bundled media plugin {}",
                        entry.manifest.id
                    ),
                });
            }
            Ok((entry, skipped)) if !taken.contains(&entry.manifest.id) => {
                if !turned_off(&entry) {
                    taken.insert(entry.manifest.id.clone());
                }
                found.push(entry);
                errors.extend(skipped);
            }
            Ok(_) => {}
            Err(error) => errors.push(error),
        }
    }
    (found, errors)
}

/// A plugin discovery read, with the problems of its components it left
/// out, or why it skipped one.
type PluginRead = Result<(PluginEntry, Vec<DiscoveryError>), DiscoveryError>;

/// The plugins found in every location Celesta searches ([`search_paths`]
/// with `explicit` first), as one set every part of Celesta shares: the
/// renderer's [`crate::PluginHost`] and the React runtime's
/// `CELESTA_PLUGIN_PATH` ([`InstalledPlugins::runtime`]) are made from the
/// same discovery, so they agree on what is installed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstalledPlugins {
    /// What may run.
    pub plugins: Vec<PluginEntry>,
    /// What waits for the user's approval ([`InstalledPlugins::allowed_by`]):
    /// never run. The React runtime treats their imports as
    /// `waiting_imports` says.
    pub waiting: Vec<PluginEntry>,
    /// What an entry's import of a waiting plugin resolves to: a stand-in
    /// in the app ([`InstalledPlugins::allowed_by`]), a failure for
    /// command-line tools ([`InstalledPlugins::find`]) and sets made by hand.
    pub waiting_imports: WaitingImports,
}

/// What a React runtime's process is told about plugins, as environment
/// variables: give it explicitly (as `Command::env` does) rather than
/// through Celesta's own environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimePlugins {
    /// [`PLUGIN_PATH_ENV`]: the plugins it may load.
    pub path: std::ffi::OsString,
    /// [`WAITING_PLUGIN_PATH_ENV`]: the plugins that wait for approval.
    pub waiting: std::ffi::OsString,
    /// [`WAITING_IMPORTS_ENV`]: what their imports resolve to.
    pub waiting_imports: WaitingImports,
}

impl RuntimePlugins {
    /// The variables to set, by name.
    pub fn env(&self) -> [(&'static str, &std::ffi::OsStr); 3] {
        [
            (PLUGIN_PATH_ENV, &self.path),
            (WAITING_PLUGIN_PATH_ENV, &self.waiting),
            (
                WAITING_IMPORTS_ENV,
                std::ffi::OsStr::new(self.waiting_imports.as_str()),
            ),
        ]
    }
}

/// `entries`' directories as a path list, in order. Discovery skips
/// directories that would break the list (see `read_manifest`); a set made
/// by hand leaves them out here, so joining what remains cannot fail.
fn path_list(entries: &[PluginEntry]) -> std::ffi::OsString {
    std::env::join_paths(
        entries
            .iter()
            .map(|entry| &entry.directory)
            .filter(|directory| !breaks_path_list(directory)),
    )
    .expect("paths without the separator or a quote join")
}

impl InstalledPlugins {
    /// Discovers them, with the plugins that were skipped and why: what
    /// command-line tools and the MCP server run. The folders the user added
    /// in their settings are searched too, and the user's settings decide as
    /// in the app ([`InstalledPlugins::allowed_by`]): a plugin turned off is
    /// left out, giving way to the next plugin with its id, and one that
    /// needs the user's approval runs only when the app allowed it as it is
    /// now, or its id is in `allow` (the exporter's `--allow-plugin`).
    /// Nothing is asked: a plugin that waits is skipped with
    /// [`DiscoveryError::WaitsForApproval`], and an entry importing it fails
    /// ([`WaitingImports::Fail`]).
    pub fn find(explicit: &[PathBuf], allow: &[String]) -> (Self, Vec<DiscoveryError>) {
        let (settings, settings_error) = UserSettings::current();
        let (found, mut errors) = Self::find_allowing(explicit, allow, &settings);
        if let Some(error) = settings_error {
            errors.insert(0, DiscoveryError::Settings(error.to_string()));
        }
        (found, errors)
    }

    /// [`InstalledPlugins::find`] with these settings.
    pub fn find_allowing(
        explicit: &[PathBuf],
        allow: &[String],
        settings: &UserSettings,
    ) -> (Self, Vec<DiscoveryError>) {
        let (found, mut errors) = Self::find_with(explicit, &[], settings);
        let allowed = found.allowed_allowing(settings, allow);
        errors.extend(
            allowed
                .unreadable
                .into_iter()
                .map(DiscoveryError::Unreadable),
        );
        errors.extend(
            allowed
                .pending
                .iter()
                .map(|pending| DiscoveryError::WaitsForApproval {
                    path: pending.entry.directory.clone(),
                    id: pending.entry.manifest.id.clone(),
                    changed: pending.changed,
                }),
        );
        let named = |id: &String| {
            allowed
                .plugins
                .plugins
                .iter()
                .chain(&allowed.disabled)
                .any(|entry| &entry.manifest.id == id)
        };
        let mut unknown: Vec<&String> = allow.iter().filter(|id| !named(id)).collect();
        unknown.sort();
        unknown.dedup();
        errors.extend(
            unknown
                .into_iter()
                .map(|id| DiscoveryError::UnknownAllowance { id: id.clone() }),
        );
        let mut plugins = allowed.plugins;
        plugins.waiting_imports = WaitingImports::Fail;
        (plugins, errors)
    }

    /// Discovers them with a trusted project's own plugin folders (see
    /// [`crate::project_plugin_paths`]) and the folders `settings` adds,
    /// leaving nothing out: a plugin `settings` turns off is listed, and so
    /// is the next one with its id, which is used in its place.
    pub fn find_with(
        explicit: &[PathBuf],
        project: &[PathBuf],
        settings: &UserSettings,
    ) -> (Self, Vec<DiscoveryError>) {
        let paths = search_paths(explicit, project, &settings.plugin_folders);
        let (plugins, errors) = discover_in_with(&paths, |entry| {
            settings.is_disabled(&entry.manifest.id, &entry.directory)
        });
        (
            Self {
                plugins,
                ..Self::default()
            },
            errors,
        )
    }

    /// Discovers them, reporting skipped plugins on stderr, for tools
    /// without a better place to show them.
    pub fn find_reporting(explicit: &[PathBuf], allow: &[String]) -> Self {
        let (plugins, errors) = Self::find(explicit, allow);
        for error in errors {
            match error {
                DiscoveryError::UnknownAllowance { .. } => eprintln!("celesta: {error}"),
                _ => eprintln!("celesta: skipped a plugin: {error}"),
            }
        }
        plugins
    }

    /// The value of `CELESTA_PLUGIN_PATH` for the React runtime: each plugin
    /// directory in search order, so the runtime resolves
    /// `@celesta-plugin/<id>` to the same plugins, without searching any
    /// folder itself.
    pub fn runtime_path(&self) -> std::ffi::OsString {
        path_list(&self.plugins)
    }

    /// Everything the React runtime is told: [`Self::runtime_path`], the
    /// waiting plugins, and what their imports resolve to.
    pub fn runtime(&self) -> RuntimePlugins {
        RuntimePlugins {
            path: self.runtime_path(),
            waiting: path_list(&self.waiting),
            waiting_imports: self.waiting_imports,
        }
    }

    /// A host that runs these plugins' content for a renderer.
    pub fn host(&self) -> crate::PluginHost {
        crate::PluginHost::new(
            self.plugins
                .iter()
                .filter(|entry| entry.has_role(PluginRole::Content))
                .cloned(),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use celesta_plugin::PROTOCOL_VERSION;

    /// A `celesta` range this Celesta is in.
    pub(crate) const COMPATIBLE: &str = concat!("=", env!("CARGO_PKG_VERSION"));

    fn write_plugin(directory: &Path, id: &str, version: &str) {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(
            directory.join(MANIFEST),
            serde_json::json!({
                "id": id,
                "version": version,
                "protocol": 1, "celesta": COMPATIBLE,
                "executable": { "windows": "bin/p.exe", "linux": "bin/p", "macos": "bin/p", "plan9-mips": "bin/other" },
                "module": "js/index.js",
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn prefers_the_executable_for_this_architecture() {
        let (os, arch) = platform();
        let entry = |keys: &[(String, &str)]| PluginEntry {
            manifest: Manifest {
                id: "p".into(),
                version: "1".into(),
                protocol: 1,
                celesta: Some(COMPATIBLE.into()),
                executable: keys
                    .iter()
                    .map(|(key, value)| (key.clone(), (*value).to_owned()))
                    .collect(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: content_roles(),
                media: None,
            },
            directory: PathBuf::from("plugin"),
            source: PluginSource::Explicit,
        };
        let any = entry(&[(os.to_owned(), "any")]);
        assert_eq!(any.executable(), Some(PathBuf::from("plugin/any")));
        let both = entry(&[(os.to_owned(), "any"), (format!("{os}-{arch}"), "this")]);
        assert_eq!(both.executable(), Some(PathBuf::from("plugin/this")));
        let other = entry(&[(format!("{os}-not-{arch}"), "other")]);
        assert_eq!(other.executable(), None);
    }

    #[test]
    fn finds_plugin_directories_and_directories_of_them() {
        let root = tempfile::tempdir().unwrap();
        let single = root.path().join("single");
        write_plugin(&single, "one", "1.0.0");
        let collection = root.path().join("collection");
        write_plugin(&collection.join("b"), "two", "1.0.0");
        write_plugin(&collection.join("a"), "one", "2.0.0");
        std::fs::create_dir_all(collection.join("c")).unwrap();
        std::fs::write(collection.join("c").join(MANIFEST), "{").unwrap();

        let (found, errors) = discover(&[
            single.clone(),
            collection.clone(),
            root.path().join("missing"),
        ]);
        // The first "one" wins over the collection's.
        let ids: Vec<_> = found
            .iter()
            .map(|entry| (entry.manifest.id.as_str(), entry.manifest.version.as_str()))
            .collect();
        assert_eq!(ids, [("one", "1.0.0"), ("two", "1.0.0")]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(matches!(errors[0], DiscoveryError::Manifest { .. }));

        let entry = &found[0];
        let executable = entry.executable().unwrap();
        assert!(
            executable.starts_with(&single)
                && executable.ends_with(if cfg!(windows) { "bin/p.exe" } else { "bin/p" })
        );
        assert_eq!(entry.module(), Some(single.join("js/index.js")));
        assert_eq!(entry.types(), None);
    }

    #[test]
    fn ids_are_one_path_component() {
        for id in ["flag", "beep-voice", "live2d", "a.b_c", "0"] {
            assert!(is_valid_id(id), "{id}");
        }
        for id in ["", "..", ".hidden", "-x", "Flag", "a/b", "a\\b", "c:", "ü"] {
            assert!(!is_valid_id(id), "{id}");
        }
    }

    #[test]
    fn manifest_paths_stay_relative_and_inside() {
        for path in ["bin/p.exe", "./js/index.js", "a..b/c", "js\\index.js"] {
            assert!(is_contained_path(path), "{path}");
        }
        for path in [
            "",
            "../p",
            "bin/../../p",
            "bin\\..\\p",
            "/usr/bin/p",
            "\\\\server\\share\\p",
            "C:\\p.exe",
            "C:p.exe",
            "p.exe:stream",
        ] {
            assert!(!is_contained_path(path), "{path}");
        }
    }

    #[test]
    fn manifest_paths_use_this_platforms_separator() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("p");
        std::fs::create_dir_all(directory.join("js")).unwrap();
        std::fs::write(directory.join("js/index.js"), "export {};").unwrap();
        std::fs::write(directory.join("js/index.d.ts"), "export {};").unwrap();
        let (os, _) = platform();
        write_manifest(
            &directory,
            serde_json::json!({
                "id": "p", "version": "1", "protocol": 1, "celesta": COMPATIBLE,
                "module": "js\\index.js", "types": "./js//index.d.ts",
                "executable": { os: "bin/windows-x86_64/p.exe" },
            }),
        );
        let (found, errors) = discover(std::slice::from_ref(&directory));
        assert!(errors.is_empty(), "{errors:?}");
        let entry = &found[0];
        // Written with `\` or with `/`, the file is the one on disk.
        let module = entry.module().unwrap();
        assert_eq!(module, directory.join("js").join("index.js"));
        assert!(module.is_file(), "{}", module.display());
        assert_eq!(entry.types(), Some(directory.join("js").join("index.d.ts")));
        let executable = entry.executable().unwrap();
        assert_eq!(
            executable,
            directory.join("bin").join("windows-x86_64").join("p.exe")
        );
        if cfg!(windows) {
            let text = executable.to_string_lossy();
            assert!(!text.contains('/'), "{text}");
        } else {
            assert!(
                !module.to_string_lossy().contains('\\'),
                "{}",
                module.display()
            );
        }
    }

    pub(crate) fn write_manifest(directory: &Path, manifest: serde_json::Value) {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join(MANIFEST), manifest.to_string()).unwrap();
    }

    #[test]
    fn components_are_offered_only_as_written_safely() {
        let root = tempfile::tempdir().unwrap();
        let write = |name: &str, components: serde_json::Value| {
            write_manifest(
                &root.path().join(name),
                serde_json::json!({
                    "id": name, "version": "1", "protocol": 1, "celesta": COMPATIBLE,
                    "module": "js/index.js", "components": components,
                }),
            );
        };
        write(
            "good",
            serde_json::json!([
                { "name": "Flag", "label": "Waving flag", "src": "image", "props": { "amplitude": 0.1, "loop": true, "tone": "warm" } },
                { "name": "Box_2" },
            ]),
        );
        write("lowercase", serde_json::json!([{ "name": "flag" }]));
        write("markup", serde_json::json!([{ "name": "Flag x={1}" }]));
        write(
            "geometry",
            serde_json::json!([{ "name": "Flag", "src": "video", "props": { "width": 3 } }]),
        );
        write(
            "attribute",
            serde_json::json!([{ "name": "Flag", "props": { "on:click": 1 } }]),
        );
        write(
            "object",
            serde_json::json!([{ "name": "Flag", "props": { "style": { "a": 1 } } }]),
        );
        write_manifest(
            &root.path().join("moduleless"),
            serde_json::json!({ "id": "moduleless", "version": "1", "protocol": 1, "celesta": COMPATIBLE, "components": [{ "name": "Flag" }] }),
        );

        let (found, errors) = discover(&[root.path().to_path_buf()]);
        assert_eq!(found.len(), 1, "{errors:?}");
        assert_eq!(errors.len(), 6, "{errors:?}");
        let components = &found[0].manifest.components;
        assert_eq!(components[0].label(), "Waving flag");
        assert_eq!(components[0].src, Some(ComponentSource::Image));
        assert_eq!(components[0].drawn_props(), ["src", "x", "y", "width"]);
        assert_eq!(components[1].label(), "Box_2");
        assert_eq!(components[1].drawn_props(), ["x", "y", "width", "height"]);
    }

    #[test]
    fn model_components_need_extensions_and_an_aspect() {
        let root = tempfile::tempdir().unwrap();
        let write = |id: &str, component: serde_json::Value| {
            write_manifest(
                &root.path().join(id),
                serde_json::json!({
                    "id": id, "version": "1", "protocol": 1, "celesta": COMPATIBLE,
                    "module": "js/index.js", "components": [component],
                }),
            );
        };
        write(
            "good",
            serde_json::json!({ "name": "Model", "label": "3D model", "src": "model", "extensions": ["glb"], "aspect": 1.7777778 }),
        );
        write(
            "bare",
            serde_json::json!({ "name": "Model", "src": "model" }),
        );
        write(
            "shapeless",
            serde_json::json!({ "name": "Model", "src": "model", "extensions": ["glb"] }),
        );
        write(
            "upper",
            serde_json::json!({ "name": "Model", "src": "model", "extensions": ["GLB"] }),
        );
        write(
            "image",
            serde_json::json!({ "name": "Model", "src": "image", "extensions": ["glb"] }),
        );
        write(
            "flat",
            serde_json::json!({ "name": "Model", "src": "model", "extensions": ["glb"], "aspect": 0 }),
        );
        let with_aspect = |aspect: f64| serde_json::json!({ "name": "Model", "src": "model", "extensions": ["glb"], "aspect": aspect });
        write("negative", with_aspect(-1.0));
        write("thin", with_aspect(1.0e-5));
        write("long", with_aspect(16385.0));
        write("widest", with_aspect(MAX_COMPONENT_ASPECT));
        write(
            "unmodelled",
            serde_json::json!({ "name": "Model", "src": "image", "aspect": 1 }),
        );
        let with_extensions = |extensions: serde_json::Value| serde_json::json!({ "name": "Model", "src": "model", "extensions": extensions });
        write("empty", with_extensions(serde_json::json!([""])));
        write("dotted", with_extensions(serde_json::json!([".glb"])));
        write(
            "twice",
            with_extensions(serde_json::json!(["glb", "gltf", "glb"])),
        );

        let (found, errors) = discover(&[root.path().to_path_buf()]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["good", "widest"], "{errors:?}");
        assert_eq!(
            found[1].manifest.components[0].aspect,
            Some(MAX_COMPONENT_ASPECT)
        );
        let component = &found[0].manifest.components[0];
        assert_eq!(component.src, Some(ComponentSource::Model));
        assert_eq!(component.label(), "3D model");
        assert_eq!(component.extensions, ["glb"]);
        assert_eq!(component.aspect, Some(1.7777778));
        assert_eq!(component.drawn_props(), ["src", "x", "y", "width"]);

        let reason = |id: &str| {
            errors
                .iter()
                .find_map(|error| match error {
                    DiscoveryError::Invalid { path, reason }
                        if path.starts_with(root.path().join(id)) =>
                    {
                        Some(reason.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{id} was not skipped: {errors:?}"))
        };
        assert_eq!(errors.len(), 12, "{errors:?}");
        assert_eq!(
            reason("bare"),
            "component \"Model\" takes a model file but lists no extensions"
        );
        assert_eq!(
            reason("shapeless"),
            "component \"Model\" takes a model file but gives no aspect"
        );
        assert_eq!(
            reason("upper"),
            "component \"Model\"'s extension \".GLB\" is not lowercase letters and digits"
        );
        assert_eq!(
            reason("image"),
            "component \"Model\" lists extensions but does not take a model file"
        );
        for (id, aspect) in [
            ("flat", "0"),
            ("negative", "-1"),
            ("thin", "0.00001"),
            ("long", "16385"),
        ] {
            assert_eq!(
                reason(id),
                format!("component \"Model\"'s aspect {aspect} is not from 1/16384 to 16384")
            );
        }
        assert_eq!(
            reason("unmodelled"),
            "component \"Model\" lists an aspect but does not take a model file"
        );
        assert_eq!(
            reason("empty"),
            "component \"Model\"'s extension \".\" is not lowercase letters and digits"
        );
        assert_eq!(
            reason("dotted"),
            "component \"Model\"'s extension \".glb\" is written with its dot; list it as \"glb\""
        );
        assert_eq!(
            reason("twice"),
            "component \"Model\" lists the extension \".glb\" twice"
        );
    }

    #[test]
    fn the_repositorys_plugins_pass_the_manifest_checks() {
        // Each `plugin.json` under `plugins/` and `examples/plugins/`, made
        // into the manifest its build script writes (`id`, `version`, the
        // protocol of its roles, and, when plugin.json has them, `roles`,
        // the module it builds, `components`, and `media`): a component the
        // checks refuse would skip the whole plugin for every user, so it
        // fails here instead.
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = tempfile::tempdir().unwrap();
        let mut expected = Vec::new();
        for folder in ["plugins", "examples/plugins"] {
            let mut sources: Vec<_> = std::fs::read_dir(repository.join(folder))
                .unwrap()
                .map(|entry| entry.unwrap().path().join("plugin.json"))
                .filter(|source| source.is_file())
                .collect();
            sources.sort();
            assert!(!sources.is_empty(), "no plugin.json under {folder}");
            for source in sources {
                let plugin: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&source).unwrap())
                        .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
                let id = plugin["id"].as_str().unwrap().to_owned();
                let roles: Vec<&str> = plugin
                    .get("roles")
                    .and_then(serde_json::Value::as_array)
                    .map(|roles| roles.iter().filter_map(serde_json::Value::as_str).collect())
                    .unwrap_or_default();
                let mut manifest = serde_json::json!({
                    "id": id, "version": plugin["version"], "protocol": protocol_for_roles(&roles),
                    "celesta": COMPATIBLE,
                });
                for key in ["roles", "media"] {
                    if let Some(value) = plugin.get(key) {
                        manifest[key] = value.clone();
                    }
                }
                if plugin.get("module").is_some() {
                    manifest["module"] = "js/index.js".into();
                    manifest["types"] = "js/index.d.ts".into();
                }
                if let Some(components) = plugin.get("components") {
                    manifest["components"] = components.clone();
                }
                let components: Vec<ComponentInfo> = serde_json::from_value(
                    plugin
                        .get("components")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([])),
                )
                .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
                write_manifest(&root.path().join(&id), manifest);
                expected.push((id, source, components));
            }
        }

        let (found, errors) = discover(&[root.path().to_path_buf()]);
        assert!(errors.is_empty(), "{errors:?}");
        let ffmpeg = found
            .iter()
            .find(|entry| entry.manifest.id == "ffmpeg")
            .expect("no plugin ffmpeg under plugins/");
        assert!(
            !ffmpeg.has_role(PluginRole::Content),
            "{:?}",
            ffmpeg.manifest.roles
        );
        assert_eq!(ffmpeg.module(), None);
        let mut found: Vec<_> = found
            .iter()
            .map(|entry| (entry.manifest.id.as_str(), &entry.manifest.components))
            .collect();
        found.sort_by_key(|(id, _)| *id);
        expected.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(found.len(), expected.len(), "{found:?}");
        for ((id, components), (expected_id, source, expected_components)) in
            found.into_iter().zip(&expected)
        {
            assert_eq!(id, expected_id, "{}", source.display());
            assert_eq!(components, expected_components, "{}", source.display());
        }
    }

    #[test]
    fn skips_manifests_that_reach_outside_their_directory() {
        let root = tempfile::tempdir().unwrap();
        let collection = root.path().join("collection");
        let manifest = |id: &str, field: &str, path: &str| {
            let mut manifest = serde_json::json!({ "id": id, "version": "1", "protocol": 1, "celesta": COMPATIBLE });
            if field == "executable" {
                manifest["executable"] = serde_json::json!({ platform().0: path });
            } else {
                manifest[field] = path.into();
            }
            manifest
        };
        write_manifest(
            &collection.join("a"),
            manifest("../escape", "module", "js/index.js"),
        );
        write_manifest(
            &collection.join("b"),
            manifest("b", "executable", "../../p"),
        );
        write_manifest(
            &collection.join("c"),
            manifest("c", "module", "/etc/index.js"),
        );
        write_manifest(
            &collection.join("d"),
            manifest("d", "types", "C:\\index.d.ts"),
        );
        write_manifest(
            &collection.join("e"),
            manifest("e", "module", "js/index.js"),
        );

        let (found, errors) = discover(&[PathBuf::from("relative"), collection]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["e"]);
        assert!(
            matches!(errors[0], DiscoveryError::RelativePath { .. }),
            "{errors:?}"
        );
        assert_eq!(errors.len(), 5, "{errors:?}");
        assert!(
            errors[1..]
                .iter()
                .all(|error| matches!(error, DiscoveryError::Invalid { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn skips_manifests_whose_files_link_outside() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside.js");
        std::fs::write(&outside, "").unwrap();
        let plugin = root.path().join("plugin");
        write_manifest(
            &plugin,
            serde_json::json!({ "id": "p", "version": "1", "protocol": 1, "celesta": COMPATIBLE, "module": "index.js" }),
        );
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, plugin.join("index.js"));
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside, plugin.join("index.js"));
        if linked.is_err() {
            // Windows creates symbolic links only in developer mode or as an
            // administrator.
            eprintln!("skipped: could not create a symbolic link");
            return;
        }

        let (found, errors) = discover(std::slice::from_ref(&plugin));
        assert!(found.is_empty());
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("leads outside")),
            "{errors:?}"
        );
    }

    #[test]
    fn only_a_folder_the_user_added_is_missed_aloud() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("gone");
        let search = |source| {
            discover_in(&[SearchPath {
                path: missing.clone(),
                source,
            }])
            .1
        };
        for quiet in [
            PluginSource::Explicit,
            PluginSource::User,
            PluginSource::Bundled,
        ] {
            assert!(search(quiet).is_empty(), "{quiet:?}");
        }
        assert!(matches!(
            &search(PluginSource::Folder)[..],
            [DiscoveryError::Io { .. }]
        ));
    }

    #[test]
    fn the_runtime_gets_each_plugin_directory_in_order() {
        let root = tempfile::tempdir().unwrap();
        write_plugin(&root.path().join("b"), "two", "1");
        write_plugin(&root.path().join("a"), "one", "1");
        let (found, errors) = discover_in(&[SearchPath {
            path: root.path().to_path_buf(),
            source: PluginSource::User,
        }]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(found.iter().all(|entry| entry.source == PluginSource::User));
        let installed = InstalledPlugins {
            plugins: found.clone(),
            waiting: found[1..].to_vec(),
            waiting_imports: WaitingImports::StandIn,
        };
        let path: Vec<_> = std::env::split_paths(&installed.runtime_path()).collect();
        assert_eq!(path, [root.path().join("a"), root.path().join("b")]);
        let runtime = installed.runtime();
        assert_eq!(runtime.path, installed.runtime_path());
        assert_eq!(runtime.waiting, root.path().join("b").into_os_string());
        assert_eq!(
            runtime.env().map(|(name, value)| (name, value.to_owned())),
            [
                (PLUGIN_PATH_ENV, installed.runtime_path()),
                (
                    WAITING_PLUGIN_PATH_ENV,
                    root.path().join("b").into_os_string()
                ),
                (WAITING_IMPORTS_ENV, "stand-in".into()),
            ]
        );
        // A set made by hand fails a waiting plugin's import.
        assert_eq!(
            InstalledPlugins::default().runtime().env()[2],
            (WAITING_IMPORTS_ENV, std::ffi::OsStr::new("fail"))
        );
        assert_eq!(
            InstalledPlugins::default().runtime(),
            RuntimePlugins::default()
        );
    }

    #[cfg(unix)]
    #[test]
    fn checked_sources_skip_what_other_users_can_change() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let open = root.path().join("open");
        write_plugin(&open, "open", "1");
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let shut = root.path().join("shut");
        write_plugin(&shut, "shut", "1");
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(shut.join(MANIFEST), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let path = |source| SearchPath {
            path: root.path().to_path_buf(),
            source,
        };
        let ids = |found: &[PluginEntry]| -> Vec<String> {
            found
                .iter()
                .map(|entry| entry.manifest.id.clone())
                .collect()
        };

        let (found, errors) = discover_in(&[path(PluginSource::User)]);
        assert_eq!(ids(&found), ["shut"]);
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("other users")),
            "{errors:?}"
        );
        // Folders named explicitly are the caller's choice.
        let (found, errors) = discover_in(&[path(PluginSource::Explicit)]);
        assert_eq!(ids(&found), ["open", "shut"]);
        assert!(errors.is_empty(), "{errors:?}");

        // A whole checked folder others can change is skipped at once.
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let (found, errors) = discover_in(&[path(PluginSource::Bundled)]);
        assert!(found.is_empty());
        assert_eq!(errors.len(), 1, "{errors:?}");
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_the_manifest_does_not_name_is_judged_too() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let plugin = root.path().join("p");
        write_plugin(&plugin, "p", "1");
        let mode = |path: &Path, mode| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap()
        };
        for path in [root.path(), plugin.as_path()] {
            mode(path, 0o755);
        }
        mode(&plugin.join(MANIFEST), 0o644);
        // A library beside the program, which the manifest does not name.
        std::fs::create_dir_all(plugin.join("bin")).unwrap();
        mode(&plugin.join("bin"), 0o755);
        std::fs::write(plugin.join("bin/libhelper.so"), "").unwrap();
        mode(&plugin.join("bin/libhelper.so"), 0o644);
        let search = [SearchPath {
            path: root.path().to_path_buf(),
            source: PluginSource::User,
        }];
        assert_eq!(discover_in(&search).0.len(), 1);

        mode(&plugin.join("bin/libhelper.so"), 0o666);
        let (found, errors) = discover_in(&search);
        assert!(found.is_empty());
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("libhelper")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_folder_that_would_break_the_runtime_path_is_skipped() {
        let root = tempfile::tempdir().unwrap();
        let separator = if cfg!(windows) { ';' } else { ':' };
        write_plugin(&root.path().join(format!("a{separator}b")), "split", "1");
        write_plugin(&root.path().join("plain"), "plain", "1");
        let (found, errors) = discover(&[root.path().to_path_buf()]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["plain"]);
        assert!(
            matches!(&errors[..], [DiscoveryError::Invalid { reason, .. }] if reason.contains("separator")),
            "{errors:?}"
        );

        // A set made by hand with such a folder leaves it out of the path.
        let mut by_hand = InstalledPlugins {
            plugins: found,
            ..InstalledPlugins::default()
        };
        let mut split = by_hand.plugins[0].clone();
        split.directory = root.path().join(format!("a{separator}b"));
        by_hand.plugins.push(split);
        let path: Vec<_> = std::env::split_paths(&by_hand.runtime_path()).collect();
        assert_eq!(path, [root.path().join("plain")]);
    }

    #[test]
    fn makes_extra_paths_absolute() {
        let paths = plugin_paths(&[PathBuf::from("plugins")]);
        assert_eq!(paths[0], std::env::current_dir().unwrap().join("plugins"));
    }

    #[test]
    fn reads_roles_and_lists_the_media_plugins() {
        let root = tempfile::tempdir().unwrap();
        let write = |name: &str, roles: serde_json::Value| {
            let directory = root.path().join(name);
            std::fs::create_dir_all(&directory).unwrap();
            let mut manifest = serde_json::json!({
                "id": name,
                "version": "1.0.0",
                "protocol": if roles == serde_json::json!(["media"]) { 2 } else { 1 },
                "celesta": COMPATIBLE,
                "executable": { std::env::consts::OS: "bin/p" },
            });
            if !roles.is_null() {
                manifest["roles"] = roles;
            }
            std::fs::write(directory.join(MANIFEST), manifest.to_string()).unwrap();
            directory
        };
        write("a-content", serde_json::Value::Null);
        let media = write("b-media", serde_json::json!(["media"]));
        write("c-media", serde_json::json!(["media"]));
        write("d-future", serde_json::json!(["content", "sound"]));
        write("e-codec", serde_json::json!(["codec"]));

        let (plugins, errors) = discover(&[root.path().to_path_buf()]);
        assert!(errors.is_empty(), "{errors:?}");
        let roles: Vec<_> = plugins
            .iter()
            .map(|entry| entry.manifest.roles.clone())
            .collect();
        assert_eq!(
            roles,
            [
                vec![PluginRole::Content],
                vec![PluginRole::Media],
                vec![PluginRole::Media],
                vec![PluginRole::Content, PluginRole::Unknown],
                vec![PluginRole::Unknown],
            ]
        );
        let installed = InstalledPlugins {
            plugins,
            ..InstalledPlugins::default()
        };
        let listed = installed.media_with_sibling(None);
        let ids: Vec<_> = listed.iter().map(|plugin| plugin.id.as_str()).collect();
        assert_eq!(ids, ["b-media", "c-media"]);
        assert_eq!(listed[0].executable, media.join("bin").join("p"));
        assert_eq!(listed[0].directory, media);
        let content: Vec<_> = installed
            .host()
            .plugins()
            .map(|entry| entry.manifest.id.clone())
            .collect();
        assert_eq!(content, ["a-content", "d-future"]);
    }

    #[test]
    fn celesta_ranges_reach_the_next_breaking_release() {
        let range = |version: &str| celesta_range_for(&semver::Version::parse(version).unwrap());
        assert_eq!(range("0.9.0"), ">=0.9.0, <0.10.0");
        assert_eq!(range("0.9.3"), ">=0.9.3, <0.10.0");
        assert_eq!(range("1.2.3"), ">=1.2.3, <2.0.0");
        assert_eq!(range("0.10.0-beta.1"), ">=0.10.0-beta.1, <0.11.0");
        assert_eq!(range("0.9.0+build.5"), ">=0.9.0, <0.10.0");
        assert_eq!(range("0.0.3"), ">=0.0.3, <0.0.4");
        // This Celesta is in the range it suggests.
        let manifest = |celesta: Option<&str>| Manifest {
            id: "p".into(),
            version: "1".into(),
            protocol: 1,
            celesta: celesta.map(Into::into),
            executable: BTreeMap::new(),
            module: None,
            types: None,
            components: Vec::new(),
            roles: content_roles(),
            media: None,
        };
        let suggested = celesta_range_for(&celesta_version());
        assert!(manifest(Some(&suggested)).works_with(&celesta_version()));
        assert!(!manifest(None).works_with(&celesta_version()));
    }

    #[test]
    fn celesta_ranges_match_as_cargo_does() {
        let works_with = |celesta: &str, version: &str| {
            Manifest {
                id: "p".into(),
                version: "1".into(),
                protocol: 1,
                celesta: Some(celesta.into()),
                executable: BTreeMap::new(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: content_roles(),
                media: None,
            }
            .works_with(&semver::Version::parse(version).unwrap())
        };
        let range = ">=0.9.0, <0.10.0";
        assert!(works_with(range, "0.9.0"));
        assert!(works_with(range, "0.9.7"));
        assert!(!works_with(range, "0.8.9"));
        assert!(!works_with(range, "0.10.0"));
        // A pre-release only matches a range naming one of its version.
        assert!(!works_with(range, "0.9.8-beta.1"));
        let beta = ">=0.10.0-beta.1, <0.11.0";
        assert!(works_with(beta, "0.10.0-beta.2"));
        assert!(works_with(beta, "0.10.0"));
        assert!(!works_with(beta, "0.10.1-beta.1"));
        // Cargo's other forms read too.
        assert!(works_with("^0.9", "0.9.4"));
        assert!(works_with("*", "3.0.0"));
        assert!(!works_with("soon", "0.9.0"));
    }

    #[test]
    fn skips_plugins_of_another_protocol() {
        let root = tempfile::tempdir().unwrap();
        let write = |id: &str, protocol: u32, celesta: &str| {
            write_manifest(
                &root.path().join(id),
                serde_json::json!({
                    "id": id, "version": "1", "protocol": protocol, "celesta": celesta,
                    "module": "js/index.js",
                }),
            );
        };
        write("a-next", PROTOCOL_VERSION + 1, COMPATIBLE);
        write("b-future", PROTOCOL_VERSION + 1, ">=999.0.0");
        write("c-fits", PROTOCOL_VERSION, COMPATIBLE);

        let (found, errors) = discover(&[root.path().to_path_buf()]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["c-fits"]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        let message = errors[0].to_string();
        assert!(
            matches!(errors[0], DiscoveryError::Invalid { .. })
                && message.ends_with(&format!(
                    "it speaks plugin protocol {}, and this Celesta speaks protocol {PROTOCOL_VERSION} with content plugins",
                    PROTOCOL_VERSION + 1
                )),
            "{message}"
        );
        // A plugin for another Celesta is reported as that, which says
        // what to do about it.
        assert!(
            matches!(errors[1], DiscoveryError::Incompatible { .. }),
            "{:?}",
            errors[1]
        );
    }

    #[test]
    fn skips_plugins_without_or_outside_a_celesta_range() {
        let root = tempfile::tempdir().unwrap();
        let write = |id: &str, celesta: Option<&str>| {
            let mut manifest = serde_json::json!({ "id": id, "version": "1", "protocol": 1, "module": "js/index.js" });
            if let Some(celesta) = celesta {
                manifest["celesta"] = celesta.into();
            }
            write_manifest(&root.path().join(id), manifest);
        };
        write("a-missing", None);
        write("b-unreadable", Some("soon"));
        write("c-future", Some(">=999.0.0"));
        write("d-fits", Some(COMPATIBLE));

        let (found, errors) = discover(&[root.path().to_path_buf()]);
        let ids: Vec<_> = found
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["d-fits"]);
        assert_eq!(errors.len(), 3, "{errors:?}");
        let missing = errors[0].to_string();
        assert!(
            matches!(errors[0], DiscoveryError::Invalid { .. })
                && missing.contains("\"celesta\" range")
                && missing.contains(&celesta_range_for(&celesta_version())),
            "{missing}"
        );
        assert!(
            matches!(errors[1], DiscoveryError::Invalid { .. })
                && errors[1]
                    .to_string()
                    .contains("\"soon\" is not a version range"),
            "{}",
            errors[1]
        );
        let DiscoveryError::Incompatible {
            id, range, celesta, ..
        } = &errors[2]
        else {
            panic!("{:?}", errors[2]);
        };
        assert_eq!(
            (id.as_str(), range.as_str(), celesta.as_str()),
            ("c-future", ">=999.0.0", CELESTA_VERSION)
        );
        assert!(
            errors[2]
                .to_string()
                .ends_with(&format!("the plugin c-future works with Celesta >=999.0.0, and this is Celesta {CELESTA_VERSION}")),
            "{}",
            errors[2]
        );
    }

    #[test]
    fn an_incompatible_media_plugin_leaves_the_next_one() {
        let root = tempfile::tempdir().unwrap();
        let write = |folder: &str, celesta: &str| {
            write_manifest(
                &root.path().join(folder).join("ffmpeg"),
                serde_json::json!({
                    "id": "ffmpeg", "version": "1", "protocol": 2, "celesta": celesta,
                    "roles": ["media"], "executable": { platform().0: "bin/codec" },
                }),
            );
        };
        write("first", ">=999.0.0");
        write("second", COMPATIBLE);

        let (plugins, errors) = discover(&[root.path().join("first"), root.path().join("second")]);
        assert!(
            matches!(errors[..], [DiscoveryError::Incompatible { .. }]),
            "{errors:?}"
        );
        let installed = InstalledPlugins {
            plugins,
            ..InstalledPlugins::default()
        };
        let media = installed.media_with_sibling(None);
        assert_eq!(media.len(), 1, "{media:?}");
        assert_eq!(
            media[0].directory,
            root.path().join("second").join("ffmpeg")
        );
    }
}
