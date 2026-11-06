//! Plugin processes, one per plugin a renderer uses, behind the renderer's
//! `ContentProvider` interface.

use crate::child::{PluginChild, RequestError};
use crate::kill::PluginKillSwitch;
use crate::manifest::{InstalledPlugins, PluginEntry};
use celesta_plugin::protocol::{ContentKind, Message, decode_content_frame, decode_opened};
use celesta_plugin::{ContentFrame, ContentProvider, ModelId, Opened, PluginError};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
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
}

/// Runs each plugin a renderer asks for in a process of its own, started on
/// first use. A process that went away is started again once per request,
/// reopening the models it had; one that missed its deadline is started
/// again on the next request instead, so a hang costs one deadline. Every
/// process is registered with the host's [`PluginKillSwitch`]: once that is
/// killed, every request fails as unavailable and no process starts.
pub struct PluginHost {
    plugins: BTreeMap<String, PluginEntry>,
    processes: HashMap<String, PluginProcess>,
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
        InstalledPlugins::find_reporting(extra).host()
    }

    pub fn plugins(&self) -> impl Iterator<Item = &PluginEntry> {
        self.plugins.values()
    }

    fn process(&mut self, plugin: &str) -> Result<&mut PluginProcess, PluginError> {
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
            let process = PluginProcess::start(entry, &self.kill)
                .map_err(|error| self.unavailable(plugin, error))?;
            self.processes.insert(plugin.to_owned(), process);
        }
        Ok(self.processes.get_mut(plugin).expect("started above"))
    }

    /// Runs `attempt` against `plugin`'s process; once more on a fresh
    /// process if the first went away (but not if it was stopped).
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
            Err(PluginError::Unavailable { .. }) => {
                self.restart(plugin);
                attempt(self)
            }
            result => result,
        }
    }

    /// Stops `plugin`'s process, as when it should reload; it starts again,
    /// reopening its models, when next needed.
    pub fn stop(&mut self, plugin: &str) {
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
        decode_opened(&answer).map_err(|error| PluginError::Failed {
            plugin: plugin.to_owned(),
            message: error.to_string(),
        })
    }
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
        let (plugin, kind) = self
            .models
            .get(&model)
            .map(|open| (open.plugin.clone(), open.kind))
            .ok_or_else(|| PluginError::Failed {
                plugin: String::new(),
                message: format!("no open model {}", model.0),
            })?;
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
                        // Nothing will draw it: the next frame reopens it.
                        // As `close`, without starting a process for it.
                        if let Some(process) = host.processes.get_mut(&plugin) {
                            let close = json!({ "type": "close", "model": reopened.model.0 });
                            let _ = process.request(
                                Message::new(close),
                                "closed",
                                host.deadlines.close,
                            );
                        }
                        return Err(PluginError::Failed {
                            plugin: plugin.clone(),
                            message: format!(
                                "model {} was opened as {kind:?} content but reopened as {:?}",
                                model.0,
                                reopened.content.kind()
                            ),
                        });
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
