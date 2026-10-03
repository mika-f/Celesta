//! Scene inspection uses the same bridge as export, including native text
//! shaping during prepare() and frame evaluation, without allocating a GPU.

use std::io::{self, BufRead, Write};
use std::path::Path;

use celesta_composition::Time;
use celesta_react_bridge::ReactBridge;
use serde_json::{Value, json};

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
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let response = (|| {
            let request: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
            let time: Time = serde_json::from_value(request["time"].clone())
                .map_err(|error| error.to_string())?;
            let frame = bridge
                .evaluate_at(time, None)
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(json!({ "scene": frame.scene, "audio": frame.audio }))
        })();
        let response = response.unwrap_or_else(|error| json!({ "error": error }));
        write_message(&mut output, &response)?;
    }
    Ok(())
}

fn write_message(output: &mut impl Write, message: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *output, message).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}
