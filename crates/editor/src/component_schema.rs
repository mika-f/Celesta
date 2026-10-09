use crate::audio::take_latest;
use celesta_react_bridge::{ComponentPropertyField, ComponentPropertySchema, ReactBridge};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

pub(crate) struct ComponentSchemaRequest {
    pub(crate) generation: u64,
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
}

pub(crate) struct ComponentSchemaResult {
    pub(crate) generation: u64,
    pub(crate) schemas: Result<BTreeMap<String, ComponentPropertySchema>, String>,
    pub(crate) project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
}

/// Queries a `.tsx` entry's registered `registerComponent()` schemas by
/// spawning the same `@celesta/cli` Node.js runtime `celesta-exporter --react`
/// uses, reading them off `ReactBridge::metadata` (populated during the
/// startup handshake, before any frame is requested), then dropping the
/// process — the editor's own preview never renders React content, so
/// nothing else needs this connection to stay open. `ReactBridge::spawn` is
/// a blocking call (it blocks on the child process's first stdout line), so
/// this runs on its own thread like the other editor workers rather than on
/// the GPUI render thread.
pub(crate) struct ComponentSchemaWorker {
    pub(crate) requests: mpsc::Sender<ComponentSchemaRequest>,
    pub(crate) results: mpsc::Receiver<ComponentSchemaResult>,
}

impl ComponentSchemaWorker {
    pub(crate) fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<ComponentSchemaRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ComponentSchemaResult>();
        thread::Builder::new()
            .name("celesta-component-schema".to_owned())
            .spawn(move || {
                while let Ok(first) = request_rx.recv() {
                    let request = take_latest(first, &request_rx);
                    let result =
                        ReactBridge::spawn(&request.node, &request.cli_script, &request.entry);
                    let schemas = result
                        .as_ref()
                        .map(|bridge| {
                            bridge
                                .metadata()
                                .component_schemas
                                .iter()
                                .map(|(name, schema)| (name.clone(), schema.clone()))
                                .collect()
                        })
                        .map_err(|error| error.to_string());
                    let project_property_schema = result
                        .ok()
                        .and_then(|bridge| bridge.metadata().project_property_schema.clone());
                    if result_tx
                        .send(ComponentSchemaResult {
                            generation: request.generation,
                            schemas,
                            project_property_schema,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    pub(crate) fn request(&self, request: ComponentSchemaRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "component schema worker stopped unexpectedly".to_owned())
    }
}
