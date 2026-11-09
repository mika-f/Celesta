//! The media plugins Celesta reads and writes media files through, and
//! which of their decoders is tried for a file.

use crate::codec::ProviderFactory;
use celesta_plugin_host::{MediaDecoderEntry, MediaOrigin};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

/// A plugin's component by its reference, `<plugin>/<component>`, such as
/// `ffmpeg/video`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentRef {
    pub plugin: String,
    pub component: String,
}

impl fmt::Display for ComponentRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.plugin, self.component)
    }
}

/// A text that is not `<plugin>/<component>` with neither side empty.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a component reference: write it as `<plugin>/<component>`")]
pub struct InvalidComponentRef(pub String);

impl FromStr for ComponentRef {
    type Err = InvalidComponentRef;

    /// Splits at the first `/`: a plugin ID has none.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.split_once('/') {
            Some((plugin, component)) if !plugin.is_empty() && !component.is_empty() => Ok(Self {
                plugin: plugin.to_owned(),
                component: component.to_owned(),
            }),
            _ => Err(InvalidComponentRef(text.to_owned())),
        }
    }
}

/// A media plugin, as a [`MediaRegistry`] holds it: where it comes from,
/// the decoders its manifest declares (in manifest order), and how to start
/// an instance of it.
#[derive(Clone)]
pub struct RegisteredPlugin {
    pub id: String,
    pub origin: MediaOrigin,
    pub decoders: Vec<MediaDecoderEntry>,
    pub factory: ProviderFactory,
}

impl fmt::Debug for RegisteredPlugin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisteredPlugin")
            .field("id", &self.id)
            .field("origin", &self.origin)
            .field("decoders", &self.decoders)
            .finish_non_exhaustive()
    }
}

/// A file a decoder read: the probe of `path` that routed it to `decoder`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoderUse {
    pub decoder: ComponentRef,
    pub origin: MediaOrigin,
    pub path: PathBuf,
}

/// The media plugins, in search order, and the files their project
/// plugins' decoders read, which the editor shows so a project cannot
/// decode files unnoticed.
#[derive(Debug)]
pub struct MediaRegistry {
    plugins: Vec<RegisteredPlugin>,
    /// Every decoder, as (plugin index, decoder index), in the order they
    /// are tried: by origin, then search order, then manifest order.
    decoders: Vec<(usize, usize)>,
    uses: Mutex<ProjectUses>,
}

#[derive(Debug, Default)]
struct ProjectUses {
    /// Bumped with each use added, so a reader can tell the list changed.
    generation: u64,
    uses: Vec<DecoderUse>,
}

impl MediaRegistry {
    /// A registry of `plugins`, in search order. Decoder extensions are
    /// matched lower-case and without a leading dot, as a manifest's are
    /// read, however a registered plugin spells them.
    pub fn new(mut plugins: Vec<RegisteredPlugin>) -> Self {
        for extension in plugins
            .iter_mut()
            .flat_map(|plugin| &mut plugin.decoders)
            .flat_map(|decoder| &mut decoder.extensions)
        {
            *extension = extension
                .strip_prefix('.')
                .unwrap_or(extension)
                .to_lowercase();
        }
        let mut decoders: Vec<(usize, usize)> = plugins
            .iter()
            .enumerate()
            .flat_map(|(plugin, entry)| {
                (0..entry.decoders.len()).map(move |decoder| (plugin, decoder))
            })
            .collect();
        // Stable, and already in search and manifest order.
        decoders.sort_by_key(|&(plugin, _)| plugins[plugin].origin);
        Self {
            plugins,
            decoders,
            uses: Mutex::default(),
        }
    }

    /// Whether any plugin declares a decoder.
    pub fn has_decoders(&self) -> bool {
        !self.decoders.is_empty()
    }

