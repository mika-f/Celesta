//! `--inspect`: evaluates a React entry's frames on request through the same
//! bridge an export uses — native text shaping and media probing included —
//! without a GPU or rendering pixels. `skills/celesta/scripts/inspect.mjs
//! --native` drives it.
//!
//! The first stdout line is the handshake (the CLI's own `config`,
//! `componentSchemas` and `propertySchema`), or `error`/`invalidProperties`
//! when the entry does not load. Each stdin line `{"time":{value,timescale}}`
//! is answered with one line: `{scene, audio}` or `{error, status, fatal}`,
//! where `status` is `requestError`, `sceneError` or `runtimeError`. A
//! runtime error is fatal and ends inspection. A request with `"openEnd":
//! true` evaluates a frame past the end as if the video went on (`inspect.mjs
//! --loop`), and its answer echoes `"openEnd": true`.
//!
//! Like the CLI's own messages, the handshake and every answer carry
//! `"nondeterministic": [...]`, the clock and random APIs the entry was
//! first seen calling while rendering, when there are new ones.

use std::io::{self, BufRead, Write};
use std::path::Path;

use celesta_composition::Time;
use celesta_exporter::ReactRuntimeOptions;
use celesta_plugin_host::InstalledPlugins;
use celesta_react_bridge::{ReactBridge, ReactBridgeError, SpawnOptions};
use serde_json::{Value, json};

/// Adds to `message` the APIs `bridge` reports past the first `*sent`.
fn with_new_calls(mut message: Value, bridge: &ReactBridge, sent: &mut Vec<String>) -> Value {
    let new: Vec<&String> = bridge
        .nondeterministic_calls()
        .apis()
        .iter()
        .filter(|api| !sent.contains(api))
        .collect();
    if !new.is_empty() {
        message["nondeterministic"] = json!(new);
        sent.extend(new.into_iter().cloned());
    }
    message
}

#[derive(serde::Deserialize)]
struct InspectRequest {
    time: Time,
    #[serde(default, rename = "openEnd")]
    open_end: bool,
}

/// Runs the inspection protocol until stdin closes, resolving
/// `@celesta-plugin/<id>` among `plugins`; `false` if anything failed.
pub fn run(
    entry: &Path,
    runtime: &ReactRuntimeOptions,
    plugins: &InstalledPlugins,
) -> Result<bool, io::Error> {
    let spawned = ReactBridge::spawn_with_options(
        &runtime.node,
        &runtime.cli_script,
        entry,
        &runtime.properties,
        SpawnOptions {
            composition_id: runtime.composition_id.clone(),
            plugins: Some(plugins.runtime()),
            ..SpawnOptions::default()
        },
    );
    let mut bridge = match spawned {
        Ok(bridge) => bridge,
        Err(ReactBridgeError::InvalidProperties(issues)) => {
            let issues: Vec<Value> = issues
                .iter()
                .map(|issue| json!({ "key": issue.key, "source": issue.source, "message": issue.message }))
                .collect();
            write_line(
                &mut io::stdout().lock(),
                &json!({ "invalidProperties": issues }),
            )?;
            return Ok(false);
        }
        Err(error) => return startup_failed(&message(&error)),
    };
    let mut output = io::stdout().lock();
    let metadata = bridge.metadata();
    let mut sent = Vec::new();
    let handshake = with_new_calls(
        json!({
            "compositionId": metadata.composition_id,
            "compositions": metadata.compositions,
            "config": {
                "width": metadata.width,
                "height": metadata.height,
                "frameRate": metadata.frame_rate,
                "durationInFrames": metadata.duration_in_frames,
            },
            "componentSchemas": metadata.component_schemas,
            "propertySchema": metadata.project_property_schema,
        }),
        &bridge,
        &mut sent,
    );
    write_line(&mut output, &handshake)?;
    let mut ok = true;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<InspectRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                ok = false;
                let error = format!("invalid request JSON: {error}");
                write_line(
                    &mut output,
                    &json!({ "error": error, "status": "requestError", "fatal": false }),
                )?;
                continue;
            }
        };
        let evaluated = if request.open_end {
            bridge.evaluate_past_end(request.time)
        } else {
            bridge.evaluate_at(request.time, None)
        };
        match evaluated {
            Ok(frame) => {
                let mut answer = json!({ "scene": frame.scene, "audio": frame.audio });
                if request.open_end {
                    answer["openEnd"] = json!(true);
                }
                write_line(&mut output, &with_new_calls(answer, &bridge, &mut sent))?;
            }
            Err(error) => {
                ok = false;
                // A scene error leaves the runtime ready for the next frame,
                // as does a runtime too old for `openEnd`; anything else
                // means it is gone or out of step.
                let status = match error {
                    ReactBridgeError::Render(_) | ReactBridgeError::Time(_) => "sceneError",
                    ReactBridgeError::OpenEndUnsupported => "requestError",
                    _ => "runtimeError",
                };
                let fatal = status == "runtimeError";
                let answer = json!({
                    "error": message(&error),
                    "status": status,
                    "fatal": fatal,
                });
                write_line(&mut output, &with_new_calls(answer, &bridge, &mut sent))?;
                if fatal {
                    break;
                }
            }
        }
    }
    Ok(ok)
}

/// Reports, in place of the handshake, that the entry could not be started.
pub fn startup_failed(error: &str) -> Result<bool, io::Error> {
    write_line(&mut io::stdout().lock(), &json!({ "error": error }))?;
    Ok(false)
}

/// The entry's own message for errors it raised, as Node-only inspection
/// reports them; the bridge's description otherwise.
fn message(error: &ReactBridgeError) -> String {
    match error {
        ReactBridgeError::EntryFailed(message) | ReactBridgeError::Render(message) => {
            message.clone()
        }
        error => error.to_string(),
    }
}

fn write_line(output: &mut impl Write, message: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, message)?;
    writeln!(output)?;
    output.flush()
}
