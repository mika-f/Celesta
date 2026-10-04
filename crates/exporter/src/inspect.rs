//! Scene inspection uses the same bridge as export, including native text
//! shaping during prepare() and frame evaluation, without allocating a GPU.

use std::io::{self, BufRead, Write};
use std::path::Path;

use celesta_composition::Time;
use celesta_react_bridge::{ReactBridge, ReactBridgeError};
use serde_json::{Value, json};

#[derive(serde::Deserialize)]
struct InspectRequest {
    time: Time,
}

pub fn run(entry: &Path, node: &Path, runtime: &Path) -> Result<(), String> {
    let mut output = io::stdout().lock();
    let mut bridge = match ReactBridge::spawn(node, runtime, entry) {
        Ok(bridge) => bridge,
        Err(error) => {
            write_message(&mut output, &json!({ "error": error.to_string() }))?;
            return Err(error.to_string());
        }
    };
    let metadata = bridge.metadata();
    write_message(
        &mut output,
        &json!({
            "config": {
                "width": metadata.width,
                "height": metadata.height,
                "frameRate": metadata.frame_rate,
                "durationInFrames": metadata.duration_in_frames,
            },
            "componentSchemas": metadata.component_schemas,
            "propertySchema": metadata.project_property_schema,
        }),
    )?;
    let mut failed = false;
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let time = serde_json::from_str::<InspectRequest>(&line).map(|request| request.time);
        let time = match time {
            Ok(time) => time,
            Err(error) => {
                failed = true;
                write_message(
                    &mut output,
                    &json!({ "error": error.to_string(), "status": "requestError" }),
                )?;
                continue;
            }
        };
        match bridge.evaluate_at(time, None) {
            Ok(frame) => write_message(
                &mut output,
                &json!({ "scene": frame.scene, "audio": frame.audio }),
            )?,
            Err(error) => {
                let fatal = !matches!(
                    error,
                    ReactBridgeError::Render(_) | ReactBridgeError::Time(_)
                );
                write_message(
                    &mut output,
                    &json!({
                        "error": error.to_string(),
                        "status": if fatal { "runtimeError" } else { "sceneError" },
                        "fatal": fatal,
                    }),
                )?;
                if fatal {
                    return Err(error.to_string());
                }
                failed = true;
            }
        }
    }
    if failed {
        Err("one or more inspection requests failed".to_owned())
    } else {
        Ok(())
    }
}

fn write_message(output: &mut impl Write, message: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *output, message).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}
