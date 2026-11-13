//! The media plugins Celesta reads and writes media files through, their
//! components, which of their decoders is tried for a file, and what each
//! plugin's `describe` answered.

use crate::provider::{
    ComponentDescription, ComponentKind, MediaProvider, ProviderFactory, ProviderUse,
};
use celesta_plugin_host::{
    FFMPEG_PLUGIN_ID, MediaAudioEncoderEntry, MediaDecoderEntry, MediaManifest, MediaMuxerEntry,
    MediaOrigin, MediaVideoEncoderEntry, PluginKillSwitch, SkippedMedia,
};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::SystemTime;

/// The reason a component the manifest declares but `describe` left out is
/// unavailable.
const NOT_DESCRIBED: &str = "the plugin did not describe it";

/// The reason given for an unavailable component the plugin gave none for.
const NO_REASON: &str = "the plugin gave no reason";

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

/// Where `plugin`'s components rank among defaults, lowest first: built-in
/// 0, the bundled FFmpeg 1, other bundled 2, user 3; `None` for a project's,
/// which is never a default.
pub fn default_rank(plugin: &RegisteredPlugin) -> Option<u8> {
    match plugin.origin {
        MediaOrigin::BuiltIn => Some(0),
        MediaOrigin::Bundled if plugin.id == FFMPEG_PLUGIN_ID => Some(1),
        MediaOrigin::Bundled => Some(2),
        MediaOrigin::User => Some(3),
        MediaOrigin::Project => None,
    }
}

/// Where `plugin`'s decoders are tried: the user's, the bundled FFmpeg's,
/// other bundled, then a project's.
fn decoder_order(plugin: &RegisteredPlugin) -> u8 {
    match plugin.origin {
        MediaOrigin::User => 0,
        MediaOrigin::Bundled if plugin.id == FFMPEG_PLUGIN_ID => 1,
        MediaOrigin::Bundled => 2,
        MediaOrigin::Project => 3,
        MediaOrigin::BuiltIn => 4,
    }
}

/// A media plugin, as a [`MediaRegistry`] holds it: where it comes from,
/// the components its manifest declares (each list in manifest order), and
/// how to start an instance of it. `version` (the manifest's), `directory`,
/// and `executable` key its `describe` cache; a plugin in this process has
/// no `executable`.
#[derive(Clone)]
pub struct RegisteredPlugin {
    pub id: String,
    /// The display name of its manifest.
    pub name: String,
    pub version: String,
    pub origin: MediaOrigin,
    pub directory: PathBuf,
    pub executable: Option<PathBuf>,
    pub media: MediaManifest,
    pub factory: ProviderFactory,
}

impl fmt::Debug for RegisteredPlugin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisteredPlugin")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("version", &self.version)
            .field("origin", &self.origin)
            .field("directory", &self.directory)
            .field("executable", &self.executable)
            .field("media", &self.media)
            .finish_non_exhaustive()
    }
}

/// A component as its plugin's manifest declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentEntry {
    Decoder(MediaDecoderEntry),
    VideoEncoder(MediaVideoEncoderEntry),
    AudioEncoder(MediaAudioEncoderEntry),
    Muxer(MediaMuxerEntry),
}

impl ComponentEntry {
    pub fn kind(&self) -> ComponentKind {
        match self {
            Self::Decoder(_) => ComponentKind::Decoder,
            Self::VideoEncoder(_) => ComponentKind::VideoEncoder,
            Self::AudioEncoder(_) => ComponentKind::AudioEncoder,
            Self::Muxer(_) => ComponentKind::Muxer,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Decoder(entry) => &entry.id,
            Self::VideoEncoder(entry) => &entry.id,
            Self::AudioEncoder(entry) => &entry.id,
            Self::Muxer(entry) => &entry.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Decoder(entry) => &entry.name,
            Self::VideoEncoder(entry) => &entry.name,
            Self::AudioEncoder(entry) => &entry.name,
            Self::Muxer(entry) => &entry.name,
        }
    }

    /// `media`'s components: decoders, video encoders, audio encoders, then
    /// muxers, each in manifest order.
    fn of(media: &MediaManifest) -> Vec<Self> {
        let decoders = media.decoders.iter().cloned().map(Self::Decoder);
        let video = media.video_encoders.iter().cloned().map(Self::VideoEncoder);
        let audio = media.audio_encoders.iter().cloned().map(Self::AudioEncoder);
        let muxers = media.muxers.iter().cloned().map(Self::Muxer);
        decoders.chain(video).chain(audio).chain(muxers).collect()
    }
}

