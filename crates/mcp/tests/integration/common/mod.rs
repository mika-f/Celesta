//! Starts the built `celesta-mcp` and talks to it as an MCP client.

use std::path::{Path, PathBuf};

use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};
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

/// An entry in a directory where `@celesta.studio/react` resolves.
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

fn server() -> tokio::process::Command {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_celesta-mcp"))
}

async fn serve(
    command: tokio::process::Command,
) -> (Client, mpsc::UnboundedReceiver<ProgressNotificationParam>) {
    let (progress, received) = mpsc::unbounded_channel();
    let transport = TokioChildProcess::new(command).unwrap();
    let client = TestClient { progress }.serve(transport).await.unwrap();
    (client, received)
}

pub async fn client() -> (Client, mpsc::UnboundedReceiver<ProgressNotificationParam>) {
    serve(server()).await
}

/// A client whose server reads and writes media with the test codec
/// `plugin`, whatever codec the developer's `CELESTA_PLUGIN_PATH` names.
/// Bind `plugin` before the client, so that it drops after the client on
/// every path, a panic included, and outlives the server.
pub async fn client_with_test_codec(
    plugin: &TestCodecPlugin,
) -> (Client, mpsc::UnboundedReceiver<ProgressNotificationParam>) {
    let mut command = server();
    // Replaces the developer's value rather than adding to it.
    command.env(PLUGIN_PATH_ENV, plugin.folder());
    serve(command).await
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