    /// The decoders to try for `path`, in order, each with the index of
    /// its plugin: those that declare its extension (case-insensitively),
    /// or every decoder when none does or it has none.
    pub fn decoder_candidates(&self, path: &Path) -> Vec<(usize, ComponentRef)> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_lowercase);
        let reads = |&(plugin, decoder): &(usize, usize)| {
            extension.as_ref().is_some_and(|extension| {
                self.plugins[plugin].decoders[decoder]
                    .extensions
                    .contains(extension)
            })
        };
        let matching: Vec<(usize, usize)> = self.decoders.iter().copied().filter(reads).collect();
        let candidates = if matching.is_empty() {
            &self.decoders
        } else {
            &matching
        };
        candidates
            .iter()
            .map(|&(plugin, decoder)| {
                let entry = &self.plugins[plugin];
                (
                    plugin,
                    ComponentRef {
                        plugin: entry.id.clone(),
                        component: entry.decoders[decoder].id.clone(),
                    },
                )
            })
            .collect()
    }

    /// The plugin that encodes and muxes exports until exports choose
    /// their components: the first bundled one in search order, so a
    /// decoder-only plugin the user installs cannot take over exports;
    /// when none is bundled, the first in search order that is not a
    /// project's, whatever its decoders. A project's plugin never encodes,
    /// so with only project plugins there is none (`NoCodec`).
    pub fn legacy_encoder(&self) -> Option<usize> {
        self.plugins
            .iter()
            .position(|plugin| plugin.origin == MediaOrigin::Bundled)
            .or_else(|| {
                self.plugins
                    .iter()
                    .position(|plugin| plugin.origin != MediaOrigin::Project)
            })
    }

    /// The plugin at `index`, as [`Self::decoder_candidates`] and
    /// [`Self::legacy_encoder`] give it.
    pub fn plugin(&self, index: usize) -> &RegisteredPlugin {
        &self.plugins[index]
    }

    /// Notes that `decoder`, of a plugin from `origin`, read `path`. Only a
    /// project plugin's use is kept, once per decoder and path.
    pub fn record_use(&self, decoder: &ComponentRef, origin: MediaOrigin, path: &Path) {
        if origin != MediaOrigin::Project {
            return;
        }
        let mut project = self.uses.lock().unwrap_or_else(PoisonError::into_inner);
        if project
            .uses
            .iter()
            .any(|used| used.decoder == *decoder && used.path == path)
        {
            return;
        }
        project.uses.push(DecoderUse {
            decoder: decoder.clone(),
            origin,
            path: path.to_owned(),
        });
        project.generation += 1;
    }

    /// The generation [`Self::project_decoder_uses`] would return, without
    /// copying the list.
    pub fn project_decoder_generation(&self) -> u64 {
        self.uses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .generation
    }

    /// The files project plugins' decoders read, in the order first read,
    /// and the list's generation, which changes whenever the list does.
    pub fn project_decoder_uses(&self) -> (u64, Vec<DecoderUse>) {
        let project = self.uses.lock().unwrap_or_else(PoisonError::into_inner);
        (project.generation, project.uses.clone())
    }
}

static REGISTRY: RwLock<Option<Arc<MediaRegistry>>> = RwLock::new(None);

/// Chooses the media plugins [`crate::MediaBackend::new`]'s backends use:
/// each takes the registry chosen when it first needs one, so a backend made
/// earlier that has not needed one yet gets this choice, and one that has
/// keeps its own. `None` leaves none, so reading media fails with
/// [`crate::MediaError::NoDecoder`] and writing it with
/// [`crate::MediaError::NoCodec`].
pub fn set_registry(registry: Option<Arc<MediaRegistry>>) {
    *REGISTRY.write().unwrap_or_else(PoisonError::into_inner) = registry;
}

/// The registry [`set_registry`] chose.
pub fn registry() -> Option<Arc<MediaRegistry>> {
    REGISTRY
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generation_changes_with_each_new_project_use_only() {
        let registry = MediaRegistry::new(Vec::new());
        let decoder: ComponentRef = "proj/dec".parse().unwrap();
        assert_eq!(registry.project_decoder_generation(), 0);
        registry.record_use(&decoder, MediaOrigin::Project, Path::new("/a"));
        assert_eq!(registry.project_decoder_generation(), 1);
        // Again, and from a user plugin: nothing new.
        registry.record_use(&decoder, MediaOrigin::Project, Path::new("/a"));
        registry.record_use(&decoder, MediaOrigin::User, Path::new("/b"));
        assert_eq!(registry.project_decoder_generation(), 1);
        assert_eq!(registry.project_decoder_uses().0, 1);
    }
}