/// One component of a registered plugin, from its manifest: `plugin` is
/// the plugin's index in the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaComponentInfo {
    pub reference: ComponentRef,
    pub kind: ComponentKind,
    pub name: String,
    pub origin: MediaOrigin,
    pub plugin: usize,
    /// Its plugin's display name.
    pub plugin_name: String,
    pub plugin_version: String,
    pub directory: PathBuf,
    pub entry: ComponentEntry,
}

/// What a plugin's `describe` says of its components: one per component
/// its manifest declares, in [`MediaRegistry::components`]'s order. A
/// component the plugin did not describe, or every one when `describe`
/// failed, is unavailable with the reason; an unavailable one has only its
/// reason.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginDescription {
    pub components: Vec<ComponentDescription>,
}

/// What a cached `describe` answer is for: the plugin's folder, its
/// manifest's version, and its executable's size and modification time.
/// One that differs from the plugin's now misses the cache.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DescribeKey {
    directory: PathBuf,
    version: String,
    executable: Option<(u64, SystemTime)>,
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
    /// The successful `describe` answers, by plugin index.
    described: Mutex<HashMap<usize, (DescribeKey, Arc<PluginDescription>)>>,
    /// The plugins warned of describing components their manifest lacks.
    warned: Mutex<HashSet<usize>>,
    /// The recognized programs discovery left out, with why
    /// ([`MediaRegistry::with_skipped`]).
    skipped: Vec<SkippedMedia>,
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
            .flat_map(|plugin| &mut plugin.media.decoders)
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
                (0..entry.media.decoders.len()).map(move |decoder| (plugin, decoder))
            })
            .collect();
        // Stable, and already in search and manifest order. Built-in
        // plugins have no decoders.
        decoders.sort_by_key(|&(plugin, _)| decoder_order(&plugins[plugin]));
        Self {
            plugins,
            decoders,
            uses: Mutex::default(),
            described: Mutex::default(),
            warned: Mutex::default(),
            skipped: Vec::new(),
        }
    }

    /// This registry, knowing the programs discovery left out
    /// ([`celesta_plugin_host::MediaDiscovery::skipped`]), so that what names
    /// a component of one says why ([`MediaRegistry::skipped`]) rather than
    /// "not installed". One whose id a registered plugin has is ignored.
    pub fn with_skipped(mut self, mut skipped: Vec<SkippedMedia>) -> Self {
        skipped.retain(|skipped| self.plugin_index(&skipped.id).is_none());
        self.skipped = skipped;
        self
    }

    /// The program left out at discovery that would have served the plugin
    /// `id`, which no registered plugin has.
    pub fn skipped(&self, id: &str) -> Option<&SkippedMedia> {
        self.skipped_programs()
            .iter()
            .find(|skipped| skipped.id == id)
    }

    /// Every program left out at discovery whose id no registered plugin has.
    pub fn skipped_programs(&self) -> &[SkippedMedia] {
        &self.skipped
    }

    /// The plugins, in search order.
    pub fn plugins(&self) -> &[RegisteredPlugin] {
        &self.plugins
    }

    /// The index of the first plugin with `id`.
    pub fn plugin_index(&self, id: &str) -> Option<usize> {
        self.plugins.iter().position(|plugin| plugin.id == id)
    }

    /// Every plugin's components, from the manifests: plugins in search
    /// order, and within a plugin its decoders, video encoders, audio
    /// encoders, then muxers, each in manifest order.
    pub fn components(&self) -> Vec<MediaComponentInfo> {
        (0..self.plugins.len())
            .flat_map(|plugin| self.plugin_components(plugin))
            .collect()
    }

    /// The `kind` component `reference` names, if its plugin declares one.
    pub fn component(
        &self,
        reference: &ComponentRef,
        kind: ComponentKind,
    ) -> Option<MediaComponentInfo> {
        let plugin = self.plugin_index(&reference.plugin)?;
        self.plugin_components(plugin)
            .into_iter()
            .find(|info| info.kind == kind && info.reference.component == reference.component)
    }

    fn plugin_components(&self, index: usize) -> Vec<MediaComponentInfo> {
        let plugin = &self.plugins[index];
        ComponentEntry::of(&plugin.media)
            .into_iter()
            .map(|entry| MediaComponentInfo {
                reference: ComponentRef {
                    plugin: plugin.id.clone(),
                    component: entry.id().to_owned(),
                },
                kind: entry.kind(),
                name: entry.name().to_owned(),
                origin: plugin.origin,
                plugin: index,
                plugin_name: plugin.name.clone(),
                plugin_version: plugin.version.clone(),
                directory: plugin.directory.clone(),
                entry,
            })
            .collect()
    }

    /// What plugin `plugin` says of its components: the cached answer, or
    /// else one from an instance started for [`ProviderUse::Describe`] with
    /// `kill`, asked, and stopped. A plugin that cannot start, or whose
    /// `describe` fails, has every component unavailable with the error as
    /// the reason, and is asked again next time.
    pub fn describe(&self, plugin: usize, kill: &PluginKillSwitch) -> Arc<PluginDescription> {
        let key = self.describe_key(plugin);
        if let Some(cached) = self.cached(plugin, &key) {
            return cached;
        }
        match (self.plugins[plugin].factory)(kill, ProviderUse::Describe) {
            Ok(mut provider) => self.ask(plugin, key, provider.as_mut()),
            Err(error) => self.failed(plugin, &error.to_string()),
        }
    }

    /// [`Self::describe`], asking `provider`, an instance of plugin `plugin`
    /// the caller started (an export's), when the answer is not cached. It
    /// fills the same cache.
    pub fn describe_with(
        &self,
        plugin: usize,
        provider: &mut dyn MediaProvider,
    ) -> Arc<PluginDescription> {
        let key = self.describe_key(plugin);
        if let Some(cached) = self.cached(plugin, &key) {
            return cached;
        }
        self.ask(plugin, key, provider)
    }

    /// The key plugin `plugin`'s answer would be cached under now. An
    /// executable whose metadata cannot be read has none.
    fn describe_key(&self, plugin: usize) -> DescribeKey {
        let plugin = &self.plugins[plugin];
        let executable = plugin.executable.as_deref().and_then(|executable| {
            let metadata = std::fs::metadata(executable).ok()?;
            Some((metadata.len(), metadata.modified().ok()?))
        });
        DescribeKey {
            directory: plugin.directory.clone(),
            version: plugin.version.clone(),
            executable,
        }
    }

    fn cached(&self, plugin: usize, key: &DescribeKey) -> Option<Arc<PluginDescription>> {
        let described = self
            .described
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        described
            .get(&plugin)
            .filter(|(cached, _)| cached == key)
            .map(|(_, description)| description.clone())
    }

    /// Asks `provider` to describe plugin `plugin`, and caches the answer
    /// under `key`, taken before it was asked, if it succeeded.
    fn ask(
        &self,
        plugin: usize,
        key: DescribeKey,
        provider: &mut dyn MediaProvider,
    ) -> Arc<PluginDescription> {
        let answered = match provider.describe() {
            Ok(answered) => answered,
            Err(error) => return self.failed(plugin, &error.to_string()),
        };
        let description = Arc::new(self.merge(plugin, answered));
        self.described
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(plugin, (key, description.clone()));
        description
    }

    /// Every component of plugin `plugin` unavailable for `reason`.
    fn failed(&self, plugin: usize, reason: &str) -> Arc<PluginDescription> {
        let components = ComponentEntry::of(&self.plugins[plugin].media)
            .iter()
            .map(|entry| ComponentDescription::unavailable(entry.kind(), entry.id(), reason))
            .collect();
        Arc::new(PluginDescription { components })
    }

    /// `answered` laid over plugin `plugin`'s manifest: each manifest
    /// component as described (an unavailable one with only its reason),
    /// or unavailable when it was not. Components the manifest lacks are
    /// left out, and named on stderr once per plugin.
    fn merge(&self, plugin: usize, mut answered: Vec<ComponentDescription>) -> PluginDescription {
        let entries = ComponentEntry::of(&self.plugins[plugin].media);
        let components = entries
            .iter()
            .map(|entry| {
                let found = answered.iter().position(|component| {
                    component.kind == entry.kind() && component.id == entry.id()
                });
                match found {
                    Some(index) => only_reason_when_unavailable(answered.remove(index)),
                    None => {
                        ComponentDescription::unavailable(entry.kind(), entry.id(), NOT_DESCRIBED)
                    }
                }
            })
            .collect();
        // What is left the manifest does not declare (or repeats an answer).
        if !answered.is_empty()
            && self
                .warned
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(plugin)
        {
            let ids: Vec<&str> = answered
                .iter()
                .map(|component| component.id.as_str())
                .collect();
            eprintln!(
                "celesta: {} described components its manifest does not declare: {}",
                self.plugins[plugin].id,
                ids.join(", ")
            );
        }
        PluginDescription { components }
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
                self.plugins[plugin].media.decoders[decoder]
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
                        component: entry.media.decoders[decoder].id.clone(),
                    },
                )
            })
            .collect()
    }

    /// The plugin at `index`, as [`Self::decoder_candidates`] gives it.
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

