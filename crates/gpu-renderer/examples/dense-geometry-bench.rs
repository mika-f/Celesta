//! Times the GPU renderer on dense animated geometry: the projected ribbons
//! of `examples/afterimage/film.tsx`, thousands of thin rotated rects whose
//! length, thickness, position, rotation, and opacity change every frame,
//! plus a full-frame wipe that changes size every frame. Frames go through
//! the same pipelined `submit`/`drain` readback an export uses. Run as:
//!
//! ```text
//! cargo run --release -p celesta-gpu-renderer --example dense-geometry-bench -- [frames] [ribbons] [strands] [bands]
//! ```
//!
//! The defaults (120 frames, 3 ribbons of 9 strands: 3,024 rects) match the
//! film's densest section.
//!
//! The same ribbons are then drawn as native `Path` layers (issue #31): each
//! strand is split where its depth crosses into another of `bands` depth
//! bands (default 3), and each run is one stroked path whose width and
//! opacity come from its band, drawn back to front by band.

use std::env;
use std::f64::consts::PI;
use std::time::{Duration, Instant};

use celesta_composition::{
    BlendMode, EvaluatedTransform, Layer, LayerContent, LineCap, LineJoin, Paint, PathCommand,
    Point, Rational, Scene, Stroke, Time,
};
use celesta_gpu_renderer::{GpuRenderOptions, GpuRenderer};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const SEGMENTS: usize = 112;

fn main() {
    let mut args = env::args().skip(1);
    let mut next = |default: usize| {
        args.next().map_or(default, |value| {
            value.parse().expect("arguments are numbers")
        })
    };
    let frames = next(120);
    let ribbons = next(3);
    let strands = next(9);
    let bands = next(3);

    let mut renderer = GpuRenderer::new(GpuRenderOptions::default()).expect("gpu renderer");
    println!(
        "gpu: {} ({:?})",
        renderer.adapter_info().name,
        renderer.adapter_info().backend
    );

    for (name, geometry) in [
        ("rects", Geometry::Rects),
        ("paths", Geometry::Paths { bands }),
    ] {
        let started = Instant::now();
        let mut building = Duration::ZERO;
        let mut rendered = 0;
        let mut layers = 0;
        for frame in 0..frames {
            let build_started = Instant::now();
            let scene = scene(frame, ribbons, strands, geometry);
            building += build_started.elapsed();
            layers = layers.max(count_layers(&scene.layers));
            rendered += usize::from(renderer.submit(&scene).expect("frame renders").is_some());
        }
        rendered += renderer.drain().expect("frames drain").len();
        let elapsed = started.elapsed();
        assert_eq!(rendered, frames);

        let per_frame = elapsed.as_secs_f64() * 1000.0 / frames as f64;
        println!(
            "{name}: {frames} frames of {WIDTH}x{HEIGHT}, up to {layers} layers: {elapsed:.2?} \
             ({per_frame:.2} ms/frame, {:.1} fps; building scenes {:.2} ms/frame)",
            frames as f64 / elapsed.as_secs_f64(),
            building.as_secs_f64() * 1000.0 / frames as f64,
        );
    }
}

#[derive(Clone, Copy)]
enum Geometry {
    /// One rotated rect per segment, as the film draws them.
    Rects,
    /// One path per run of a strand inside one of `bands` depth bands.
    Paths { bands: usize },
}

fn count_layers(layers: &[Layer]) -> usize {
    layers
        .iter()
        .map(|layer| match &layer.content {
            LayerContent::Group {
                layers,
                clip: _,
                mask,
            } => {
                1 + count_layers(layers)
                    + mask.as_ref().map_or(0, |mask| count_layers(&mask.layers))
            }
            _ => 1,
        })
        .sum()
}

fn scene(frame: usize, ribbons: usize, strands: usize, geometry: Geometry) -> Scene {
    let time = frame as f64 / 30.0;
    let mut layers = vec![rect(
        "background",
        Point { x: 0.0, y: 0.0 },
        0.0,
        f64::from(WIDTH),
        f64::from(HEIGHT),
        "#EAE5D9",
        1.0,
    )];
    for index in 0..ribbons {
        let offset = index as f64 - (ribbons as f64 - 1.0) / 2.0;
        layers.push(Layer {
            id: format!("ribbon-{index}"),
            transform: EvaluatedTransform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Group {
                layers: ribbon(
                    time - index as f64 * 0.35,
                    960.0 + offset * 400.0,
                    510.0,
                    310.0,
                    strands,
                    geometry,
                ),
                clip: None,
                mask: None,
            },
        });
    }
    // A wipe whose width changes every frame, as in the film's intro.
    let wipe = (frame % 45) as f64 / 45.0;
    layers.push(rect(
        "wipe",
        Point { x: 0.0, y: 0.0 },
        0.0,
        f64::from(WIDTH / 2) * (1.0 - wipe),
        f64::from(HEIGHT),
        "#171716",
        1.0,
    ));
    Scene {
        width: WIDTH,
        height: HEIGHT,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers,
    }
}

