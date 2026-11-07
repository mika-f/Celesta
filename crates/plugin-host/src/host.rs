//! Plugin processes, one per plugin a renderer uses, behind the renderer's
//! `ContentProvider` interface.

use crate::child::{PluginChild, RequestError};
use crate::kill::PluginKillSwitch;
use crate::manifest::{InstalledPlugins, PluginEntry};
use celesta_plugin::protocol::{ContentKind, Message, decode_content_frame, decode_opened};
use celesta_plugin::{
    ContentFrame, ContentModel, ContentProvider, ModelId, Opened, PluginError, TextureSource,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a content plugin may take to open a model (loading a file it
/// reads itself, such as an avatar), draw a frame, and close a model.
const OPEN_TIMEOUT: Duration = Duration::from_secs(120);
const FRAME_TIMEOUT: Duration = Duration::from_secs(30);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// The deadlines a host gives its plugins' requests.
#[derive(Clone, Copy)]
struct Deadlines {
    open: Duration,
    frame: Duration,
    close: Duration,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self {
            open: OPEN_TIMEOUT,
            frame: FRAME_TIMEOUT,
            close: CLOSE_TIMEOUT,
        }
    }
}

/// A running content plugin, checked to be the plugin its entry names.
struct PluginProcess(PluginChild);

impl PluginProcess {
    /// Starts `entry`'s program, registered with `kill` before it is asked
    /// anything, so killing the switch also ends a slow `hello`.
    fn start(entry: &PluginEntry, kill: &PluginKillSwitch) -> Result<Self, RequestError> {
        let executable = entry.executable().ok_or_else(|| {
            RequestError::Unavailable(format!(
                "{} has no executable for this platform",
                entry.directory.display()
            ))
        })?;
        let mut child =
            PluginChild::start(&executable, &entry.directory).map_err(RequestError::Unavailable)?;
        kill.register(child.kill_handle());
        let hello = child.hello().map_err(|error| match error {
            refused @ RequestError::Refused(_) => RequestError::Unavailable(refused.message()),
            error => error,
        })?;
        if hello.header["plugin"] != entry.manifest.id.as_str() {
            return Err(RequestError::Unavailable(format!(
                "{} says it is plugin {}, not {}",
                executable.display(),
                hello.header["plugin"],
                entry.manifest.id
            )));
        }
        Ok(Self(child))
    }

    fn request(
        &mut self,
        request: Message,
        expected: &str,
        timeout: Duration,
    ) -> Result<Message, RequestError> {
        self.0.request(request, expected, timeout)
    }
}

struct OpenModel {
    plugin: String,
    assets: BTreeMap<String, PathBuf>,
    /// What the model was first opened as; its frames are decoded as this,
    /// and a reopened model must be the same.
    kind: ContentKind,
    /// The plugin process's own id for the model; `None` after the process
    /// was restarted, until it is opened again.
    remote: Option<ModelId>,
    /// Why its frames are refused: reopened in a restarted process, it came
    /// back as another kind. Later frames are refused with it without
    /// reopening it, until [`PluginHost::stop`].
    refused: Option<PluginError>,
}

/// Runs each plugin a renderer asks for in a process of its own, started on
/// first use. A process that went away is started again once per request,
/// reopening the models it had; one that missed its deadline is started
/// again on the next request instead, so a hang costs one deadline. Every
/// process is registered with the host's [`PluginKillSwitch`]: once that is
/// killed, every request fails as unavailable and no process starts. A
/// process that cannot be started is not tried again until [`Self::stop`]:
/// a preview asks for every frame.
pub struct PluginHost {
    plugins: BTreeMap<String, PluginEntry>,
    processes: HashMap<String, PluginProcess>,
    /// Why each plugin whose process could not be started (spawned and
    /// checked with `hello`) could not be, by plugin id.
    failed: HashMap<String, String>,
    models: HashMap<ModelId, OpenModel>,
    next: u64,
    deadlines: Deadlines,
    kill: PluginKillSwitch,
    /// The last request's process was stopped on purpose (it missed its
    /// deadline, or was killed), so [`Self::retry`] does not send it again.
    stopped: bool,
}

impl PluginHost {
    pub fn new(plugins: impl IntoIterator<Item = PluginEntry>) -> Self {
        Self {
            plugins: plugins
                .into_iter()
                .map(|entry| (entry.manifest.id.clone(), entry))
                .collect(),
            processes: HashMap::new(),
            failed: HashMap::new(),
            models: HashMap::new(),
            next: 0,
            deadlines: Deadlines::default(),
            kill: PluginKillSwitch::new(),
            stopped: false,
        }
    }

    /// Registers every process this host has started, and every one it
    /// starts, with `kill`, which another thread can kill to end a request
    /// in progress, such as an export's Cancel. The host's previous switch
    /// still kills the processes started before.
    pub fn with_kill_switch(mut self, kill: PluginKillSwitch) -> Self {
        for process in self.processes.values() {
            kill.register(process.0.kill_handle());
        }
        self.kill = kill;
        self
    }

    /// The switch that kills every process this host starts; clones share
    /// it.
    pub fn kill_switch(&self) -> PluginKillSwitch {
        self.kill.clone()
    }

    /// The plugins in every location Celesta searches, `extra` first (see
    /// [`InstalledPlugins::find`]). Skipped plugins are reported on stderr.
    pub fn discover(extra: &[PathBuf]) -> Self {
        InstalledPlugins::find_reporting(extra, &[]).host()
    }

    pub fn plugins(&self) -> impl Iterator<Item = &PluginEntry> {
        self.plugins.values()
    }

    fn process(&mut self, plugin: &str) -> Result<&mut PluginProcess, PluginError> {
        if let Some(message) = self.failed.get(plugin) {
            return Err(PluginError::Unavailable {
                plugin: plugin.to_owned(),
                message: message.clone(),
            });
        }
        if !self.processes.contains_key(plugin) {
            let entry = self
                .plugins
                .get(plugin)
                .ok_or_else(|| PluginError::UnknownPlugin(plugin.to_owned()))?;
            if self.kill.is_killed() {
                self.stopped = true;
                return Err(PluginError::Unavailable {
                    plugin: plugin.to_owned(),
                    message: "it was not started, as its plugins were stopped".to_owned(),
                });
            }
            let process = PluginProcess::start(entry, &self.kill).map_err(|error| {
                // Not when its switch stopped it: that says nothing of the
                // plugin, and a switch given later may let it start.
                if !self.kill.is_killed() {
                    self.failed.insert(plugin.to_owned(), error.message());
                }
                self.unavailable(plugin, error)
            })?;
            self.processes.insert(plugin.to_owned(), process);
        }
        Ok(self.processes.get_mut(plugin).expect("started above"))
    }

    /// Runs `attempt` against `plugin`'s process; once more on a fresh
    /// process if the first went away (but not if it was stopped, or did
    /// not start).
    fn retry<T>(
        &mut self,
        plugin: &str,
        mut attempt: impl FnMut(&mut Self) -> Result<T, PluginError>,
    ) -> Result<T, PluginError> {
        self.stopped = false;
        match attempt(self) {
            Err(stopped @ PluginError::Unavailable { .. }) if self.stopped => {
                // Started again when next needed.
                self.restart(plugin);
                Err(stopped)
            }
            Err(PluginError::Unavailable { .. }) if !self.failed.contains_key(plugin) => {
                self.restart(plugin);
                attempt(self)
            }
            result => result,
        }
    }

    /// Stops `plugin`'s process, as when it should reload; it starts again,
    /// reopening its models, when next needed. A process that could not be
    /// started, and models it refused, are tried again.
    pub fn stop(&mut self, plugin: &str) {
        self.failed.remove(plugin);
        for model in self.models.values_mut() {
            if model.plugin == plugin {
                model.refused = None;
            }
        }
        self.restart(plugin);
    }

    /// Forgets `plugin`'s process; its models are reopened when next used.
    fn restart(&mut self, plugin: &str) {
        self.processes.remove(plugin);
        for model in self.models.values_mut() {
            if model.plugin == plugin {
                model.remote = None;
            }
        }
    }

    fn send(
        &mut self,
        plugin: &str,
        message: Message,
        expected: &str,
        timeout: Duration,
    ) -> Result<Message, PluginError> {
        let answer = self.process(plugin)?.request(message, expected, timeout);
        answer.map_err(|error| match error {
            refused @ RequestError::Refused(_) => PluginError::Failed {
                plugin: plugin.to_owned(),
                message: refused.message(),
            },
            error => self.unavailable(plugin, error),
        })
    }

    /// `plugin` cannot be talked to for `error`; noting whether it was
    /// stopped, which [`Self::retry`] does not retry.
    fn unavailable(&mut self, plugin: &str, error: RequestError) -> PluginError {
        self.stopped |= matches!(error, RequestError::Stopped(_));
        PluginError::Unavailable {
            plugin: plugin.to_owned(),
            message: error.message(),
        }
    }

    fn open_remote(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        // A path that is not UTF-8 cannot be sent as JSON.
        let assets = serde_json::to_value(assets).map_err(|error| PluginError::Failed {
            plugin: plugin.to_owned(),
            message: format!("cannot send these assets to the plugin: {error}"),
        })?;
        let request = Message::new(json!({ "type": "open", "assets": assets }));
        let answer = self.send(plugin, request, "opened", self.deadlines.open)?;
        let opened = decode_opened(&answer).map_err(|error| PluginError::Failed {
            plugin: plugin.to_owned(),
            message: error.to_string(),
        })?;
        if let Some(message) = relative_texture(&opened.content) {
            // Nothing will draw it. As `close`, without starting a process
            // for it.
            if let Some(process) = self.processes.get_mut(plugin) {
                let close = json!({ "type": "close", "model": opened.model.0 });
                let _ = process.request(Message::new(close), "closed", self.deadlines.close);
            }
            return Err(PluginError::Failed {
                plugin: plugin.to_owned(),
                message,
            });
        }
        Ok(opened)
    }
}

/// Why `content` cannot be drawn, if one of its textures is a relative
/// path: the protocol gives texture files as absolute paths. A plugin runs
/// in its own folder, so it may mean one relative to that, but Celesta
/// would open it against its own working directory (`/` for an app started
/// from the Finder), finding nothing or another file of the same name.
fn relative_texture(content: &ContentModel) -> Option<String> {
    let sources: Vec<&TextureSource> = match content {
        ContentModel::Meshes(model) => model.textures.iter().collect(),
        ContentModel::Scene(model) => model
            .textures
            .iter()
            .map(|texture| &texture.source)
            .collect(),
    };
    sources
        .into_iter()
        .enumerate()
        .find_map(|(index, source)| match source {
            // Quoted as written: `{:?}` would double a Windows path's `\`.
            TextureSource::Path(path) if !Path::new(path).is_absolute() => Some(format!(
                "texture {index}'s path \"{}\" is relative; a plugin gives texture files as absolute paths",
                path.display()
            )),
            _ => None,
        })
}

impl ContentProvider for PluginHost {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        let opened = self.retry(plugin, |host| host.open_remote(plugin, assets))?;
        self.next += 1;
        let id = ModelId(self.next);
        self.models.insert(
            id,
            OpenModel {
                plugin: plugin.to_owned(),
                assets: assets.clone(),
                kind: opened.content.kind(),
                remote: Some(opened.model),
                refused: None,
            },
        );
        Ok(Opened {
            model: id,
            ..opened
        })
    }

    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &Value,
    ) -> Result<ContentFrame, PluginError> {
        let open = self
            .models
            .get(&model)
            .ok_or(PluginError::UnknownModel(model))?;
        if let Some(refused) = &open.refused {
            return Err(refused.clone());
        }
        let (plugin, kind) = (open.plugin.clone(), open.kind);
        let answer = self.retry(&plugin, |host| {
            let open = &host.models[&model];
            let remote = match open.remote {
                Some(remote) => remote,
                None => {
                    // The process was restarted: the model must be opened in
                    // the new one first, as the same kind. Its notices were
                    // given at the first open.
                    let assets = open.assets.clone();
                    let reopened = host.open_remote(&plugin, &assets)?;
                    if reopened.content.kind() != kind {
                        // Nothing will draw it. As `close`, without starting
                        // a process for it.
                        if let Some(process) = host.processes.get_mut(&plugin) {
                            let close = json!({ "type": "close", "model": reopened.model.0 });
                            let _ = process.request(
                                Message::new(close),
                                "closed",
                                host.deadlines.close,
                            );
                        }
                        let refused = PluginError::Failed {
                            plugin: plugin.clone(),
                            message: format!(
                                "model {} was opened as {kind:?} content but reopened as {:?}",
                                model.0,
                                reopened.content.kind()
                            ),
                        };
                        // Reopening it again would come back the same.
                        host.models.get_mut(&model).expect("open").refused = Some(refused.clone());
                        return Err(refused);
                    }
                    host.models.get_mut(&model).expect("open").remote = Some(reopened.model);
                    reopened.model
                }
            };
            host.send(
                &plugin,
                Message::new(
                    json!({ "type": "frame", "model": remote.0, "time": time, "data": data }),
                ),
                "frame",
                host.deadlines.frame,
            )
        })?;
        decode_content_frame(&answer, kind).map_err(|error| PluginError::Failed {
            plugin,
            message: error.to_string(),
        })
    }

    fn close(&mut self, model: ModelId) {
        let Some(open) = self.models.remove(&model) else {
            return;
        };
        if let (Some(remote), Some(process)) = (open.remote, self.processes.get_mut(&open.plugin)) {
            let close = json!({ "type": "close", "model": remote.0 });
            let _ = process.request(Message::new(close), "closed", self.deadlines.close);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::manifest::{MANIFEST, discover, platform};
    use celesta_plugin::MeshModel;
    use celesta_plugin::protocol::{encode_opened, write_message};
    use std::os::unix::fs::PermissionsExt;
    use std::thread;
    use std::time::Instant;

    /// A plugin, `hang`, that answers `hello` and one `open`, then hangs:
    /// it never answers `frame`. Each start adds a line to `starts`, and
    /// `started` is touched once it answered.
    struct HangingPlugin {
        root: tempfile::TempDir,
    }

    impl HangingPlugin {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let directory = root.path().join("hang");
            std::fs::create_dir_all(directory.join("bin")).unwrap();
            std::fs::write(
                directory.join(MANIFEST),
                json!({
                    "id": "hang",
                    "version": "1.0.0",
                    "protocol": 1,
                    "celesta": concat!("=", env!("CARGO_PKG_VERSION")),
                    "executable": { platform().0: "bin/p" },
                })
                .to_string(),
            )
            .unwrap();
            let mut answers = Vec::new();
            let hello = json!({
                "type": "hello", "protocol": 1, "plugin": "hang",
                "version": "1.0.0", "roles": ["content"],
            });
            write_message(&mut answers, &Message::new(hello)).unwrap();
            let opened = Opened {
                model: ModelId(1),
                content: MeshModel {
                    canvas: [2.0, 2.0],
                    textures: Vec::new(),
                    meshes: Vec::new(),
                }
                .into(),
                notices: Vec::new(),
            };
            write_message(&mut answers, &encode_opened(&opened)).unwrap();
            std::fs::write(directory.join("answers"), answers).unwrap();
            let program = directory.join("bin/p");
            std::fs::write(
                &program,
                format!(
                    "#!/bin/sh\necho $$ >> '{starts}'\ncat answers\ntouch '{started}'\nexec sleep 60\n",
                    starts = root.path().join("starts").display(),
                    started = root.path().join("started").display(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self { root }
        }

        fn host(&self) -> PluginHost {
            let (plugins, errors) = discover(&[self.root.path().to_path_buf()]);
            assert!(errors.is_empty(), "{errors:?}");
            PluginHost::new(plugins)
        }

        /// The process ids of the processes started, in order.
        fn starts(&self) -> Vec<i32> {
            std::fs::read_to_string(self.root.path().join("starts"))
                .unwrap_or_default()
                .lines()
                .map(|line| line.parse().unwrap())
                .collect()
        }

        fn wait_until_started(&self) {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !self.root.path().join("started").exists() {
                assert!(Instant::now() < deadline, "the plugin did not start");
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn running(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[test]
    fn a_frame_that_hangs_ends_at_its_deadline_without_a_second_try() {
        let plugin = HangingPlugin::new();
        let mut host = plugin.host();
        host.deadlines.frame = Duration::from_millis(300);
        let opened = host.open("hang", &BTreeMap::new()).unwrap();

        let error = host.frame(opened.model, 0.0, &Value::Null).unwrap_err();
        let PluginError::Unavailable {
            plugin: id,
            message,
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!(id, "hang");
        assert!(
            message.contains("did not answer `frame` within 0.3 s"),
            "{message}"
        );
        let starts = plugin.starts();
        assert_eq!(starts.len(), 1, "it was not started again for this frame");
        assert!(!running(starts[0]), "the plugin was stopped");
    }

    #[test]
    fn a_switch_given_later_kills_the_processes_started_before() {
        let plugin = HangingPlugin::new();
        let mut host = plugin.host();
        let opened = host.open("hang", &BTreeMap::new()).unwrap();
        plugin.wait_until_started();
        let switch = PluginKillSwitch::new();
        let mut host = host.with_kill_switch(switch.clone());

        switch.kill();
        let begun = Instant::now();
        assert!(matches!(
            host.frame(opened.model, 0.0, &Value::Null),
            Err(PluginError::Unavailable { .. })
        ));
        assert!(begun.elapsed() < Duration::from_secs(10));
        assert!(!running(plugin.starts()[0]), "the plugin was stopped");
    }

    #[test]
    fn killing_its_switch_ends_a_frame_in_progress_and_starts_nothing() {
        let plugin = HangingPlugin::new();
        let switch = PluginKillSwitch::new();
        let mut host = plugin.host().with_kill_switch(switch.clone());
        let opened = host.open("hang", &BTreeMap::new()).unwrap();
        plugin.wait_until_started();

        let killer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            switch.kill();
        });
        let begun = Instant::now();
        let error = host.frame(opened.model, 0.0, &Value::Null).unwrap_err();
        killer.join().unwrap();
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "it returned well before the frame's 30 s deadline"
        );
        let PluginError::Unavailable { message, .. } = &error else {
            panic!("{error:?}");
        };
        assert!(
            message.contains("stopped while it worked on `frame`"),
            "{message}"
        );
        let starts = plugin.starts();
        assert_eq!(starts.len(), 1);
        assert!(!running(starts[0]), "the plugin was stopped");

        // Later requests fail at once, starting no process.
        assert!(matches!(
            host.frame(opened.model, 0.0, &Value::Null),
            Err(PluginError::Unavailable { .. })
        ));
        assert!(matches!(
            host.open("hang", &BTreeMap::new()),
            Err(PluginError::Unavailable { .. })
        ));
        assert_eq!(plugin.starts().len(), 1);
    }
}

/// The refusal of relative texture paths, on every platform.
#[cfg(test)]
mod texture_tests {
    use super::*;
    use celesta_plugin::MeshModel;
    use celesta_plugin::scene::{ColorSpace, Sampler, SceneModel, SceneTexture};
    use std::sync::Arc;

    fn meshes(textures: Vec<TextureSource>) -> ContentModel {
        MeshModel {
            canvas: [1.0, 1.0],
            textures,
            meshes: Vec::new(),
        }
        .into()
    }

    fn absolute() -> PathBuf {
        std::env::temp_dir().join("skin.png")
    }

    #[test]
    fn a_relative_texture_path_is_refused_by_name() {
        let inline = TextureSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::new(vec![0; 4]),
        };
        let relative = TextureSource::Path(PathBuf::from("textures/skin.png"));
        let message =
            relative_texture(&meshes(vec![inline.clone(), relative.clone()])).expect("refused");
        assert!(message.contains("texture 1"), "{message}");
        assert!(message.contains("\"textures/skin.png\""), "{message}");
        assert!(message.contains("absolute"), "{message}");

        let scene = ContentModel::Scene(SceneModel {
            canvas: [1.0, 1.0],
            textures: vec![SceneTexture {
                source: relative,
                color_space: ColorSpace::Srgb,
                sampler: Sampler::default(),
            }],
            geometries: Vec::new(),
            skins: Vec::new(),
            shaders: Vec::new(),
            materials: Vec::new(),
            draws: Vec::new(),
        });
        let message = relative_texture(&scene).expect("refused");
        assert!(message.contains("texture 0"), "{message}");

        // A `\` is shown once, as the plugin wrote it.
        let windows = TextureSource::Path(PathBuf::from(r"textures\skin.png"));
        let message = relative_texture(&meshes(vec![windows])).expect("refused");
        assert!(message.contains(r#""textures\skin.png""#), "{message}");

        let fine = meshes(vec![inline, TextureSource::Path(absolute())]);
        assert_eq!(relative_texture(&fine), None);
    }

    /// A plugin of a shell script that answers `hello`, then `opened` with a
    /// model whose texture is relative, then `closed`, recording what it
    /// is sent: the host refuses the model and closes it in the plugin,
    /// without opening it again.
    #[cfg(unix)]
    #[test]
    fn a_refused_model_is_closed_in_the_plugin() {
        use celesta_plugin::protocol::{encode_opened, read_message, write_message};
        use std::os::unix::fs::PermissionsExt;
        use std::time::Instant;

        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("p");
        std::fs::create_dir(&directory).unwrap();
        let mut answers = Vec::new();
        for answer in [
            Message::new(json!({
                "type": "hello",
                "protocol": celesta_plugin::PROTOCOL_VERSION,
                "plugin": "p",
            })),
            encode_opened(&Opened {
                model: ModelId(42),
                content: meshes(vec![TextureSource::Path("skin.png".into())]),
                notices: Vec::new(),
            }),
            Message::new(json!({ "type": "closed" })),
        ] {
            write_message(&mut answers, &answer).unwrap();
        }
        std::fs::write(directory.join("answers"), answers).unwrap();
        std::fs::write(
            directory.join("plugin.sh"),
            "#!/bin/sh\ncat answers\nexec cat > requests\n",
        )
        .unwrap();
        std::fs::set_permissions(
            directory.join("plugin.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let (os, _) = crate::manifest::platform();
        std::fs::write(
            directory.join(crate::manifest::MANIFEST),
            json!({
                "id": "p", "version": "1", "protocol": 1,
                "celesta": concat!("=", env!("CARGO_PKG_VERSION")),
                "executable": { os: "plugin.sh" },
            })
            .to_string(),
        )
        .unwrap();
        let (found, errors) = crate::manifest::discover(std::slice::from_ref(&directory));
        assert!(errors.is_empty(), "{errors:?}");

        let mut host = PluginHost::new(found);
        let error = host.open("p", &BTreeMap::new()).unwrap_err();
        let PluginError::Failed { plugin, message } = error else {
            panic!("{error:?}");
        };
        assert_eq!(plugin, "p");
        assert!(
            message.contains(r#"texture 0's path "skin.png""#),
            "{message}"
        );
        assert!(host.models.is_empty());

        // The requests reach the file a little after their answers were read.
        let deadline = Instant::now() + Duration::from_secs(30);
        let kinds = loop {
            let bytes = std::fs::read(directory.join("requests")).unwrap_or_default();
            let mut reader = &bytes[..];
            let mut requests = Vec::new();
            while let Ok(Some(request)) = read_message(&mut reader) {
                requests.push(request.header);
            }
            if requests.len() >= 3 || Instant::now() > deadline {
                break requests;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let kinds: Vec<_> = kinds
            .iter()
            .map(|header| (header["type"].clone(), header["model"].clone()))
            .collect();
        assert_eq!(
            kinds,
            [
                (json!("hello"), Value::Null),
                (json!("open"), Value::Null),
                (json!("close"), json!(42)),
            ]
        );
    }
}

#[cfg(all(test, unix))]
mod failure_tests {
    use super::*;
    use crate::locations::PluginSource;
    use crate::manifest::{Manifest, PluginRole, platform};
    use celesta_plugin::protocol::{PROTOCOL_VERSION, encode_opened, write_message};
    use celesta_plugin::{ContentModel, MeshModel, SceneModel};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// Plugin `p`, a shell script running `body` in `directory`, after it
    /// adds a line to `starts` there.
    fn plugin(directory: &Path, body: &str) -> PluginEntry {
        let path = directory.join("plugin.sh");
        std::fs::write(&path, format!("#!/bin/sh\necho >> starts\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        PluginEntry {
            manifest: Manifest {
                id: "p".into(),
                version: "1".into(),
                protocol: PROTOCOL_VERSION,
                celesta: None,
                executable: [(platform().0.to_owned(), "plugin.sh".to_owned())].into(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![PluginRole::Content],
            },
            directory: directory.to_owned(),
            source: PluginSource::Explicit,
        }
    }

    /// How many times the plugin in `directory` was started.
    fn starts(directory: &Path) -> usize {
        std::fs::read_to_string(directory.join("starts"))
            .map(|starts| starts.lines().count())
            .unwrap_or(0)
    }

    /// Writes `messages` to `name` in `directory`, as a plugin answers.
    fn answers(directory: &Path, name: &str, messages: &[Message]) {
        let mut bytes = Vec::new();
        for message in messages {
            write_message(&mut bytes, message).unwrap();
        }
        std::fs::write(directory.join(name), bytes).unwrap();
    }

    fn hello(plugin: &str) -> Message {
        Message::new(json!({ "type": "hello", "protocol": PROTOCOL_VERSION, "plugin": plugin }))
    }

    fn opened(content: ContentModel) -> Message {
        encode_opened(&Opened {
            model: ModelId(1),
            content,
            notices: Vec::new(),
        })
    }

    fn meshes() -> ContentModel {
        MeshModel {
            canvas: [1.0, 1.0],
            textures: Vec::new(),
            meshes: Vec::new(),
        }
        .into()
    }

    fn scene() -> ContentModel {
        SceneModel {
            canvas: [1.0, 1.0],
            textures: Vec::new(),
            geometries: Vec::new(),
            skins: Vec::new(),
            shaders: Vec::new(),
            materials: Vec::new(),
            draws: Vec::new(),
        }
        .into()
    }

    #[test]
    fn a_plugin_that_cannot_start_is_not_started_again_until_stopped() {
        let directory = tempfile::tempdir().unwrap();
        answers(directory.path(), "hello", &[hello("someone-else")]);
        let entry = plugin(directory.path(), "cat hello\nexec cat >/dev/null");
        let mut host = PluginHost::new([entry]);
        for _ in 0..2 {
            let error = host.open("p", &BTreeMap::new()).unwrap_err();
            let PluginError::Unavailable { message, .. } = error else {
                panic!("{error}");
            };
            assert!(message.contains("says it is plugin"), "{message}");
        }
        assert_eq!(starts(directory.path()), 1);

        // Stopping it, as reloading plugins does, tries it again.
        host.stop("p");
        host.open("p", &BTreeMap::new()).unwrap_err();
        assert_eq!(starts(directory.path()), 2);
    }

    #[test]
    fn a_plugin_that_exits_in_open_is_started_again_once() {
        let directory = tempfile::tempdir().unwrap();
        answers(directory.path(), "hello", &[hello("p")]);
        let entry = plugin(directory.path(), "cat hello");
        let mut host = PluginHost::new([entry]);
        let error = host.open("p", &BTreeMap::new()).unwrap_err();
        assert!(matches!(error, PluginError::Unavailable { .. }), "{error}");
        assert_eq!(starts(directory.path()), 2);
    }

    #[test]
    fn a_model_reopened_as_another_kind_is_not_reopened_again() {
        let directory = tempfile::tempdir().unwrap();
        // The first process opens meshes and exits on the frame; the next
        // reopens them as a scene, and would refuse a further reopen with
        // an error of its own.
        answers(directory.path(), "first", &[hello("p"), opened(meshes())]);
        answers(
            directory.path(),
            "next",
            &[
                hello("p"),
                opened(scene()),
                Message::new(json!({ "type": "closed" })),
                celesta_plugin::protocol::error_message("opened again"),
            ],
        );
        let entry = plugin(
            directory.path(),
            r#"if [ "$(wc -l < starts)" -eq 1 ]; then cat first; exit 0; fi
cat next
exec cat >/dev/null"#,
        );
        let mut host = PluginHost::new([entry]);
        let model = host.open("p", &BTreeMap::new()).unwrap().model;
        let reopened = host.frame(model, 0.0, &Value::Null).unwrap_err();
        assert!(
            reopened
                .to_string()
                .contains("opened as Meshes content but reopened as Scene"),
            "{reopened}"
        );
        let again = host.frame(model, 0.0, &Value::Null).unwrap_err();
        assert_eq!(again.to_string(), reopened.to_string());
        assert_eq!(starts(directory.path()), 2);
    }
}

/// A plugin that falls out of step, and models no plugin has.
#[cfg(all(test, unix))]
mod out_of_step {
    use super::*;
    use crate::locations::PluginSource;
    use crate::manifest::{Manifest, PluginRole, platform};
    use celesta_plugin::protocol::{encode_content_frame, encode_opened, write_message};
    use celesta_plugin::{MeshFrame, MeshModel, PROTOCOL_VERSION};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    const ID: &str = "doubled";

    /// The bytes of `answers`, as a plugin writes them.
    fn answers(answers: &[Message]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for answer in answers {
            write_message(&mut bytes, answer).unwrap();
        }
        bytes
    }

    /// A plugin whose first process answers `open` twice, and whose later
    /// processes answer as they should. Each start is logged in `runs`.
    fn doubling_plugin(directory: &Path) -> PluginEntry {
        let hello = Message::new(json!({
            "type": "hello", "protocol": PROTOCOL_VERSION, "plugin": ID,
            "version": "1", "roles": ["content"],
        }));
        let opened = encode_opened(&Opened {
            model: ModelId(1),
            content: MeshModel {
                canvas: [1.0, 1.0],
                textures: Vec::new(),
                meshes: Vec::new(),
            }
            .into(),
            notices: Vec::new(),
        });
        let frame = encode_content_frame(&MeshFrame { meshes: Vec::new() }.into());
        std::fs::write(
            directory.join("first.bin"),
            answers(&[hello.clone(), opened.clone(), opened.clone()]),
        )
        .unwrap();
        std::fs::write(
            directory.join("later.bin"),
            answers(&[hello, opened, frame]),
        )
        .unwrap();
        let script = directory.join("plugin.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\n\
             if [ -e runs ]; then answers=later.bin; else answers=first.bin; fi\n\
             echo run >> runs\n\
             cat \"$answers\"\n\
             exec cat >/dev/null\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        PluginEntry {
            manifest: Manifest {
                id: ID.into(),
                version: "1".into(),
                protocol: PROTOCOL_VERSION,
                celesta: Some("*".into()),
                executable: BTreeMap::from([(platform().0.to_owned(), "plugin.sh".into())]),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![PluginRole::Content],
            },
            directory: directory.to_path_buf(),
            source: PluginSource::Explicit,
        }
    }

    #[test]
    fn a_plugin_that_answers_twice_is_restarted_back_in_step() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = PluginHost::new([doubling_plugin(directory.path())]);
        let opened = host.open(ID, &BTreeMap::new()).unwrap();
        // The frame request reads the second `opened`: the process is
        // stopped and started again, and the model reopened in it.
        let frame = host.frame(opened.model, 0.0, &Value::Null).unwrap();
        assert_eq!(frame, MeshFrame { meshes: Vec::new() }.into());
        let runs = std::fs::read_to_string(directory.path().join("runs")).unwrap();
        assert_eq!(runs.lines().count(), 2, "{runs}");
    }

    #[test]
    fn an_unknown_model_names_no_plugin() {
        let mut host = PluginHost::new([]);
        let error = host.frame(ModelId(7), 0.0, &Value::Null).unwrap_err();
        assert!(matches!(error, PluginError::UnknownModel(ModelId(7))));
        assert_eq!(error.to_string(), "no open model 7");
    }
}
