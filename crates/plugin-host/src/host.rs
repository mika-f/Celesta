//! Plugin processes, one per plugin a renderer uses, behind the renderer's
//! `ContentProvider` interface.

use crate::manifest::{InstalledPlugins, PluginEntry};
use celesta_plugin::protocol::{
    ContentKind, Message, PROTOCOL_VERSION, ProtocolError, decode_content_frame, decode_opened,
    read_message, write_message,
};
use celesta_plugin::{ContentFrame, ContentProvider, ModelId, Opened, PluginError};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// A running plugin process.
struct PluginProcess {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

/// How a request went wrong: the process cannot be talked to, or it
/// refused.
enum RequestError {
    Unavailable(String),
    Refused(String),
}

impl PluginProcess {
    fn start(entry: &PluginEntry) -> Result<Self, String> {
        let executable = entry.executable().ok_or_else(|| {
            format!(
                "{} has no executable for this platform",
                entry.directory.display()
            )
        })?;
        let mut child = Command::new(&executable)
            .current_dir(&entry.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The plugin's own log goes where Celesta's does.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", executable.display()))?;
        let input = BufWriter::new(child.stdin.take().expect("piped"));
        let output = BufReader::new(child.stdout.take().expect("piped"));
        let mut process = Self {
            child,
            input,
            output,
        };
        let hello = process
            .request(&Message::new(
                json!({ "type": "hello", "protocol": PROTOCOL_VERSION }),
            ))
            .map_err(|error| match error {
                RequestError::Unavailable(message) | RequestError::Refused(message) => message,
            })?;
        if hello.header["plugin"] != entry.manifest.id.as_str() {
            return Err(format!(
                "{} says it is plugin {}, not {}",
                executable.display(),
                hello.header["plugin"],
                entry.manifest.id
            ));
        }
        Ok(process)
    }

    fn request(&mut self, request: &Message) -> Result<Message, RequestError> {
        let io = |error: std::io::Error| RequestError::Unavailable(error.to_string());
        write_message(&mut self.input, request).map_err(io)?;
        let answer = match read_message(&mut self.output) {
            Ok(Some(answer)) => answer,
            Ok(None) => {
                return Err(RequestError::Unavailable(
                    "the plugin process exited".into(),
                ));
            }
            Err(ProtocolError::Io(error)) => return Err(io(error)),
            Err(error) => return Err(RequestError::Unavailable(error.to_string())),
        };
        if answer.kind() == "error" {
            return Err(RequestError::Refused(
                answer.header["message"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_owned(),
            ));
        }
        Ok(answer)
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        // Closing stdin asks the plugin to exit; it is killed if it has not.
        let _ = self.child.kill();
        let _ = self.child.wait();
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
/// first use. A process that stops answering is started again once per
/// request, reopening the models it had.
pub struct PluginHost {
    plugins: BTreeMap<String, PluginEntry>,
    processes: HashMap<String, PluginProcess>,
    models: HashMap<ModelId, OpenModel>,
    next: u64,
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
        }
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
            let process =
                PluginProcess::start(entry).map_err(|message| PluginError::Unavailable {
                    plugin: plugin.to_owned(),
                    message,
                })?;
            self.processes.insert(plugin.to_owned(), process);
        }
        Ok(self.processes.get_mut(plugin).expect("started above"))
    }

    /// Runs `attempt` against `plugin`'s process; once more on a fresh
    /// process if the first could not be talked to.
    fn retry<T>(
        &mut self,
        plugin: &str,
        mut attempt: impl FnMut(&mut Self) -> Result<T, PluginError>,
    ) -> Result<T, PluginError> {
        match attempt(self) {
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

    fn send(&mut self, plugin: &str, message: &Message) -> Result<Message, PluginError> {
        self.process(plugin)?
            .request(message)
            .map_err(|error| match error {
                RequestError::Unavailable(message) => PluginError::Unavailable {
                    plugin: plugin.to_owned(),
                    message,
                },
                RequestError::Refused(message) => PluginError::Failed {
                    plugin: plugin.to_owned(),
                    message,
                },
            })
    }

    fn open_remote(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        let request = Message::new(json!({ "type": "open", "assets": assets }));
        let answer = self.send(plugin, &request)?;
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
                        let close = json!({ "type": "close", "model": reopened.model.0 });
                        let _ = host.send(&plugin, &Message::new(close));
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
                &Message::new(
                    json!({ "type": "frame", "model": remote.0, "time": time, "data": data }),
                ),
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
            let _ = process.request(&Message::new(json!({ "type": "close", "model": remote.0 })));
        }
    }
}
