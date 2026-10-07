//! End-to-end Code evaluation, transfer size and CPU/GPU rendering timings.
//! Build packages/react first, then run with a TSX entry and an optional frame count.
use celesta_composition::{Layer, LayerContent, Time};
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer};
use celesta_react_bridge::ReactBridge;
use celesta_renderer::{CpuRenderer, RenderOptions};
use std::{env, error::Error, path::PathBuf, time::Instant};

fn texts(layers: &[Layer]) -> usize {
    layers
        .iter()
        .map(|layer| match &layer.content {
            LayerContent::Text { .. } => 1,
            LayerContent::Group { layers, .. } => texts(layers),
            _ => 0,
        })
        .sum()
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let entry = env::args()
        .nth(1)
        .ok_or("usage: code-runs <entry.tsx> [frames]")?;
    let frames: i64 = env::args().nth(2).map_or(Ok(30), |value| value.parse())?;
    if frames < 1 {
        return Err("frames must be positive".into());
    }
    let started = Instant::now();
    let mut bridge = ReactBridge::spawn(
        "node",
        root.join("packages/react/bin/celesta-react-render.js"),
        &entry,
    )?;
    let first = bridge.scene_at(Time::new(0, 30))?;
    let initial = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let scenes = (0..frames)
        .map(|frame| bridge.scene_at(Time::new(120 + frame, 30)))
        .collect::<Result<Vec<_>, _>>()?;
    let evaluation = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let started = Instant::now();
    let typing = (1..=frames)
        .map(|frame| bridge.scene_at(Time::new((frame * 119 / frames).clamp(1, 119), 30)))
        .collect::<Result<Vec<_>, _>>()?;
    let typing_evaluation = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    // Renderer setup is excluded, including device/pipeline initialization.
    let mut cpu = CpuRenderer::new(RenderOptions::default());
    let mut gpu = GpuRenderer::new(GpuRenderOptions::default())?;
    let adapter = format!(
        "{} ({:?})",
        gpu.adapter_info().name,
        gpu.adapter_info().backend
    );
    let started = Instant::now();
    cpu.render(&first)?;
    let cpu_first = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    gpu.render(&first)?;
    let gpu_first = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    for scene in &scenes {
        cpu.render(scene)?;
    }
    let cpu_continued = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let started = Instant::now();
    for scene in &scenes {
        gpu.submit(scene)?;
    }
    gpu.drain()?;
    let gpu_continued = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let started = Instant::now();
    for scene in &typing {
        cpu.render(scene)?;
    }
    let cpu_typing = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let started = Instant::now();
    for scene in &typing {
        gpu.submit(scene)?;
    }
    gpu.drain()?;
    let gpu_typing = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    println!(
        "{}",
        serde_json::json!({
            "entry": entry, "adapter": adapter, "frames": frames,
            "text_layers": texts(&first.layers), "scene_bytes": serde_json::to_vec(&first)?.len(),
            "initial_evaluation_ms": initial, "continued_evaluation_ms": evaluation, "typing_evaluation_ms": typing_evaluation,
            "cpu_first_ms": cpu_first, "cpu_continued_ms": cpu_continued, "cpu_typing_ms": cpu_typing,
            "gpu_first_ms": gpu_first, "gpu_continued_ms": gpu_continued, "gpu_typing_ms": gpu_typing,
        })
    );
    Ok(())
}
