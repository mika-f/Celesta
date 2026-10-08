//! The scenes each workload renders. `nebula` is the real NEBULA
//! composition from `examples/versus/bench`, and `spectra` the SPECTRA reel
//! from `examples/spectra`, both evaluated by the React bridge (Node.js and
//! a built `packages/react` are required). The others are
//! synthetic scenes, one per kind of work the GPU renderer does, laid out on
//! a 1920x1080 canvas. Every frame moves something, so caches only help
//! where they would in an export. All are scaled to the requested size, so
//! a small canvas does proportionally less of the same work.

use std::f64::consts::{PI, TAU};
use std::path::{Path, PathBuf};

use celesta_composition::{
    AssetLocation, BlendMode, Clip, EvaluatedTransform, GradientStop, ImageFit, Layer,
    LayerContent, LayerEffects, LayerGlow, LayerShadow, LineBreak, LineCap, LineJoin, Paint,
    PathCommand, Point, Rational, ResolvedAsset, Scene, Stroke, TextStyle, Time,
};
use celesta_react_bridge::{ReactBridge, runtime_paths};

/// The image the `images` workload draws, relative to the asset root.
pub const IMAGE_PATH: &str = "bench.png";

pub struct Workload {
    pub name: &'static str,
    pub description: &'static str,
    source: Source,
}

enum Source {
    /// Builds a frame's layers on the 1920x1080 canvas.
    Synthetic(fn(&Canvas, usize) -> Vec<Layer>),
    /// A React entry, relative to the repository root, and where its runs
    /// of frames start: the measured frames are split evenly into runs of
    /// consecutive frames, one from each start (see `run_frame`), so a film
    /// with chapters is measured in every one of them. It needs at least as
    /// many measured frames as starts.
    React(&'static str, &'static [u64]),
}

pub const WORKLOADS: &[Workload] = &[
    Workload {
        name: "nebula",
        description: "NEBULA from examples/versus/bench (React; about 1,660 layers)",
        source: Source::React("examples/versus/bench/celesta/nebula.tsx", &[0]),
    },
    Workload {
        name: "spectra",
        description: "SPECTRA from examples/spectra (React; every chapter and crossfade)",
        // Each chapter once it has settled, and the crossfades into
        // GEOMETRY (frames 135-149) and SOURCE (405-419), where two chapters
        // draw at once: a run of a single frame, as in CI, lands in them too.
        source: Source::React("examples/spectra/film.tsx", &[60, 140, 300, 410, 600]),
    },
    Workload {
        name: "rings",
        description: "24 rotating stroked ellipses (NEBULA's rings, PR #113)",
        source: Source::Synthetic(rings),
    },
    Workload {
        name: "blur",
        description: "six large blurred blobs plus cards with shadows and glows",
        source: Source::Synthetic(blur),
    },
    Workload {
        name: "ribbons",
        description: "about 3,000 thin rotated rects that change every frame",
        source: Source::Synthetic(ribbons),
    },
    Workload {
        name: "text",
        description: "scaling text lines, a wrapped paragraph, and a ticking counter",
        source: Source::Synthetic(text),
    },
    Workload {
        name: "shapes",
        description: "gradient rounded rects in clipped, rotated, blended groups",
        source: Source::Synthetic(shapes),
    },
    Workload {
        name: "images",
        description: "a 512x512 image drawn 16 times, scaled and rotated",
        source: Source::Synthetic(images),
    },
];

