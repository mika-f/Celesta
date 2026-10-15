//! Starts the built `celesta-mcp` and talks to it as an MCP client.

use std::path::{Path, PathBuf};

use rmcp::model::{CallToolRequestParams, CallToolResult, ProgressNotificationParam};
use rmcp::service::{NotificationContext, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use serde_json::Value;
use tokio::sync::mpsc;

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether the React runtime the server spawns has been built.
pub fn runtime_built() -> bool {
    let built = root().join("packages/cli/dist/cli.js").exists();
    if !built {
        eprintln!("skipping: build packages/cli first (pnpm run build:runtime)");
    }
    built
}

/// An entry in a directory where `@celesta/react` resolves.
pub fn entry(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = dir.path().join("scene.tsx");
    std::fs::write(&entry, source).unwrap();
    (dir, entry)
}

/// Forwards progress notifications to the test.
pub struct TestClient {
    progress: mpsc::UnboundedSender<ProgressNotificationParam>,
}

impl ClientHandler for TestClient {
    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let _ = self.progress.send(params);
    }
}

pub type Client = RunningService<RoleClient, TestClient>;

pub async fn client() -> (Client, mpsc::UnboundedReceiver<ProgressNotificationParam>) {
    let (progress, received) = mpsc::unbounded_channel();
    let command = tokio::process::Command::new(env!("CARGO_BIN_EXE_celesta-mcp"));
    let transport = TokioChildProcess::new(command).unwrap();
    let client = TestClient { progress }.serve(transport).await.unwrap();
    (client, received)
}

pub fn params(name: &'static str, args: Value) -> CallToolRequestParams {
    let Value::Object(args) = args else {
        panic!("tool arguments must be an object")
    };
    CallToolRequestParams::new(name).with_arguments(args)
}

pub async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    client.call_tool(params(name, args)).await.unwrap()
}

/// All text content of a result, joined by newlines.
pub fn text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn is_error(result: &CallToolResult) -> bool {
    result.is_error == Some(true)
}

pub fn structured(result: &CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("structured content")
}
