//! The MCP tool router: argument types, validation, and dispatch.

use std::path::PathBuf;
use std::sync::Arc;

use celesta_exporter::report;
use celesta_react_bridge::{
    TemplateError, initialize_project, list_templates, project_templates_dir, runtime_paths,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, tool, tool_handler, tool_router};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::args::{ContactSheetArgs, ExportVideoArgs, RenderFramesArgs};
use crate::docs;
use crate::export::{self, Run};
use crate::frames::{OutputDirs, image_blocks};
use crate::inspect::{self, InspectArgs};

#[derive(Clone)]
pub struct CelestaServer {
    tool_router: ToolRouter<Self>,
    /// Held by every tool that renders, so they take turns on the GPU.
    render_lock: Arc<Mutex<()>>,
    outputs: Arc<OutputDirs>,
}

impl CelestaServer {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            tool_router: Self::tool_router(),
            render_lock: Arc::new(Mutex::new(())),
            outputs: Arc::new(OutputDirs::new()?),
        })
    }
}

/// A rendering tool's result: the PNGs it wrote as images, then its report.
fn rendered(run: Run) -> CallToolResult {
    if !run.ok {
        return CallToolResult::structured_error(run.report);
    }
    let mut content = match image_blocks(&run.files) {
        Ok(images) => images,
        Err(error) => return error_result("io", format!("could not read a rendered PNG: {error}")),
    };
    content.push(ContentBlock::text(run.report.to_string()));
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(run.report);
    result
}

/// An error result shaped like an export report's.
pub fn error_result(code: &str, message: impl Into<String>) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "status": "error",
        "error": { "code": code, "message": message.into() },
    }))
}

/// Arguments rejected before anything ran.
pub fn usage_error(message: &str) -> CallToolResult {
    CallToolResult::structured_error(
        serde_json::to_value(report::usage(message)).expect("reports serialize"),
    )
}

