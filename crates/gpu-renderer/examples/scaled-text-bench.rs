//! Times the GPU renderer on text whose scale animates, the case where
//! `RenderQuality::Final` re-rasterizes text: `layers` lines that each zoom
//! from 1x to 2x and back over two seconds, out of phase, above a background.
//! Frames go through the same pipelined `submit`/`drain` readback an export
//! uses. Run as:
//!
//! ```text
//! cargo run --release -p celesta-gpu-renderer --example scaled-text-bench -- [frames] [layers]
//! ```
//!
//! The defaults (120 frames, 10 layers) match a title card with a line per
//! row popping in. Static text is cached and costs the same in both qualities.

use std::env;
use std::time::Instant;

use celesta_composition::{
    BlendMode, EvaluatedTransform, Layer, LayerContent, Paint, Point, Rational, Scene, TextStyle,
    Time,
};
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer, RenderQuality};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

fn main() {
    let mut args = env::args().skip(1);
    let mut next = |default: usize| {
        args.next().map_or(default, |value| {
            value.parse().expect("arguments are numbers")
        })
    };
    let frames = next(120);
    let layers = next(10);

    let mut renderer = GpuRenderer::new(GpuRenderOptions::default()).expect("gpu renderer");
    println!(
        "gpu: {} ({:?})",
        renderer.adapter_info().name,
        renderer.adapter_info().backend
    );
    for quality in RenderQuality::ALL {
        renderer.set_render_quality(quality);
        // Warm up fonts and pipelines outside the timed loop.
        renderer.render(&scene(0, layers)).expect("frame renders");

        let started = Instant::now();
        let mut rendered = 0;
        for frame in 0..frames {
            rendered += usize::from(
                renderer
                    .submit(&scene(frame, layers))
                    .expect("frame renders")
                    .is_some(),
            );
        }
        rendered += renderer.drain().expect("frames drain").len();
        let elapsed = started.elapsed();
        assert_eq!(rendered, frames);
        let per_frame = elapsed.as_secs_f64() * 1000.0 / frames as f64;
        println!(
            "{quality}: {frames} frames of {WIDTH}x{HEIGHT}, {layers} scaling text layers: \
             {elapsed:.2?} ({per_frame:.2} ms/frame, {:.1} fps)",
            frames as f64 / elapsed.as_secs_f64(),
        );
    }
}

fn scene(frame: usize, count: usize) -> Scene {
    let time = frame as f64 / 30.0;
    let mut layers = vec![Layer {
        id: "background".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width: f64::from(WIDTH),
            height: f64::from(HEIGHT),
            fill: Some(Paint::Solid {
                color: "#14161C".to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    }];
    for index in 0..count {
        let phase = time * std::f64::consts::PI + index as f64 * 0.6;
        let scale = 1.5 - 0.5 * phase.cos();
        layers.push(Layer {
            id: format!("line-{index}"),
            transform: EvaluatedTransform {
                position: Point {
                    x: 960.0,
                    y: 60.0 + index as f64 * (960.0 / count.max(1) as f64),
                },
                scale: Point { x: scale, y: scale },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: format!("Line {index}: 回線を、自動で見分ける。"),
                style: TextStyle {
                    font_size: Some(40.0),
                    fill: Some(Paint::Solid {
                        color: "#FFFFFF".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                max_width: None,
                baseline_anchor: false,
            },
        });
    }
    Scene {
        width: WIDTH,
        height: HEIGHT,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers,
        shaders: Vec::new(),
    }
}