impl Workload {
    /// The scenes of `warmup` frames and then `frames` measured ones, at
    /// `width`x`height`, and the asset root their relative paths resolve
    /// against. `images` is the directory holding [`IMAGE_PATH`], which only
    /// the `images` workload needs.
    pub fn scenes(
        &self,
        width: u32,
        height: u32,
        warmup: usize,
        frames: usize,
        images: Option<&Path>,
    ) -> Result<(Vec<Scene>, PathBuf), String> {
        let count = warmup + frames;
        match self.source {
            Source::Synthetic(build) => {
                let canvas = Canvas {
                    k: f64::from(width) / 1920.0,
                };
                let scenes = (0..count)
                    .map(|frame| synthetic(&canvas, width, height, build(&canvas, frame)))
                    .collect();
                Ok((scenes, images.map_or_else(PathBuf::new, Path::to_owned)))
            }
            Source::React(entry, starts) => {
                if frames < starts.len() {
                    return Err(format!(
                        "measures {} runs of frames, so it needs --frames {} or more",
                        starts.len(),
                        starts.len()
                    ));
                }
                let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(entry)
                    .canonicalize()
                    .map_err(|error| format!("{entry}: {error}"))?;
                let (node, cli) = runtime_paths();
                if !cli.is_file() {
                    return Err(format!(
                        "{} is missing; build packages/react first (see README.md)",
                        cli.display()
                    ));
                }
                let mut bridge = ReactBridge::spawn(node, &cli, &entry)
                    .map_err(|error| format!("starting the React bridge: {error}"))?;
                let metadata = bridge.metadata().clone();
                let scenes = (0..count)
                    .map(|index| {
                        let frame = run_frame(starts, warmup, frames, index);
                        let frame = (frame % metadata.duration_in_frames.max(1)) as i64;
                        let time = Time::frames(frame, metadata.frame_rate)
                            .map_err(|error| error.to_string())?;
                        let scene = bridge
                            .scene_at(time)
                            .map_err(|error| format!("evaluating frame {frame}: {error}"))?;
                        Ok(fit(scene, width, height))
                    })
                    .collect::<Result<_, String>>()?;
                let root = entry.parent().expect("an entry is a file").to_owned();
                Ok((scenes, root))
            }
        }
    }
}

/// The frame the `index`th scene shows. The warmup frames start at the
/// first start and the first run follows them, so a single run from 0
/// renders frames `0..warmup + frames` in order; every other run starts at
/// its own start.
fn run_frame(starts: &[u64], warmup: usize, frames: usize, index: usize) -> u64 {
    let Some(measured) = index.checked_sub(warmup) else {
        return starts[0] + index as u64;
    };
    let run = measured * starts.len() / frames;
    let first = (run * frames).div_ceil(starts.len());
    let skipped = if run == 0 { warmup } else { 0 };
    starts[run] + (skipped + measured - first) as u64
}

