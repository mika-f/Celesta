//! The 24 rotating, thin ellipses from PR #113's NEBULA scene, isolated from
//! text, effects, React evaluation, and encoding. Measures the shared CPU
//! rasterizer and the GPU export's submit/drain pipeline separately.
//!
//! cargo run --release -p celesta-gpu-renderer --example path-bench -- [frames]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use celesta_composition::{
    BlendMode, EvaluatedTransform, Layer, LayerContent, LineCap, LineJoin, Paint, PathCommand,
    Point, Rational, Scene, Stroke, Time,
};
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer};
use celesta_renderer::{PathDraw, PathShape, PathTransform, rasterize_paths};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const WARMUP: usize = 10;

fn main() {
    let frames: usize = std::env::args()
        .nth(1)
        .map_or(120, |value| value.parse().expect("frames must be a number"));
    assert!(frames > 0, "frames must be positive");
    let scenes: Vec<_> = (0..frames.max(WARMUP)).map(scene).collect();
    let rasterize = |scene: &Scene| {
        let draws: Vec<_> = scene.layers.iter().map(draw).collect();
        rasterize_paths(&draws, WIDTH, HEIGHT)
            .expect("paths rasterize")
            .expect("rings are visible")
    };
    for scene in &scenes[..WARMUP] {
        std::hint::black_box(rasterize(scene));
    }
    let mut elapsed = Duration::ZERO;
    let mut checksum = DefaultHasher::new();
    for scene in &scenes[..frames] {
        let started = Instant::now();
        let path = rasterize(scene);
        elapsed += started.elapsed();
        // Outside the timing: detect any changed pixels across revisions.
        path.image.pixels().hash(&mut checksum);
    }
    report("raster", frames, elapsed);
    println!("raster checksum: {:016x}", checksum.finish());

    let mut renderer = GpuRenderer::new(GpuRenderOptions::default()).expect("GPU renderer");
    println!(
        "gpu: {} ({:?})",
        renderer.adapter_info().name,
        renderer.adapter_info().backend
    );
    for scene in &scenes[..WARMUP] {
        renderer.submit(scene).expect("warmup renders");
    }
    renderer.drain().expect("warmup drains");
    let started = Instant::now();
    let mut rendered = 0;
    for scene in &scenes[..frames] {
        rendered += usize::from(renderer.submit(scene).expect("frame renders").is_some());
    }
    rendered += renderer.drain().expect("frames drain").len();
    let elapsed = started.elapsed();
    assert_eq!(rendered, frames);
    report("gpu submit/drain", frames, elapsed);
}

fn report(name: &str, frames: usize, elapsed: Duration) {
    println!(
        "{name}: {frames} frames, {:.2} ms/frame ({:.1} fps)",
        elapsed.as_secs_f64() * 1000.0 / frames as f64,
        frames as f64 / elapsed.as_secs_f64()
    );
}

fn scene(frame: usize) -> Scene {
    let t = frame as f64 / 60.0;
    Scene {
        width: WIDTH,
        height: HEIGHT,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: (0..24)
            .map(|k| {
                let rx = 180.0 + k as f64 * 30.0;
                Layer {
                    id: format!("ring-{k}"),
                    transform: EvaluatedTransform {
                        position: Point { x: 960.0, y: 540.0 },
                        rotation: k as f64 * 7.5 + t * if k % 2 == 1 { -12.0 } else { 12.0 },
                        ..Default::default()
                    },
                    opacity: 0.18 + 0.22 * (0.5 + 0.5 * (t * 2.0 + k as f64 * 0.4).sin()),
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content: LayerContent::Path {
                        commands: ellipse(rx, rx * 0.38),
                        fill: None,
                        stroke: Some(Stroke {
                            paint: Paint::Solid {
                                color: "#8FB8FF".to_owned(),
                            },
                            width: 1.5,
                        }),
                        line_cap: LineCap::Butt,
                        line_join: LineJoin::Miter,
                        miter_limit: 4.0,
                    },
                }
            })
            .collect(),
    }
}

fn draw(layer: &Layer) -> PathDraw<'_> {
    let LayerContent::Path {
        commands,
        fill,
        stroke,
        line_cap,
        line_join,
        miter_limit,
    } = &layer.content
    else {
        unreachable!("benchmark contains only paths");
    };
    let (sin, cos) = layer.transform.rotation.to_radians().sin_cos();
    PathDraw {
        shape: PathShape {
            commands,
            fill: fill.as_ref(),
            stroke: stroke.as_ref(),
            line_cap: *line_cap,
            line_join: *line_join,
            miter_limit: *miter_limit,
        },
        transform: PathTransform {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: layer.transform.position.x,
            ty: layer.transform.position.y,
        },
        opacity: layer.opacity,
    }
}

fn ellipse(rx: f64, ry: f64) -> Vec<PathCommand> {
    let k = 0.5523;
    vec![
        PathCommand::MoveTo { x: rx, y: 0.0 },
        PathCommand::CubicTo {
            x1: rx,
            y1: ry * k,
            x2: rx * k,
            y2: ry,
            x: 0.0,
            y: ry,
        },
        PathCommand::CubicTo {
            x1: -rx * k,
            y1: ry,
            x2: -rx,
            y2: ry * k,
            x: -rx,
            y: 0.0,
        },
        PathCommand::CubicTo {
            x1: -rx,
            y1: -ry * k,
            x2: -rx * k,
            y2: -ry,
            x: 0.0,
            y: -ry,
        },
        PathCommand::CubicTo {
            x1: rx * k,
            y1: -ry,
            x2: rx,
            y2: -ry * k,
            x: rx,
            y: 0.0,
        },
        PathCommand::Close,
    ]
}