/// `component` as the spec has it: one that is not available has its
/// reason and nothing else.
fn only_reason_when_unavailable(component: ComponentDescription) -> ComponentDescription {
    if component.available {
        return component;
    }
    let reason = component.reason.unwrap_or_else(|| NO_REASON.to_owned());
    ComponentDescription::unavailable(component.kind, &component.id, reason)
}

static REGISTRY: RwLock<Option<Arc<MediaRegistry>>> = RwLock::new(None);

/// Chooses the media plugins [`crate::MediaBackend::new`]'s backends use:
/// each takes the registry chosen when it first needs one, so a backend made
/// earlier that has not needed one yet gets this choice, and one that has
/// keeps its own. `None` leaves none, so reading media fails with
/// [`crate::MediaError::NoDecoder`].
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
    use crate::backend::MediaBackend;
    use crate::error::MediaError;
    use crate::provider::{
        ComponentDescription, Decoder, MediaProvider, PixelFormat, ProviderUse, SampleRates,
        VideoRequest,
    };
    use crate::types::{AudioBuffer, MediaProbe, VideoFrame};
    use celesta_plugin_host::{
        MediaAudioEncoderEntry, MediaManifest, MediaMuxerEntry, MediaVideoEncoderEntry,
        PluginKillSwitch,
    };

    /// What a fake's `describe` answers: the components, or the text of a
    /// [`MediaError::Codec`].
    type Described = Result<Vec<ComponentDescription>, String>;

    /// A plugin in this process that describes itself as it was told and
    /// reads every file as one with no streams.
    struct Fake {
        described: Described,
    }

    impl MediaProvider for Fake {
        fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
            self.described.clone().map_err(MediaError::Codec)
        }

        fn decoder(&mut self) -> Option<&mut dyn Decoder> {
            Some(self)
        }
    }

    impl Decoder for Fake {
        fn probe(&mut self, _: &str, _: &Path) -> Result<MediaProbe, MediaError> {
            Ok(MediaProbe {
                duration: None,
                video: None,
                audio: Vec::new(),
            })
        }

        fn decode_video(
            &mut self,
            _: &str,
            _: &VideoRequest<'_>,
        ) -> Result<VideoFrame, MediaError> {
            unreachable!("never decodes")
        }

        fn decode_audio(
            &mut self,
            _: &str,
            _: &Path,
            _: u32,
            _: u16,
        ) -> Result<AudioBuffer, MediaError> {
            unreachable!("never decodes")
        }
    }

    /// The uses a fake plugin's factory was started for, in order.
    #[derive(Clone, Default)]
    struct Starts(Arc<Mutex<Vec<ProviderUse>>>);

    impl Starts {
        fn uses(&self) -> Vec<ProviderUse> {
            self.0.lock().unwrap().clone()
        }

        fn count(&self) -> usize {
            self.0.lock().unwrap().len()
        }
    }

    /// Plugin `p`, with `media`, whose factory notes each start in `starts`
    /// and whose `n`th instance (from 0) describes itself as `answer(n)`.
    fn fake(
        media: MediaManifest,
        executable: Option<PathBuf>,
        starts: &Starts,
        answer: impl Fn(usize) -> Described + Send + Sync + 'static,
    ) -> RegisteredPlugin {
        let starts = starts.clone();
        RegisteredPlugin {
            id: "p".into(),
            name: "p".into(),
            version: "1.0.0".into(),
            origin: MediaOrigin::User,
            directory: PathBuf::from("/plugins/p"),
            executable,
            media,
            factory: Arc::new(move |_, purpose| {
                let mut uses = starts.0.lock().unwrap();
                uses.push(purpose);
                let described = answer(uses.len() - 1);
                Ok(Box::new(Fake { described }) as Box<dyn MediaProvider>)
            }),
        }
    }

    fn decoder(id: &str) -> MediaDecoderEntry {
        MediaDecoderEntry {
            id: id.into(),
            name: format!("Decoder {id}"),
            extensions: Vec::new(),
        }
    }

    fn video_encoder(id: &str) -> MediaVideoEncoderEntry {
        MediaVideoEncoderEntry {
            id: id.into(),
            name: format!("Video {id}"),
            codecs: vec!["avc1".into()],
            input_formats: None,
        }
    }

    fn audio_encoder(id: &str) -> MediaAudioEncoderEntry {
        MediaAudioEncoderEntry {
            id: id.into(),
            name: format!("Audio {id}"),
            codecs: vec!["mp4a".into()],
        }
    }

    fn muxer(id: &str) -> MediaMuxerEntry {
        MediaMuxerEntry {
            id: id.into(),
            name: format!("Muxer {id}"),
            extensions: vec!["mp4".into()],
            video: vec!["avc1".into()],
            audio: Some(vec!["mp4a".into()]),
            default_video: Some("avc1".into()),
            default_audio: Some("mp4a".into()),
            trims_leading_audio: true,
            sequence: false,
        }
    }

    /// One component of each kind: decoder `d`, video encoder `v`, audio
    /// encoder `a`, and muxer `m`.
    fn one_of_each() -> MediaManifest {
        MediaManifest {
            decoders: vec![decoder("d")],
            video_encoders: vec![video_encoder("v")],
            audio_encoders: vec![audio_encoder("a")],
            muxers: vec![muxer("m")],
        }
    }

    /// An available `kind` `id` that takes the pipe.
    fn available(kind: ComponentKind, id: &str) -> ComponentDescription {
        ComponentDescription {
            available: true,
            reason: None,
            transports: vec!["pipe".into()],
            ..ComponentDescription::unavailable(kind, id, "")
        }
    }

    /// Every component of [`one_of_each`], available.
    fn all_available() -> Vec<ComponentDescription> {
        vec![
            available(ComponentKind::Decoder, "d"),
            available(ComponentKind::VideoEncoder, "v"),
            available(ComponentKind::AudioEncoder, "a"),
            available(ComponentKind::Muxer, "m"),
        ]
    }

    /// Plugin `id` of `origin`, declaring decoder `d` for `.mp4` files when
    /// `decodes`.
    fn ranked(id: &str, origin: MediaOrigin, decodes: bool) -> RegisteredPlugin {
        let decoders = if decodes {
            vec![MediaDecoderEntry {
                extensions: vec!["mp4".into()],
                ..decoder("d")
            }]
        } else {
            Vec::new()
        };
        RegisteredPlugin {
            id: id.into(),
            name: id.into(),
            origin,
            media: MediaManifest {
                decoders,
                video_encoders: Vec::new(),
                audio_encoders: Vec::new(),
                muxers: Vec::new(),
            },
            ..fake(one_of_each(), None, &Starts::default(), |_| Ok(Vec::new()))
        }
    }

    #[test]
    fn ffmpeg_ranks_first_among_bundled() {
        let plugins = vec![
            ranked("avfoundation", MediaOrigin::Bundled, true),
            ranked("ffmpeg", MediaOrigin::Bundled, true),
            ranked("u", MediaOrigin::User, true),
            ranked("p", MediaOrigin::Project, true),
            ranked("builtin", MediaOrigin::BuiltIn, false),
        ];
        let ranks: Vec<Option<u8>> = plugins.iter().map(default_rank).collect();
        assert_eq!(ranks, [Some(2), Some(1), Some(3), None, Some(0)]);

        let registry = MediaRegistry::new(plugins);
        let order: Vec<String> = registry
            .decoder_candidates(Path::new("a.mp4"))
            .iter()
            .map(|(_, reference)| reference.plugin.clone())
            .collect();
        assert_eq!(order, ["u", "ffmpeg", "avfoundation", "p"]);
    }

    #[test]
    fn components_list_every_kind_in_order() {
        let starts = Starts::default();
        let registry =
            MediaRegistry::new(vec![fake(one_of_each(), None, &starts, |_| Ok(Vec::new()))]);
        let components = registry.components();
        let listed: Vec<(String, ComponentKind, &str)> = components
            .iter()
            .map(|info| (info.reference.to_string(), info.kind, info.name.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                ("p/d".to_owned(), ComponentKind::Decoder, "Decoder d"),
                ("p/v".to_owned(), ComponentKind::VideoEncoder, "Video v"),
                ("p/a".to_owned(), ComponentKind::AudioEncoder, "Audio a"),
                ("p/m".to_owned(), ComponentKind::Muxer, "Muxer m"),
            ]
        );
        let muxer_info = &components[3];
        assert_eq!(muxer_info.origin, MediaOrigin::User);
        assert_eq!(muxer_info.plugin, 0);
        assert_eq!(muxer_info.plugin_version, "1.0.0");
        assert_eq!(muxer_info.directory, Path::new("/plugins/p"));
        assert_eq!(muxer_info.entry, ComponentEntry::Muxer(muxer("m")));

        assert_eq!(registry.plugin_index("p"), Some(0));
        assert_eq!(registry.plugin_index("q"), None);
        assert_eq!(registry.plugins().len(), 1);
        let reference: ComponentRef = "p/v".parse().unwrap();
        assert_eq!(
            registry.component(&reference, ComponentKind::VideoEncoder),
            Some(components[1].clone())
        );
        assert_eq!(registry.component(&reference, ComponentKind::Muxer), None);
        assert_eq!(
            registry.component(&"q/v".parse().unwrap(), ComponentKind::VideoEncoder),
            None
        );
        assert_eq!(starts.count(), 0, "listing starts nothing");
    }

    #[test]
    fn describe_is_cached_until_the_program_changes() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("program");
        std::fs::write(&program, "a").unwrap();
        let starts = Starts::default();
        let registry = MediaRegistry::new(vec![fake(
            one_of_each(),
            Some(program.clone()),
            &starts,
            |_| Ok(all_available()),
        )]);
        let kill = PluginKillSwitch::default();
        let first = registry.describe(0, &kill);
        let second = registry.describe(0, &kill);
        assert_eq!(starts.count(), 1);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.components, all_available());

        std::fs::write(&program, "ab").unwrap();
        registry.describe(0, &kill);
        assert_eq!(starts.count(), 2, "a changed program is described again");
        registry.describe(0, &kill);
        assert_eq!(starts.count(), 2);
    }

    #[test]
    fn a_failed_describe_is_not_cached() {
        let starts = Starts::default();
        let registry = MediaRegistry::new(vec![fake(one_of_each(), None, &starts, |start| {
            if start < 2 {
                Err("cannot describe".into())
            } else {
                Ok(all_available())
            }
        })]);
        let kill = PluginKillSwitch::default();
        let answers: Vec<Arc<PluginDescription>> =
            (0..3).map(|_| registry.describe(0, &kill)).collect();
        assert_eq!(starts.count(), 3);
        let failed = vec![
            ComponentDescription::unavailable(ComponentKind::Decoder, "d", "cannot describe"),
            ComponentDescription::unavailable(ComponentKind::VideoEncoder, "v", "cannot describe"),
            ComponentDescription::unavailable(ComponentKind::AudioEncoder, "a", "cannot describe"),
            ComponentDescription::unavailable(ComponentKind::Muxer, "m", "cannot describe"),
        ];
        assert_eq!(answers[0].components, failed);
        assert_eq!(answers[1].components, failed);
        assert_eq!(answers[2].components, all_available());
        registry.describe(0, &kill);
        assert_eq!(starts.count(), 3, "the success is cached");
    }

    #[test]
    fn components_carry_their_plugins_display_name() {
        let registry = MediaRegistry::new(vec![RegisteredPlugin {
            name: "Test codec".into(),
            ..fake(one_of_each(), None, &Starts::default(), |_| Ok(Vec::new()))
        }]);
        let reference: ComponentRef = "p/v".parse().unwrap();
        let info = registry
            .component(&reference, ComponentKind::VideoEncoder)
            .unwrap();
        assert_eq!(info.plugin_name, "Test codec");
    }

    #[test]
    fn a_plugin_that_cannot_start_is_unavailable() {
        let registry = MediaRegistry::new(vec![RegisteredPlugin {
            factory: Arc::new(|_, _| Err(MediaError::CodecUnavailable("no program".into()))),
            ..fake(one_of_each(), None, &Starts::default(), |_| Ok(Vec::new()))
        }]);
        let described = registry.describe(0, &PluginKillSwitch::default());
        let reason = "the media plugin is unavailable: no program";
        assert_eq!(
            described.components[0],
            ComponentDescription::unavailable(ComponentKind::Decoder, "d", reason)
        );
        assert!(
            described
                .components
                .iter()
                .all(|component| component.reason.as_deref() == Some(reason))
        );
    }

    #[test]
    fn described_components_follow_the_manifest() {
        let starts = Starts::default();
        let media = MediaManifest {
            video_encoders: vec![video_encoder("a"), video_encoder("b")],
            ..MediaManifest::default()
        };
        let registry = MediaRegistry::new(vec![fake(media, None, &starts, |_| {
            Ok(vec![
                available(ComponentKind::VideoEncoder, "c"),
                ComponentDescription {
                    input_formats: vec![PixelFormat::Yuv420p],
                    ..available(ComponentKind::VideoEncoder, "a")
                },
                // Another kind's `b` is not the video encoder `b`.
                available(ComponentKind::Muxer, "b"),
            ])
        })]);
        let described = registry.describe(0, &PluginKillSwitch::default());
        assert_eq!(
            described.components,
            [
                ComponentDescription {
                    input_formats: vec![PixelFormat::Yuv420p],
                    ..available(ComponentKind::VideoEncoder, "a")
                },
                ComponentDescription::unavailable(
                    ComponentKind::VideoEncoder,
                    "b",
                    "the plugin did not describe it"
                ),
            ]
        );
    }

    #[test]
    fn unavailable_components_have_only_a_reason() {
        let starts = Starts::default();
        let media = MediaManifest {
            audio_encoders: vec![audio_encoder("a"), audio_encoder("b")],
            ..MediaManifest::default()
        };
        let registry = MediaRegistry::new(vec![fake(media, None, &starts, |_| {
            let mut a = available(ComponentKind::AudioEncoder, "a");
            a.available = false;
            a.reason = Some("this build has no libopus".into());
            a.sample_rates = Some(SampleRates::Only(vec![48_000]));
            a.channels = vec![1, 2];
            let mut b = available(ComponentKind::AudioEncoder, "b");
            b.available = false;
            Ok(vec![a, b])
        })]);
        let described = registry.describe(0, &PluginKillSwitch::default());
        assert_eq!(
            described.components,
            [
                ComponentDescription::unavailable(
                    ComponentKind::AudioEncoder,
                    "a",
                    "this build has no libopus"
                ),
                ComponentDescription::unavailable(
                    ComponentKind::AudioEncoder,
                    "b",
                    "the plugin gave no reason"
                ),
            ]
        );
    }

    #[test]
    fn describe_with_fills_the_same_cache() {
        let starts = Starts::default();
        let registry =
            MediaRegistry::new(vec![fake(one_of_each(), None, &starts, |_| Ok(Vec::new()))]);
        let mut provider = Fake {
            described: Ok(all_available()),
        };
        let answered = registry.describe_with(0, &mut provider);
        assert_eq!(answered.components, all_available());
        let cached = registry.describe(0, &PluginKillSwitch::default());
        assert_eq!(starts.count(), 0);
        assert!(Arc::ptr_eq(&answered, &cached));
    }

    #[test]
    fn factories_learn_their_use() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clip.mp4");
        std::fs::write(&path, "").unwrap();
        let starts = Starts::default();
        let registry = Arc::new(MediaRegistry::new(vec![fake(
            one_of_each(),
            None,
            &starts,
            |_| Ok(all_available()),
        )]));
        registry.describe(0, &PluginKillSwitch::default());
        assert_eq!(starts.uses(), [ProviderUse::Describe]);
        MediaBackend::with_registry(registry).probe(&path).unwrap();
        assert_eq!(starts.uses(), [ProviderUse::Describe, ProviderUse::Decode]);
    }

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
