use crate::problems::error_chain;
use crate::worker::LatestWorker;
use celesta_react_bridge::{ComponentPropertyField, ComponentPropertySchema, ReactBridge};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) struct ComponentSchemaRequest {
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
}

pub(crate) struct ComponentSchemaResult {
    pub(crate) schemas: Result<BTreeMap<String, ComponentPropertySchema>, String>,
    pub(crate) project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
}

/// Queries a `.tsx` entry's registered `registerComponent()` schemas by
/// spawning the same `@celesta/cli` Node.js runtime `celesta-exporter --react`
/// uses, reading them off `ReactBridge::metadata` (populated during the
/// startup handshake, before any frame is requested), then dropping the
/// process. `ReactBridge::spawn` blocks on the child's first stdout line, so
/// this runs off the GPUI thread.
pub(crate) type ComponentSchemaWorker = LatestWorker<ComponentSchemaRequest, ComponentSchemaResult>;

pub(crate) fn spawn_component_schema() -> anyhow::Result<ComponentSchemaWorker> {
    LatestWorker::spawn("component schema", |request: ComponentSchemaRequest, _| {
        let result = ReactBridge::spawn(&request.node, &request.cli_script, &request.entry);
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
            .map_err(|error| error_chain(&error));
        let project_property_schema = result
            .ok()
            .and_then(|bridge| bridge.metadata().project_property_schema.clone());
        ComponentSchemaResult {
            schemas,
            project_property_schema,
        }
    })
}