fn synthetic(canvas: &Canvas, width: u32, height: u32, layers: Vec<Layer>) -> Scene {
    let mut background = rect(
        "background",
        canvas,
        Point { x: 0.0, y: 0.0 },
        1920.0,
        1080.0,
        solid("#14161C"),
    );
    background.transform.anchor = Point { x: 0.0, y: 0.0 };
    Scene {
        width,
        height,
        frame_rate: Rational::new(60, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: std::iter::once(background).chain(layers).collect(),
    }
}

/// Scales a scene to `width`x`height` (by width) in a group. Effect radii
/// and offsets are in output pixels, so they are scaled as well.
fn fit(mut scene: Scene, width: u32, height: u32) -> Scene {
    if (scene.width, scene.height) == (width, height) {
        return scene;
    }
    let k = f64::from(width) / f64::from(scene.width);
    fn scale_effects(layers: &mut [Layer], k: f64) {
        for layer in layers {
            let effects = &mut layer.effects;
            effects.blur *= k;
            if let Some(shadow) = &mut effects.shadow {
                shadow.blur *= k;
                shadow.offset_x *= k;
                shadow.offset_y *= k;
            }
            if let Some(glow) = &mut effects.glow {
                glow.blur *= k;
            }
            if let LayerContent::Group { layers, .. } = &mut layer.content {
                scale_effects(layers, k);
            }
        }
    }
    scale_effects(&mut scene.layers, k);
    let layers = std::mem::take(&mut scene.layers);
    scene.layers = vec![layer(
        "bench-fit".to_owned(),
        EvaluatedTransform {
            scale: Point { x: k, y: k },
            ..EvaluatedTransform::default()
        },
        LayerContent::Group {
            layers,
            clip: None,
            mask: None,
        },
    )];
    (scene.width, scene.height) = (width, height);
    scene
}

/// Maps the 1920x1080 layout to the output size.
struct Canvas {
    k: f64,
}

impl Canvas {
    fn at(&self, x: f64, y: f64) -> Point {
        Point {
            x: x * self.k,
            y: y * self.k,
        }
    }

    fn len(&self, value: f64) -> f64 {
        value * self.k
    }
}

fn seconds(frame: usize) -> f64 {
    frame as f64 / 60.0
}

fn layer(id: String, transform: EvaluatedTransform, content: LayerContent) -> Layer {
    Layer {
        id,
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: LayerEffects::default(),
        content,
    }
}

fn at(position: Point) -> EvaluatedTransform {
    EvaluatedTransform {
        position,
        ..EvaluatedTransform::default()
    }
}

fn solid(color: &str) -> Paint {
    Paint::Solid {
        color: color.to_owned(),
    }
}

fn stops(colors: &[&str]) -> Vec<GradientStop> {
    let last = (colors.len() - 1) as f64;
    colors
        .iter()
        .enumerate()
        .map(|(index, color)| GradientStop {
            offset: index as f64 / last,
            color: (*color).to_owned(),
        })
        .collect()
}

fn rect(id: &str, canvas: &Canvas, center: Point, width: f64, height: f64, fill: Paint) -> Layer {
    layer(
        id.to_owned(),
        at(canvas.at(center.x, center.y)),
        LayerContent::Rect {
            width: canvas.len(width),
            height: canvas.len(height),
            fill: Some(fill),
            stroke: None,
            corner_radius: 0.0,
        },
    )
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

fn rings(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    let t = seconds(frame);
    (0..24)
        .map(|k| {
            let rx = canvas.len(180.0 + k as f64 * 30.0);
            let direction = if k % 2 == 1 { -12.0 } else { 12.0 };
            let mut ring = layer(
                format!("ring-{k}"),
                EvaluatedTransform {
                    rotation: k as f64 * 7.5 + t * direction,
                    ..at(canvas.at(960.0, 540.0))
                },
                LayerContent::Path {
                    commands: ellipse(rx, rx * 0.38),
                    fill: None,
                    stroke: Some(Stroke {
                        paint: solid("#8FB8FF"),
                        width: canvas.len(1.5).max(1.0),
                    }),
                    line_cap: LineCap::Butt,
                    line_join: LineJoin::Miter,
                    miter_limit: 4.0,
                },
            );
            ring.opacity = 0.18 + 0.22 * (0.5 + 0.5 * (t * 2.0 + k as f64 * 0.4).sin());
            ring
        })
        .collect()
}

fn blur(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    let t = seconds(frame);
    let colors = [
        "#5B6CFF", "#FF5BC8", "#3EE6C1", "#FFB13E", "#9B5BFF", "#3EA8FF",
    ];
    let mut layers: Vec<_> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| {
            let phase = t * 0.7 + index as f64 * TAU / 6.0;
            let r = canvas.len(220.0);
            let mut blob = layer(
                format!("blob-{index}"),
                at(canvas.at(
                    960.0 + 520.0 * phase.cos(),
                    540.0 + 260.0 * (phase * 1.3).sin(),
                )),
                LayerContent::Path {
                    commands: ellipse(r, r * 0.8),
                    fill: Some(Paint::Radial {
                        center: Point { x: 0.0, y: 0.0 },
                        radius: r,
                        stops: stops(&[color, "#00000000"]),
                    }),
                    stroke: None,
                    line_cap: LineCap::Butt,
                    line_join: LineJoin::Miter,
                    miter_limit: 4.0,
                },
            );
            blob.opacity = 0.8;
            blob.blend_mode = BlendMode::Screen;
            blob.effects.blur = canvas.len(48.0);
            blob
        })
        .collect();
    for index in 0..4 {
        let x = 330.0 + index as f64 * 420.0;
        let y = 540.0 + 30.0 * (t * 2.0 + index as f64).sin();
        let mut card = layer(
            format!("card-{index}"),
            at(canvas.at(x, y)),
            LayerContent::Rect {
                width: canvas.len(320.0),
                height: canvas.len(200.0),
                fill: Some(solid("#1E2230")),
                stroke: None,
                corner_radius: canvas.len(24.0),
            },
        );
        card.effects.shadow = Some(LayerShadow {
            color: "#000000B0".to_owned(),
            blur: canvas.len(24.0),
            offset_x: 0.0,
            offset_y: canvas.len(12.0),
        });
        if index % 2 == 0 {
            card.effects.glow = Some(LayerGlow {
                color: "#8FB8FF".to_owned(),
                blur: canvas.len(16.0),
            });
        }
        layers.push(card);
    }
    layers
}

fn ribbons(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    const RIBBONS: usize = 3;
    const STRANDS: usize = 9;
    const SEGMENTS: usize = 112;
    let t = seconds(frame);
    (0..RIBBONS)
        .map(|ribbon| {
            let cx = 960.0 + (ribbon as f64 - 1.0) * 400.0;
            let mut rects = Vec::with_capacity(STRANDS * SEGMENTS);
            for strand in 0..STRANDS {
                let twist = strand as f64 / STRANDS as f64 * TAU;
                let point = |segment: usize| {
                    let s = segment as f64 / SEGMENTS as f64;
                    let angle = s * 4.0 * PI + t * 1.5 + twist + ribbon as f64;
                    let depth = angle.cos();
                    (
                        cx + 150.0 * angle.sin() + 40.0 * (s * 9.0 + t).sin(),
                        80.0 + s * 920.0,
                        depth,
                    )
                };
                for segment in 0..SEGMENTS {
                    let (x0, y0, depth) = point(segment);
                    let (x1, y1, _) = point(segment + 1);
                    let length = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                    let mut piece = layer(
                        format!("r{ribbon}-s{strand}-{segment}"),
                        EvaluatedTransform {
                            rotation: (y1 - y0).atan2(x1 - x0).to_degrees(),
                            ..at(canvas.at((x0 + x1) / 2.0, (y0 + y1) / 2.0))
                        },
                        LayerContent::Rect {
                            width: canvas.len(length + 1.0),
                            height: canvas.len(1.0 + 2.0 * (depth + 1.0)),
                            fill: Some(solid("#E8E4DA")),
                            stroke: None,
                            corner_radius: 0.0,
                        },
                    );
                    piece.opacity = 0.25 + 0.35 * (depth + 1.0);
                    rects.push(piece);
                }
            }
            layer(
                format!("ribbon-{ribbon}"),
                EvaluatedTransform {
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                LayerContent::Group {
                    layers: rects,
                    clip: None,
                    mask: None,
                },
            )
        })
        .collect()
}

fn text(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    let t = seconds(frame);
    let style = |size: f64, color: &str| TextStyle {
        font_size: Some(canvas.len(size)),
        fill: Some(solid(color)),
        ..TextStyle::default()
    };
    let mut layers: Vec<_> = (0..10)
        .map(|index| {
            let phase = t * PI + index as f64 * 0.6;
            let scale = 1.5 - 0.5 * phase.cos();
            layer(
                format!("line-{index}"),
                EvaluatedTransform {
                    scale: Point { x: scale, y: scale },
                    ..at(canvas.at(700.0, 60.0 + index as f64 * 96.0))
                },
                LayerContent::Text {
                    text: format!("Line {index}: 回線を、自動で見分ける。"),
                    style: style(40.0, "#FFFFFF"),
                    max_width: None,
                    baseline_anchor: false,
                },
            )
        })
        .collect();
    layers.push(layer(
        "paragraph".to_owned(),
        at(canvas.at(1560.0, 540.0)),
        LayerContent::Text {
            text: "Celesta はコードで動画を書くためのツールです。タイムラインを JSON や \
                   React で記述し、プレビューしながら MP4 に書き出せます。"
                .repeat(3),
            style: TextStyle {
                line_break: Some(LineBreak::Phrase),
                stroke: Some(Stroke {
                    paint: solid("#000000"),
                    width: canvas.len(2.0),
                }),
                ..style(28.0, "#E0E6FF")
            },
            max_width: Some(canvas.len(560.0)),
            baseline_anchor: false,
        },
    ));
    layers.push(layer(
        "counter".to_owned(),
        at(canvas.at(1560.0, 980.0)),
        LayerContent::Text {
            text: format!(
                "{:02}:{:02}.{:02}",
                frame / 3600,
                frame / 60 % 60,
                frame % 60
            ),
            style: style(64.0, "#FFB13E"),
            max_width: None,
            baseline_anchor: false,
        },
    ));
    layers
}

fn shapes(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    let t = seconds(frame);
    let modes = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
        BlendMode::Difference,
    ];
    modes
        .iter()
        .enumerate()
        .map(|(group, mode)| {
            let children = (0..12)
                .map(|index| {
                    let (column, row) = ((index % 4) as f64, (index / 4) as f64);
                    let size = canvas.len(110.0);
                    let fill = if index % 2 == 0 {
                        Paint::Linear {
                            start: Point { x: 0.0, y: 0.0 },
                            end: Point { x: size, y: size },
                            stops: stops(&["#FF5BC8", "#5B6CFF", "#3EE6C1"]),
                        }
                    } else {
                        Paint::Radial {
                            center: Point {
                                x: size / 2.0,
                                y: size / 2.0,
                            },
                            radius: size / 2.0,
                            stops: stops(&["#FFFFFF", "#FFB13E", "#FFB13E00"]),
                        }
                    };
                    let mut shape = layer(
                        format!("shape-{group}-{index}"),
                        EvaluatedTransform {
                            rotation: t * 40.0 + index as f64 * 15.0,
                            ..at(canvas.at(-180.0 + column * 120.0, -120.0 + row * 120.0))
                        },
                        LayerContent::Rect {
                            width: size,
                            height: size,
                            fill: Some(fill),
                            stroke: Some(Stroke {
                                paint: solid("#FFFFFF80"),
                                width: canvas.len(3.0),
                            }),
                            corner_radius: canvas.len(10.0 + 10.0 * (t + index as f64).sin()),
                        },
                    );
                    shape.opacity = 0.7;
                    shape
                })
                .collect();
            let (column, row) = ((group % 3) as f64, (group / 3) as f64);
            let mut layer = layer(
                format!("group-{group}"),
                EvaluatedTransform {
                    rotation: 8.0 * (t + group as f64).sin(),
                    ..at(canvas.at(340.0 + column * 620.0, 290.0 + row * 500.0))
                },
                LayerContent::Group {
                    layers: children,
                    clip: Some(Clip {
                        x: canvas.len(-240.0),
                        y: canvas.len(-170.0),
                        width: canvas.len(480.0),
                        height: canvas.len(340.0),
                        corner_radius: canvas.len(32.0),
                    }),
                    mask: None,
                },
            );
            layer.blend_mode = *mode;
            layer.opacity = 0.9;
            layer
        })
        .collect()
}

fn images(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    let t = seconds(frame);
    (0..16)
        .map(|index| {
            let (column, row) = ((index % 4) as f64, (index / 4) as f64);
            let scale = 0.6 + 0.4 * (t * 1.7 + index as f64).sin().abs();
            let mut image = layer(
                format!("image-{index}"),
                EvaluatedTransform {
                    scale: Point { x: scale, y: scale },
                    rotation: t * 20.0 * if index % 2 == 0 { 1.0 } else { -1.0 },
                    ..at(canvas.at(300.0 + column * 440.0, 160.0 + row * 250.0))
                },
                LayerContent::Image {
                    asset: ResolvedAsset {
                        id: "bench-image".to_owned(),
                        location: AssetLocation::File {
                            path: IMAGE_PATH.to_owned(),
                        },
                    },
                    width: Some(canvas.len(400.0)),
                    height: Some(canvas.len(225.0)),
                    fit: Some(if index % 2 == 0 {
                        ImageFit::Cover
                    } else {
                        ImageFit::Contain
                    }),
                },
            );
            image.opacity = 0.85;
            image
        })
        .collect()
}

/// The image `images` draws: a gradient with a checkerboard, so filtering
/// and mipmapping have edges to work on.
pub fn image() -> image::RgbaImage {
    image::RgbaImage::from_fn(512, 512, |x, y| {
        let check = ((x / 32) + (y / 32)) % 2 == 0;
        let base = if check { 255 } else { 96 };
        image::Rgba([
            (x / 2) as u8,
            (y / 2) as u8,
            base,
            if (x / 64 + y / 64) % 5 == 0 { 160 } else { 255 },
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::run_frame;

    #[test]
    fn one_run_renders_frames_in_order() {
        let frames: Vec<_> = (0..12).map(|index| run_frame(&[0], 2, 10, index)).collect();
        assert_eq!(frames, (0..12).collect::<Vec<_>>());
    }

    #[test]
    fn every_start_gets_a_run() {
        let starts = [60, 140, 300, 410, 600];
        let frames: Vec<_> = (0..8)
            .map(|index| run_frame(&starts, 2, 6, index))
            .collect();
        assert_eq!(frames, [60, 61, 62, 63, 140, 300, 410, 600]);
        let frames: Vec<_> = (0..20)
            .map(|index| run_frame(&starts, 0, 20, index))
            .collect();
        assert_eq!(&frames[..5], [60, 61, 62, 63, 140]);
        assert_eq!(frames[19], 603);
        // Later runs start at their own start, whatever the warmup.
        let frames: Vec<_> = (0..130)
            .map(|index| run_frame(&starts, 10, 120, index))
            .collect();
        assert_eq!(frames[10], 70);
        assert_eq!(&frames[34..36], [140, 141]);
        assert_eq!(frames[82], 410);
    }
}