/// A successful result: `text` for reading, `value` as structured content.
pub fn success(text: String, value: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    result
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadDocsArgs {
    /// The topic to read; leave it out for the list of topics.
    #[serde(default)]
    #[schemars(schema_with = "topic_schema")]
    topic: Option<String>,
}

fn topic_schema(_: &mut SchemaGenerator) -> Schema {
    let names: Vec<&str> = docs::TOPICS.iter().map(|topic| topic.name).collect();
    Schema::try_from(json!({ "type": "string", "enum": names })).expect("an object schema")
}

#[derive(Deserialize, JsonSchema)]
pub struct InitProjectArgs {
    /// The folder to create the project in; created when missing.
    folder: PathBuf,
    /// A template name from list_templates; `blank` when left out.
    template: Option<String>,
}

#[tool_router]
impl CelestaServer {
    #[tool(
        description = "Read Celesta's documentation as Markdown: how to write React compositions and .celesta.json projects, animation, text, media, dialogue, loops, export options, and error codes. Call without a topic to list the topics."
    )]
    fn read_docs(&self, Parameters(args): Parameters<ReadDocsArgs>) -> CallToolResult {
        match docs::read(args.topic.as_deref()) {
            Ok(markdown) => CallToolResult::success(vec![ContentBlock::text(markdown)]),
            Err(message) => usage_error(&message),
        }
    }

    #[tool(description = "List the project templates init_project can start from.")]
    fn list_templates(&self) -> CallToolResult {
        let (_, cli_script) = runtime_paths();
        match list_templates(&project_templates_dir(&cli_script)) {
            Ok(templates) => {
                let text = templates
                    .iter()
                    .map(|t| format!("- {} ({}): {}", t.name, t.title, t.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                let value = json!({
                    "templates": templates
                        .iter()
                        .map(|t| json!({ "name": t.name, "title": t.title, "description": t.description }))
                        .collect::<Vec<_>>(),
                });
                success(text, value)
            }
            Err(error) => template_error(&error),
        }
    }

    #[tool(
        description = "Create a React project (film.tsx, package.json, TypeScript setup) in a folder, like `Celesta --init`. With a template, also its props.json and data files; existing files are never overwritten."
    )]
    fn init_project(&self, Parameters(args): Parameters<InitProjectArgs>) -> CallToolResult {
        let folder = std::path::absolute(&args.folder).unwrap_or(args.folder);
        if let Err(error) = std::fs::create_dir_all(&folder) {
            return error_result(
                "io",
                format!("could not create {}: {error}", folder.display()),
            );
        }
        let (_, cli_script) = runtime_paths();
        match initialize_project(&cli_script, &folder, args.template.as_deref()) {
            Ok(setup) => {
                let template = setup.template;
                let text = format!(
                    "Created a {} project in {}.\n\n{}",
                    template.name,
                    folder.display(),
                    template.edit
                );
                let value = json!({
                    "folder": folder,
                    "template": {
                        "name": template.name,
                        "title": template.title,
                        "description": template.description,
                    },
                    "edit": template.edit,
                });
                success(text, value)
            }
            Err(error) => template_error(&error),
        }
    }

    #[tool(
        description = "Evaluate a React entry's frames without rendering pixels, with real text shaping and media probing, and list every layer (type, position, size, paint, text), audio clip, and missing local file. With `loop`, check that the last frame runs seamlessly into the first. Use it after every edit, before render_frames."
    )]
    async fn inspect(&self, Parameters(args): Parameters<InspectArgs>) -> CallToolResult {
        blocking(move || inspect::inspect(args)).await
    }

    #[tool(
        description = "Render frames of a React entry or JSON project to PNG and return them as images (the first 6; the report lists every file). Look at them before saying a change looks right. Also validates a whole .celesta.json."
    )]
    async fn render_frames(
        &self,
        Parameters(args): Parameters<RenderFramesArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        match args.job(&self.outputs) {
            Ok(job) => rendered(export::run(job, &self.render_lock, &context).await),
            Err(message) => usage_error(&message),
        }
    }

    #[tool(
        description = "Render the selected frames as one labelled grid image, to see a whole video at once (for example every 150 frames). Returns the image and the tile frames."
    )]
    async fn contact_sheet(
        &self,
        Parameters(args): Parameters<ContactSheetArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        match args.job(&self.outputs) {
            Ok(job) => rendered(export::run(job, &self.render_lock, &context).await),
            Err(message) => usage_error(&message),
        }
    }

    #[tool(
        description = "Export a React entry or JSON project as an H.264/AAC MP4, a ProRes 4444 MOV or a VP9 WebM (both transparent where nothing is drawn), all of it or the start/end span, optionally with SRT/WebVTT subtitle files timed like it (one per language with {lang} in the path); without output it writes only the subtitle files, rendering nothing. Sends progress notifications when the request has a progress token, and stops when the request is cancelled. Use it for the final file or to check audio; check pictures with render_frames."
    )]
    async fn export_video(
        &self,
        Parameters(args): Parameters<ExportVideoArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        match args.job() {
            Ok(job) => rendered(export::run(job, &self.render_lock, &context).await),
            Err(message) => usage_error(&message),
        }
    }
}

/// Runs `work` off the async runtime.
async fn blocking(work: impl FnOnce() -> CallToolResult + Send + 'static) -> CallToolResult {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|error| error_result("internal", format!("the tool failed: {error}")))
}

fn template_error(error: &TemplateError) -> CallToolResult {
    let code = match error {
        TemplateError::Unknown { .. } => "unknown_template",
        TemplateError::Conflicts { .. } => "output_exists",
        TemplateError::Missing { .. } | TemplateError::Io(_) => "io",
    };
    error_result(code, error.to_string())
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CelestaServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("celesta", env!("CARGO_PKG_VERSION")))
            .with_instructions(include_str!("instructions.md"))
    }
}