/// The half-twist ribbon of `examples/afterimage/film.tsx`.
fn ribbon(time: f64, x: f64, y: f64, size: f64, strands: usize, geometry: Geometry) -> Vec<Layer> {
    let yaw = time * 0.23;
    let pitch = 0.85 + (time * 0.19).sin() * 0.35;
    let project = |u: f64, v: f64| {
        let radius = 1.0 + v * (u / 2.0).cos();
        let (px, py, pz) = (radius * u.cos(), radius * u.sin(), v * (u / 2.0).sin());
        let xx = px * yaw.cos() + pz * yaw.sin();
        let zz = -px * yaw.sin() + pz * yaw.cos();
        let yy = py * pitch.cos() - zz * pitch.sin();
        let depth = py * pitch.sin() + zz * pitch.cos();
        let perspective = 3.6 / (3.6 - depth);
        (
            x + size * xx * perspective,
            y + size * yy * perspective,
            depth,
        )
    };
    if let Geometry::Paths { bands } = geometry {
        return ribbon_paths(&project, strands, bands.max(1));
    }
    let mut segments = Vec::with_capacity(strands * SEGMENTS);
    for strand in 0..strands {
        let v = strand_v(strand, strands);
        for index in 0..SEGMENTS {
            let a = project(index as f64 / SEGMENTS as f64 * PI * 4.0, v);
            let b = project((index + 1) as f64 / SEGMENTS as f64 * PI * 4.0, v);
            segments.push((a, b, (a.2 + b.2) / 2.0));
        }
    }
    segments.sort_by(|left, right| left.2.total_cmp(&right.2));
    segments
        .into_iter()
        .enumerate()
        .map(|(index, (a, b, depth))| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let mut line = rect(
                &format!("segment-{index}"),
                Point { x: a.0, y: a.1 },
                dy.atan2(dx).to_degrees(),
                dx.hypot(dy) + 0.7,
                1.5 + ((depth + 1.0) / 2.0).clamp(0.0, 1.0) * 0.8,
                "#EF402B",
                0.25 + ((depth + 1.4) / 2.8).clamp(0.0, 1.0) * 0.75,
            );
            line.transform.anchor.y = 0.5;
            line
        })
        .collect()
}

fn strand_v(strand: usize, strands: usize) -> f64 {
    0.035 + strand as f64 / (strands as f64 - 1.0).max(1.0) * 0.47
}

/// The ribbon as paths: each strand cut where its depth changes band.
fn ribbon_paths(
    project: &impl Fn(f64, f64) -> (f64, f64, f64),
    strands: usize,
    bands: usize,
) -> Vec<Layer> {
    let band_of = |depth: f64| {
        ((((depth + 1.4) / 2.8).clamp(0.0, 1.0) * bands as f64) as usize).min(bands - 1)
    };
    // (band, points) runs of every strand.
    let mut runs: Vec<(usize, Vec<(f64, f64)>)> = Vec::new();
    for strand in 0..strands {
        let v = strand_v(strand, strands);
        let points: Vec<_> = (0..=SEGMENTS)
            .map(|index| project(index as f64 / SEGMENTS as f64 * PI * 4.0, v))
            .collect();
        let first = runs.len();
        for segment in points.windows(2) {
            let band = band_of((segment[0].2 + segment[1].2) / 2.0);
            let in_strand = runs.len() > first;
            match runs.last_mut() {
                Some((last, run)) if in_strand && *last == band => {
                    run.push((segment[1].0, segment[1].1));
                }
                _ => runs.push((
                    band,
                    vec![(segment[0].0, segment[0].1), (segment[1].0, segment[1].1)],
                )),
            }
        }
        // The strand is a loop: its last run continues into its first.
        if runs.len() - first > 1 && runs[first].0 == runs[runs.len() - 1].0 {
            let (_, last) = runs.pop().expect("strand has runs");
            let (_, head) = &mut runs[first];
            *head = last.into_iter().chain(head.drain(1..)).collect();
        }
    }
    runs.sort_by_key(|(band, _)| *band);
    runs.into_iter()
        .enumerate()
        .map(|(index, (band, points))| {
            let t = (band as f64 + 0.5) / bands as f64;
            let depth = t * 2.8 - 1.4;
            let commands = points
                .iter()
                .enumerate()
                .map(|(index, &(x, y))| match index {
                    0 => PathCommand::MoveTo { x, y },
                    _ => PathCommand::LineTo { x, y },
                })
                .collect();
            Layer {
                id: format!("run-{index}"),
                transform: EvaluatedTransform::default(),
                opacity: 0.25 + t * 0.75,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Path {
                    commands,
                    fill: None,
                    stroke: Some(Stroke {
                        paint: Paint::Solid {
                            color: "#EF402B".to_owned(),
                        },
                        width: 1.5 + ((depth + 1.0) / 2.0).clamp(0.0, 1.0) * 0.8,
                    }),
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    miter_limit: 4.0,
                },
            }
        })
        .collect()
}

fn rect(
    id: &str,
    position: Point,
    rotation: f64,
    width: f64,
    height: f64,
    color: &str,
    opacity: f64,
) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position,
            rotation,
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width,
            height,
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    }
}
