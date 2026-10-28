use crate::error::GpuRenderError;
use crate::hit::{HitRegion, hit_test};
use crate::layer::{PreparedContent, PreparedItem, PreparedLayer};
use crate::path::{
    MAX_PATH_TILE_EDGES, ShadedPath, bin_tiles, count_pieces_into_rows, path_entry_limit,
    sort_pieces_in_place,
};
use crate::plan::GpuStep;
use crate::readback::ReadbackLayout;
use crate::renderer::GpuRenderer;
use crate::text::{TextLayout, text_raster_scale};
use crate::texture::{DecodedImage, downsample};
use crate::transform::{Affine, LayerState, MAX_CLIP_DEPTH, path_transform};
use crate::types::{
    Color, GpuDriver, GpuFrame, GpuRenderOptions, GpuRenderTarget, PreviewFrame, PreviewViewport,
    ReadbackFormat, RenderQuality,
};
use celesta_renderer::image_source::fit_within;
use celesta_renderer::{
    Color as CpuColor, FlattenedPath, LineSegment, PathShape, PathTransform, flatten_path,
};

use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use strum::VariantArray;

use celesta_composition::{
    AssetLocation, BlendMode, Clip, EvaluatedTransform, GroupMask, ImageAnimation, ImageFit, Layer,
    LayerContent, LayerShadow, MaskMode, MediaTiming, Paint, Point, Rational, ResolvedAsset, Scene,
    Stroke, TextAlign, TextStyle, Time,
};
use celesta_media::{MediaError, VideoFrame, VideoFrameDecoder};

mod builtin_effect;
mod color_filter;
mod instances;
mod motion_blur;
mod plane_bands;
mod readback;
mod shader;
mod transform_3d;

fn empty_scene(width: u32, height: u32) -> Scene {
    Scene {
        width,
        height,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers: Vec::new(),
        shaders: Vec::new(),
    }
}

/// Concurrent GPU devices in one process stall the driver (threads wait
/// forever in `create_texture` / `bind_image_memory`), so tests that hold a
/// renderer run one at a time. The lock is reentrant per thread because one
/// test may hold several renderers.
static GPU_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static GPU_LOCK_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct GpuLockGuard(Option<MutexGuard<'static, ()>>);

impl GpuLockGuard {
    fn acquire() -> Self {
        let outermost = GPU_LOCK_DEPTH.with(|depth| {
            depth.set(depth.get() + 1);
            depth.get() == 1
        });
        // A test that panicked while holding the lock must not fail the rest.
        Self(outermost.then(|| {
            GPU_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }))
    }
}

impl Drop for GpuLockGuard {
    fn drop(&mut self) {
        self.0.take();
        GPU_LOCK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

/// A renderer plus the GPU lock; the renderer drops first, then the lock.
struct TestRenderer {
    renderer: GpuRenderer,
    _lock: GpuLockGuard,
}

impl TestRenderer {
    fn with_asset_root(mut self, asset_root: impl Into<PathBuf>) -> Self {
        self.renderer = self.renderer.with_asset_root(asset_root);
        self
    }

    fn with_video_decoder(mut self, decoder: impl VideoFrameDecoder + 'static) -> Self {
        self.renderer = self.renderer.with_video_decoder(decoder);
        self
    }
}

impl Deref for TestRenderer {
    type Target = GpuRenderer;

    fn deref(&self) -> &GpuRenderer {
        &self.renderer
    }
}

impl DerefMut for TestRenderer {
    fn deref_mut(&mut self) -> &mut GpuRenderer {
        &mut self.renderer
    }
}

/// `CELESTA_TEST_DRIVER` (e.g. `dx12`) runs the GPU tests through that driver.
fn renderer(mut options: GpuRenderOptions) -> Option<TestRenderer> {
    if let Ok(driver) = std::env::var("CELESTA_TEST_DRIVER") {
        options.driver = driver.parse().expect("CELESTA_TEST_DRIVER names a driver");
    }
    let lock = GpuLockGuard::acquire();
    match GpuRenderer::new(options) {
        Ok(renderer) => Some(TestRenderer {
            renderer,
            _lock: lock,
        }),
        Err(GpuRenderError::RequestAdapter(error)) => {
            assert!(
                std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
                "GPU tests require an adapter: {error}"
            );
            eprintln!("skipping GPU test because no adapter is available: {error}");
            None
        }
        Err(error) => panic!("could not initialize GPU renderer: {error}"),
    }
}

#[test]
fn parses_every_driver_name() {
    for &driver in GpuDriver::VARIANTS {
        assert_eq!(driver.as_str().parse(), Ok(driver));
        assert_eq!(driver.to_string(), driver.as_str());
    }
    assert_eq!(
        "d3d12".parse::<GpuDriver>().unwrap_err().to_string(),
        "unknown driver 'd3d12' (expected auto, vulkan, dx12, metal, gl)"
    );
    assert!(GpuDriver::Auto.is_built_in());
}

#[test]
fn pads_readback_rows_to_the_gpu_copy_alignment() {
    let layout = ReadbackLayout::new(3, 2).unwrap();
    assert_eq!(layout.unpadded_bytes_per_row, 12);
    assert_eq!(layout.padded_bytes_per_row, 256);
    assert_eq!(layout.buffer_size, 512);
}

#[test]
fn unpads_aligned_and_padded_readback_rows() {
    let aligned = ReadbackLayout::new(64, 2).unwrap();
    let mapped: Vec<u8> = (0..=255).cycle().take(512).collect();
    assert_eq!(aligned.unpad(&mapped, 64, 2).unwrap(), mapped);

    let padded = ReadbackLayout::new(3, 2).unwrap();
    let mut mapped = vec![0; 512];
    mapped[..12].fill(1);
    mapped[256..268].fill(2);
    let pixels = padded.unpad(&mapped, 3, 2).unwrap();
    assert_eq!(pixels.len(), 24);
    assert!(pixels[..12].iter().all(|byte| *byte == 1));
    assert!(pixels[12..].iter().all(|byte| *byte == 2));
}

/// A 2x2 image layer whose pixels are seeded straight into the renderer's
/// decoded-image map, so no file is read.
fn seeded_image(renderer: &mut GpuRenderer, id: &str, x: f64, rgba: [u8; 4]) -> Layer {
    renderer.image_sources.insert_raster(
        id,
        image::RgbaImage::from_raw(2, 2, rgba.repeat(4)).unwrap(),
    );
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position: Point { x, y: 1.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            width: None,
            height: None,
            fit: None,
            animation: None,
            asset: ResolvedAsset {
                id: id.to_owned(),
                location: AssetLocation::File {
                    path: format!("{id}.png"),
                },
            },
        },
    }
}

#[test]
fn rasterizes_a_frames_new_text_in_parallel_like_one_at_a_time() {
    let text_layer = |index: usize, text: &str| Layer {
        id: format!("label-{index}"),
        transform: EvaluatedTransform {
            position: Point {
                x: 8.0,
                y: 4.0 + index as f64 * 32.0,
            },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: text.to_owned(),
            style: Box::new(TextStyle {
                font_size: Some(16.0),
                fill: Some(Paint::Solid {
                    color: "#ffffff".to_owned(),
                }),
                ..TextStyle::default()
            }),
            max_width: None,
            baseline_anchor: false,
        },
    };
    // The repeat shares a texture with the first label.
    let labels = ["CH00 +1.000", "CH01 -2.500", "CH02 +3.750", "CH00 +1.000"];
    let scene = |labels: &[(usize, &str)]| {
        let mut scene = empty_scene(160, 136);
        scene.layers = labels
            .iter()
            .map(|&(index, text)| text_layer(index, text))
            .collect();
        scene
    };
    let all: Vec<_> = labels.iter().copied().enumerate().collect();
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };

    // Every label is new here, so they rasterize on forks in parallel.
    let together = renderer.render(&scene(&all)).unwrap();
    // White labels on black that do not overlap: drawn one per frame,
    // each by the renderer's own rasterizer, the brightest of the frames
    // is the frame with all of them.
    let mut one_at_a_time = vec![0_u8; together.pixels().len()];
    for label in &all {
        // An empty frame evicts every cached text texture first.
        renderer.render(&scene(&[])).unwrap();
        let frame = renderer
            .render(&scene(std::slice::from_ref(label)))
            .unwrap();
        for (merged, &value) in one_at_a_time.iter_mut().zip(frame.pixels()) {
            *merged = (*merged).max(value);
        }
    }

    assert!(together.pixels().contains(&255));
    assert!(together.pixels() == one_at_a_time.as_slice());
}

#[test]
fn reuses_unchanged_layer_textures_across_frames_and_evicts_unused_ones() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let scene = |layers: Vec<Layer>| {
        let mut scene = empty_scene(4, 2);
        scene.layers = layers;
        scene
    };
    let pixel = |frame: &GpuFrame, x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();

    let first = scene(vec![
        seeded_image(&mut renderer, "red", 1.0, [255, 0, 0, 255]),
        seeded_image(&mut renderer, "blue", 3.0, [0, 0, 255, 255]),
    ]);
    let frame = renderer.render(&first).unwrap();
    assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 3), [0, 0, 255, 255]);
    assert_eq!(renderer.textures.len(), 2);

    // Another image in the same place is a different texture; the blue
    // one this frame no longer uses is dropped.
    let second = scene(vec![
        seeded_image(&mut renderer, "red", 1.0, [255, 0, 0, 255]),
        seeded_image(&mut renderer, "green", 3.0, [0, 255, 0, 255]),
    ]);
    let frame = renderer.render(&second).unwrap();
    assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 3), [0, 255, 0, 255]);
    assert_eq!(renderer.textures.len(), 2);
    assert!(
        !renderer
            .textures
            .keys()
            .any(|key| key.starts_with("image\0blue\0"))
    );

    // Cached textures render the same frame again, including through
    // the pipelined readback path.
    let again = renderer.render(&second).unwrap();
    assert_eq!(again, frame);
    assert!(renderer.submit(&second).unwrap().is_none());
    assert_eq!(renderer.drain().unwrap(), vec![frame]);

    // Rects are shaded on the GPU and never occupy a texture.
    let rects = scene(vec![
        solid_rect("red", 1.0, 2.0, 2.0, "#ff0000"),
        solid_rect("blue", 3.0, 2.0, 2.0, "#0000ff"),
    ]);
    let frame = renderer.render(&rects).unwrap();
    assert_eq!(pixel(&frame, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&frame, 3), [0, 0, 255, 255]);
    assert!(renderer.textures.is_empty());
}

#[test]
fn batches_consecutive_rects_into_one_draw_in_painter_order() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut scene = empty_scene(4, 2);
    scene.layers = vec![
        solid_rect("red", 2.0, 4.0, 2.0, "#ff0000"),
        solid_rect("green", 2.5, 3.0, 2.0, "#00ff00"),
        // Over both rects' right half, then covered again on the right.
        seeded_image(&mut renderer, "white", 2.0, [255, 255, 255, 255]),
        solid_rect("blue", 3.5, 1.0, 2.0, "#0000ff"),
        solid_rect("half", 3.5, 1.0, 2.0, "#ff000080"),
    ];

    let draws = renderer.prepare_draws(&scene).unwrap();
    assert!(draws.composite.is_none());
    let runs: Vec<_> = draws
        .steps
        .iter()
        .map(|step| match step {
            GpuStep::Draw(draw) => draw.instances.clone(),
            _ => unreachable!("a frame without blend modes only draws"),
        })
        .collect();
    assert_eq!(runs, [0..2, 2..3, 3..5]);

    let frame = renderer.render(&scene).unwrap();
    let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
    assert_eq!(pixel(0), [255, 0, 0, 255]);
    assert_eq!(pixel(1), [255, 255, 255, 255]);
    assert_eq!(pixel(2), [255, 255, 255, 255]);
    assert_eq!(pixel(3), [128, 0, 127, 255]);
}

fn blend_rect(id: &str, x: f64, y: f64, size: f64, color: &str, blend_mode: BlendMode) -> Layer {
    Layer {
        blend_mode,
        transform: EvaluatedTransform {
            position: Point { x, y },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        ..solid_rect(id, 0.0, size, size, color)
    }
}

/// A blended draw copies only the part of its canvas under it to the
/// backdrop, so that part must cover every pixel its quad shades: a
/// filtered (scaled or rotated) layer's quad reaches one source texel
/// past its edge, which is several pixels once it is magnified. A
/// renderer whose backdrop still holds an earlier frame must draw the
/// same frame as a fresh one.
#[test]
fn blends_a_magnified_layer_against_its_whole_backdrop() {
    let options = GpuRenderOptions {
        background: Color::rgba(10, 20, 30, 255),
        ..GpuRenderOptions::default()
    };
    let (Some(mut fresh), Some(mut reused)) = (renderer(options), renderer(options)) else {
        return;
    };
    let mut earlier = empty_scene(64, 64);
    earlier.layers = vec![blend_rect(
        "white",
        0.0,
        0.0,
        64.0,
        "#ffffff",
        BlendMode::Screen,
    )];
    reused.render(&earlier).unwrap();

    let mut scene = empty_scene(64, 64);
    let mut magnified = blend_rect("magnified", 0.0, 0.0, 4.0, "#40a0ff", BlendMode::Difference);
    magnified.transform = EvaluatedTransform {
        position: Point { x: 32.0, y: 32.0 },
        anchor: Point { x: 0.5, y: 0.5 },
        rotation: 20.0,
        scale: Point { x: 6.0, y: 6.0 },
        ..Default::default()
    };
    scene.layers = vec![
        blend_rect("left", 0.0, 0.0, 32.0, "#203040", BlendMode::Normal),
        blend_rect("right", 32.0, 0.0, 32.0, "#c08040", BlendMode::Normal),
        blend_rect("bottom", 0.0, 32.0, 64.0, "#608060", BlendMode::Normal),
        magnified,
    ];
    let expected = fresh.render(&scene).unwrap();
    let frame = reused.render(&scene).unwrap();
    let difference = frame
        .pixels()
        .iter()
        .zip(expected.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert_eq!(difference, 0, "the earlier frame's backdrop shows through");
}

fn blend_group(opacity: f64, blend_mode: BlendMode, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "group".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity,
        blend_mode,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: None,
            motion_blur: false,
        },
    }
}

const BLEND_MODES: [BlendMode; 17] = [
    BlendMode::Normal,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Add,
    BlendMode::Difference,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

/// Every blend mode, at partial opacity and with translucent fills, on
/// its own and inside nested isolated groups, over stripes of dark, light
/// and saturated backdrops.
fn blend_scene() -> Scene {
    let width = BLEND_MODES.len() as u32 * 4 + 4;
    let mut scene = empty_scene(width, 16);
    let backdrops = ["#202830", "#e0d0c0", "#c04080", "#30a0e0"];
    scene.layers = (0..width / 2)
        .map(|stripe| Layer {
            transform: EvaluatedTransform {
                position: Point {
                    x: f64::from(stripe * 2),
                    y: 0.0,
                },
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            ..solid_rect("stripe", 0.0, 2.0, 16.0, backdrops[stripe as usize % 4])
        })
        .collect();
    scene.layers.push(Layer {
        transform: EvaluatedTransform {
            position: Point { x: 0.0, y: 4.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        ..solid_rect("warm", 0.0, f64::from(width), 6.0, "#ff4d1f80")
    });
    for (index, mode) in BLEND_MODES.into_iter().enumerate() {
        let x = index as f64 * 4.0;
        let mut layer = blend_rect("mode", x, 2.0, 5.0, "#6ab04cc0", mode);
        layer.opacity = 0.8;
        scene.layers.push(layer);
        scene.layers.push(blend_group(
            0.7,
            mode,
            vec![
                blend_rect("under", x, 9.0, 4.0, "#3c6382", BlendMode::Normal),
                blend_rect("over", x + 1.0, 10.0, 4.0, "#f8c291a0", BlendMode::Screen),
                blend_group(
                    1.0,
                    BlendMode::Difference,
                    vec![blend_rect(
                        "nested",
                        x + 2.0,
                        12.0,
                        3.0,
                        "#ffffff",
                        BlendMode::Normal,
                    )],
                ),
            ],
        ));
    }
    scene
}

#[test]
fn blends_layers_and_isolated_groups_like_the_cpu_renderer() {
    let background = Color::rgba(10, 20, 30, 255);
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let scene = blend_scene();
    let draws = renderer.prepare_draws(&scene).unwrap();
    assert!(draws.composite.is_some());

    let expected = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: celesta_renderer::Color::rgba(10, 20, 30, 255),
    })
    .render(&scene)
    .unwrap();
    let width = scene.width as usize;
    let hue = BLEND_MODES
        .iter()
        .position(|mode| *mode == BlendMode::Hue)
        .unwrap()
        * 4;
    // Twice: the second frame reuses the canvases the first created.
    for _ in 0..2 {
        let frame = renderer.render(&scene).unwrap();
        for (index, (gpu, cpu)) in frame
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.pixels().as_chunks::<4>().0.iter())
            .enumerate()
        {
            let (x, y) = (index % width, index / width);
            // `Hue` keeps the hue of the group's own color, which `over`
            // screens nearly gray (about #b3adaf), and SetSat divides by
            // that color's tiny saturation: one rounding step of the
            // group's canvas turns the hue a long way. Elsewhere, and in
            // every other mode, the two renderers agree closely.
            let tolerance = if (hue..hue + 5).contains(&x) && y >= 9 {
                16
            } else {
                2
            };
            let close = gpu
                .iter()
                .zip(cpu)
                .all(|(gpu, cpu)| gpu.abs_diff(*cpu) <= tolerance);
            assert!(close, "pixel ({x}, {y}): GPU {gpu:?}, CPU {cpu:?}");
        }
    }

    // A preview target of another size and format gets the same frame,
    // scaled into its viewport.
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 48,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render_to_target(
            &scene,
            GpuRenderTarget {
                view: &view,
                format: wgpu::TextureFormat::Bgra8Unorm,
                width: 48,
                height: 32,
            },
        )
        .unwrap();
}

/// A scene fading in over another, as a cross-fade draws it: its backing,
/// a card over it, a nested translucent group, a lone shape, and a lone
/// blended child, each in a group at partial opacity.
fn fade_scene() -> Scene {
    let mut scene = empty_scene(24, 16);
    scene.layers = vec![
        blend_rect("previous", 0.0, 0.0, 24.0, "#22314f", BlendMode::Normal),
        blend_group(
            0.5,
            BlendMode::Normal,
            vec![
                blend_rect("backing", 0.0, 0.0, 24.0, "#f2e8d5", BlendMode::Normal),
                blend_rect("card", 2.0, 2.0, 8.0, "#c0392b", BlendMode::Normal),
                blend_group(
                    0.6,
                    BlendMode::Normal,
                    vec![
                        blend_rect("under", 12.0, 2.0, 6.0, "#3c6382", BlendMode::Normal),
                        blend_rect("over", 14.0, 4.0, 6.0, "#f8c291a0", BlendMode::Normal),
                    ],
                ),
                blend_group(
                    0.7,
                    BlendMode::Normal,
                    vec![blend_rect(
                        "lone",
                        2.0,
                        11.0,
                        4.0,
                        "#6ab04c",
                        BlendMode::Normal,
                    )],
                ),
                blend_group(
                    0.7,
                    BlendMode::Normal,
                    vec![blend_rect(
                        "blended",
                        8.0,
                        11.0,
                        4.0,
                        "#e0e0e0",
                        BlendMode::Difference,
                    )],
                ),
            ],
        ),
    ];
    scene
}

#[test]
fn fades_translucent_groups_as_one_layer_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(10, 20, 30, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let scene = fade_scene();
    let draws = renderer.prepare_draws(&scene).unwrap();
    assert!(draws.composite.is_some(), "the groups are not isolated");

    let expected = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: celesta_renderer::Color::rgba(10, 20, 30, 255),
    })
    .render(&scene)
    .unwrap();
    let frame = renderer.render(&scene).unwrap();
    for (index, (gpu, cpu)) in frame
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.pixels().as_chunks::<4>().0.iter())
        .enumerate()
    {
        let close = gpu
            .iter()
            .zip(cpu)
            .all(|(gpu, cpu)| gpu.abs_diff(*cpu) <= 2);
        assert!(
            close,
            "pixel ({}, {}): GPU {gpu:?}, CPU {cpu:?}",
            index % 24,
            index / 24,
        );
    }
    // The card covers the backing: half of it over the previous scene.
    let card = &frame.pixels()[(4 * 24 + 4) * 4..][..4];
    for (channel, (card, expected)) in card.iter().zip([113, 53, 61]).enumerate() {
        assert!(
            card.abs_diff(expected) <= 2,
            "card channel {channel}: {card}, not {expected}"
        );
    }
}

#[test]
fn shades_rects_like_the_cpu_rasterizer() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(10, 20, 30, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let stroke = |color: &str, width: f64| Stroke {
        paint: Paint::Solid {
            color: color.to_owned(),
        },
        width,
    };
    let fill = |color: &str| {
        Some(Paint::Solid {
            color: color.to_owned(),
        })
    };
    let stops = |stops: &[(f64, &str)]| -> Vec<celesta_composition::GradientStop> {
        stops
            .iter()
            .map(|(offset, color)| celesta_composition::GradientStop {
                offset: *offset,
                color: (*color).to_owned(),
            })
            .collect()
    };
    let linear = |(x1, y1, x2, y2): (f64, f64, f64, f64), list: &[(f64, &str)]| {
        Some(Paint::Linear {
            start: Point { x: x1, y: y1 },
            end: Point { x: x2, y: y2 },
            stops: stops(list),
        })
    };
    let radial = |(x, y, radius): (f64, f64, f64), list: &[(f64, &str)]| {
        Some(Paint::Radial {
            center: Point { x, y },
            radius,
            stops: stops(list),
        })
    };
    // (width, height, corner radius, fill, stroke, rotation, scale, opacity)
    let cases = [
        (12.0, 7.0, 0.0, fill("#ff8000"), None, 0.0, 1.0, 1.0),
        (10.3, 5.6, 0.0, fill("#ff800080"), None, 0.0, 1.0, 0.6),
        (31.7, 1.8, 0.0, fill("#EF402B"), None, 23.5, 1.0, 0.8),
        (40.0, 24.0, 9.0, fill("#2060ff"), None, -61.0, 1.0, 1.0),
        (
            36.4,
            20.2,
            6.5,
            fill("#ffffff"),
            Some(stroke("#ff0000c0", 2.5)),
            12.0,
            1.7,
            0.9,
        ),
        (
            28.0,
            18.0,
            4.0,
            None,
            Some(stroke("#00ff00", 1.5)),
            0.0,
            1.0,
            1.0,
        ),
        (0.4, 9.0, 0.0, fill("#ffff00"), None, 45.0, 3.0, 1.0),
        // Thinner than a pixel, or empty: covered no more than they are wide.
        (0.3, 9.0, 0.0, fill("#ffff00"), None, 0.0, 1.0, 1.0),
        (0.0, 9.0, 0.0, fill("#ffff00"), None, 17.0, 1.0, 1.0),
        // A stroke reaching the centre from every side hides the fill.
        (
            5.0,
            5.0,
            0.0,
            fill("#ffffff"),
            Some(stroke("#ff0000", 2.5)),
            0.0,
            1.0,
            1.0,
        ),
        // Gradients, shaded from the stops rather than rasterized.
        (
            40.0,
            24.0,
            6.0,
            linear(
                (0.0, 0.0, 0.0, 24.0),
                &[(0.0, "#102040"), (0.6, "#c07090"), (1.0, "#f0c090")],
            ),
            None,
            0.0,
            1.0,
            1.0,
        ),
        (
            30.0,
            30.0,
            15.0,
            radial(
                (12.0, 10.0, 18.0),
                &[(0.0, "#ffe8c0ff"), (0.3, "#ffb07880"), (1.0, "#ffb07800")],
            ),
            None,
            30.0,
            1.5,
            0.8,
        ),
        (
            36.0,
            20.0,
            4.0,
            linear(
                (0.0, 0.0, 36.0, 20.0),
                &[(0.0, "#2060ff"), (1.0, "#ff6020")],
            ),
            Some(Stroke {
                paint: linear(
                    (36.0, 0.0, 0.0, 0.0),
                    &[(0.0, "#ffffffc0"), (1.0, "#00ff8040")],
                )
                .unwrap(),
                width: 3.0,
            }),
            -12.0,
            1.0,
            0.9,
        ),
        (
            // A hard edge: two stops at one offset, and stops out of order.
            24.0,
            12.0,
            0.0,
            linear(
                (0.0, 0.0, 24.0, 0.0),
                &[
                    (1.0, "#0000ff"),
                    (0.5, "#00ff00"),
                    (0.5, "#ff0000"),
                    (0.0, "#000000"),
                ],
            ),
            None,
            0.0,
            1.0,
            1.0,
        ),
    ];
    // Each case also squashed vertically, for a non-uniform scale.
    let cases = cases
        .iter()
        .flat_map(|case| [(case.clone(), 1.0), (case.clone(), 0.6)]);
    for (index, ((width, height, corner_radius, fill, stroke, rotation, scale, opacity), squash)) in
        cases.enumerate()
    {
        let transform = EvaluatedTransform {
            position: Point { x: 32.3, y: 29.6 },
            rotation,
            scale: Point {
                x: scale,
                y: scale * squash,
            },
            ..EvaluatedTransform::default()
        };
        let rect = Layer {
            id: "rect".to_owned(),
            transform,
            opacity,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Rect {
                width,
                height,
                fill: fill.clone(),
                stroke: stroke.clone(),
                corner_radius,
            },
        };
        let mut scene = empty_scene(64, 60);
        scene.layers = vec![rect];
        assert_rect_matches_cpu(&mut renderer, &scene, path_transform(&transform), &index);
    }
}

#[test]
fn shades_sheared_rects_like_the_cpu_rasterizer() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(10, 20, 30, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    // A rect rotated inside a group scaled non-uniformly is sheared: its
    // local axes are no longer perpendicular on the canvas, so each rounded
    // corner's anti-aliasing depends on which corner it is.
    let rounded = LayerContent::Rect {
        width: 30.5,
        height: 18.25,
        fill: Some(Paint::Solid {
            color: "#ffffff".to_owned(),
        }),
        stroke: Some(Stroke {
            paint: Paint::Solid {
                color: "#ff4000c0".to_owned(),
            },
            width: 2.5,
        }),
        corner_radius: 7.0,
    };
    // Thinner than a pixel: its coverage is capped by its width across the
    // leaning edges.
    let thin = LayerContent::Rect {
        width: 0.3,
        height: 24.0,
        fill: Some(Paint::Solid {
            color: "#ffffff".to_owned(),
        }),
        stroke: None,
        corner_radius: 0.0,
    };
    let cases = [(30.0, (1.6, 0.7)), (-55.0, (0.8, 1.9))]
        .into_iter()
        .flat_map(|case| [(case, rounded.clone()), (case, thin.clone())]);
    for (index, ((rotation, group_scale), content)) in cases.enumerate() {
        let group_transform = EvaluatedTransform {
            position: Point { x: 31.7, y: 30.2 },
            scale: Point {
                x: group_scale.0,
                y: group_scale.1,
            },
            ..EvaluatedTransform::default()
        };
        let rect_transform = EvaluatedTransform {
            rotation,
            ..EvaluatedTransform::default()
        };
        let mut scene = empty_scene(64, 60);
        scene.layers = vec![Layer {
            id: "group".to_owned(),
            transform: group_transform,
            opacity: 0.8,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Group {
                layers: vec![Layer {
                    id: "rect".to_owned(),
                    transform: rect_transform,
                    opacity: 0.9,
                    blend_mode: BlendMode::Normal,
                    effects: Default::default(),
                    content,
                }],
                clip: None,
                mask: None,
                motion_blur: false,
            },
        }];
        let layer = LayerState::default()
            .then(&group_transform, 1.0)
            .then(&rect_transform, 1.0)
            .transform
            .into();
        assert_rect_matches_cpu(&mut renderer, &scene, layer, &format!("sheared {index}"));
    }
}

/// Renders `scene`, whose only visible layer is a rect drawn through
/// `layer` (its layer transform, without the anchor), and compares it with
/// `celesta_renderer::rasterize_rect_transformed` over the background
/// (10, 20, 30).
fn assert_rect_matches_cpu(
    renderer: &mut GpuRenderer,
    scene: &Scene,
    layer: celesta_renderer::PathTransform,
    case: &dyn std::fmt::Display,
) {
    let shaded = renderer.render(scene).unwrap();
    /// The rect inside `layer`, and its opacity through every group.
    fn rect(layer: &Layer) -> (&Layer, f64) {
        match &layer.content {
            LayerContent::Group { layers, .. } => {
                let (rect, opacity) = rect(&layers[0]);
                (rect, layer.opacity * opacity)
            }
            _ => (layer, layer.opacity),
        }
    }
    let (rect, opacity) = rect(&scene.layers[0]);
    let LayerContent::Rect {
        width,
        height,
        fill,
        stroke,
        corner_radius,
    } = &rect.content
    else {
        unreachable!("the scene draws a rect");
    };
    // The rect's box `[0, width] x [0, height]`, its anchor on the layer's
    // position.
    let anchor = rect.transform.anchor;
    let (anchor_x, anchor_y) = (-anchor.x * width, -anchor.y * height);
    let placement = celesta_renderer::PathTransform {
        tx: layer.tx + layer.a * anchor_x + layer.c * anchor_y,
        ty: layer.ty + layer.b * anchor_x + layer.d * anchor_y,
        ..layer
    };
    let paint = celesta_renderer::resolve_rect_paint(fill.as_ref(), stroke.as_ref()).unwrap();
    let rasterized = celesta_renderer::rasterize_rect_transformed(
        *width,
        *height,
        *corner_radius,
        &paint,
        placement,
        scene.width,
        scene.height,
    )
    .unwrap();
    let mut expected: Vec<u8> = (0..scene.width * scene.height)
        .flat_map(|_| [10, 20, 30, 255])
        .collect();
    let image = &rasterized.image;
    for (offset, texel) in image.pixels().as_chunks::<4>().0.iter().enumerate() {
        let x = rasterized.left as usize + offset % image.width() as usize;
        let y = rasterized.top as usize + offset / image.width() as usize;
        let pixel = &mut expected[(y * scene.width as usize + x) * 4..][..3];
        let alpha = f64::from(texel[3]) / 255.0 * opacity;
        for (channel, value) in pixel.iter_mut().zip(texel) {
            *channel =
                (f64::from(*value) * alpha + f64::from(*channel) * (1.0 - alpha)).round() as u8;
        }
    }
    // The shader works in f32 and the rasterizer in f64, so an edge's
    // coverage, a gradient's color or a stroke's blend of the two colors
    // can land on a rounding tie in one and just miss it in the other.
    // At an edge pixel the color and the coverage can each be one code
    // off, and blending onto the background compounds them into two
    // (seen on lavapipe). A misplaced edge differs by far more.
    let max_difference = shaded
        .pixels()
        .iter()
        .zip(&expected)
        .map(|(shaded, expected)| shaded.abs_diff(*expected))
        .max()
        .unwrap();
    assert!(
        max_difference <= 2,
        "case {case}: channels differ by up to {max_difference}"
    );
}

#[test]
fn shades_rects_in_scene_pixels_through_a_scaled_preview_viewport() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    // A 32x16 scene letterboxed into a 64x64 target: scaled by 2 into the
    // viewport at y 16..48.
    let mut scene = empty_scene(32, 16);
    scene.layers = vec![corner_rect("rect", 4.5, 4.0, 10.0, 6.0, "#ffffff")];
    let (width, height) = (64, 64);
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta viewport test target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render_to_target(
            &scene,
            GpuRenderTarget {
                view: &view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                width,
                height,
            },
        )
        .unwrap();
    let layout = ReadbackLayout::new(width, height).unwrap();
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Celesta viewport test readback"),
        size: layout.buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(layout.padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    renderer.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let pixels = layout.unpad(&mapped, width, height).unwrap();
    let red = |x: u32, y: u32| pixels[((y * width + x) * 4) as usize];

    // The rect covers scene x 4.5..14.5, y 4..10: target x 9..29, y 24..36,
    // anti-aliased over a scene pixel (two target pixels). Sampled 0.75
    // scene pixels inside and outside each edge.
    for (x, y) in [(19, 30), (10, 25), (27, 34)] {
        assert_eq!(red(x, y), 255, "inside at {x}, {y}");
    }
    for (x, y) in [(7, 30), (30, 30), (19, 22), (19, 37)] {
        assert_eq!(red(x, y), 0, "outside at {x}, {y}");
    }
}

/// A rect at `(x, y)` in its parent, top-left anchored.
fn corner_rect(id: &str, x: f64, y: f64, width: f64, height: f64, color: &str) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position: Point { x, y },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
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

fn clipped_group(transform: EvaluatedTransform, clip: Clip, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "clipped".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: Some(clip),
            mask: None,
            motion_blur: false,
        },
    }
}

fn pixel_at(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

fn max_channel_difference(gpu: &GpuFrame, cpu: &celesta_renderer::RgbaFrame) -> u8 {
    assert_eq!(gpu.pixels().len(), cpu.pixels().len());
    gpu.pixels()
        .iter()
        .zip(cpu.pixels())
        .map(|(gpu, cpu)| gpu.abs_diff(*cpu))
        .max()
        .unwrap()
}

#[test]
fn blurred_group_with_shadow_and_glow_matches_cpu() {
    use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(64, 64);
    scene.layers = vec![Layer {
        id: "fx".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 0.8,
        blend_mode: BlendMode::Screen,
        effects: LayerEffects {
            blur: 1.5,
            shadow: Some(Box::new(LayerShadow {
                color: "#0000ffb0".to_owned(),
                blur: 2.0,
                offset_x: 5.5,
                offset_y: 3.25,
            })),
            glow: Some(Box::new(LayerGlow {
                color: "#ff000080".to_owned(),
                blur: 3.0,
            })),
            filters: Vec::new(),
        },
        content: LayerContent::Group {
            layers: vec![corner_rect("child", 20.0, 20.0, 16.0, 12.0, "#ffe080")],
            clip: None,
            mask: None,
            motion_blur: false,
        },
    }];
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    let difference = max_channel_difference(&gpu, &cpu);
    assert!(
        difference <= 5,
        "effect channels differ by up to {difference}"
    );
    assert_ne!(pixel_at(&gpu, 25, 25), pixel_at(&gpu, 0, 0));

    scene.layers[0].effects.blur = 0.0;
    scene.layers[0].effects.glow = None;
    scene.layers[0].effects.shadow.as_mut().unwrap().blur = 0.0;
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    let difference = max_channel_difference(&gpu, &cpu);
    assert!(
        difference <= 5,
        "zero-radius shadow differs by up to {difference}"
    );
}

/// Effects filter only around what their layers cover; small, nested,
/// empty, and edge-crossing effects must still match the CPU
/// renderer, frame after frame as the pooled canvases are reused.
#[test]
fn effects_on_small_layers_match_cpu() {
    use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let effect = |id: &str, effects: LayerEffects, layers: Vec<Layer>| Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects,
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: None,
            motion_blur: false,
        },
    };
    let glow = |color: &str, blur: f64| LayerEffects {
        glow: Some(Box::new(LayerGlow {
            color: color.to_owned(),
            blur,
        })),
        ..LayerEffects::default()
    };
    let mut hidden = corner_rect("hidden", 40.0, 40.0, 8.0, 8.0, "#ffffff");
    hidden.opacity = 0.0;
    let mut nested = effect(
        "nested",
        LayerEffects::default(),
        vec![corner_rect("inner", 84.0, 6.0, 8.0, 10.0, "#ff60c0")],
    );
    nested.blend_mode = BlendMode::Screen;
    let mut scene = empty_scene(96, 64);
    scene.layers = vec![
        corner_rect("backdrop", 0.0, 0.0, 96.0, 64.0, "#203040"),
        effect(
            "small",
            glow("#ffd060c0", 4.0),
            vec![corner_rect("small", 18.0, 30.0, 10.0, 6.0, "#80ffa0")],
        ),
        effect(
            "edge",
            LayerEffects {
                blur: 1.0,
                shadow: Some(Box::new(LayerShadow {
                    color: "#000000a0".to_owned(),
                    blur: 3.0,
                    offset_x: 7.5,
                    offset_y: -4.25,
                })),
                glow: None,
                filters: Vec::new(),
            },
            vec![nested],
        ),
        effect("empty", glow("#ff0000ff", 6.0), vec![hidden]),
        effect(
            "second",
            glow("#60a0ffff", 2.5),
            vec![corner_rect("dot", 50.0, 50.0, 4.0, 4.0, "#ffffff")],
        ),
    ];
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    for frame in 0..3 {
        let gpu = renderer.render(&scene).unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "frame {frame}: effect channels differ by up to {difference}"
        );
    }
}

/// Large blurs pair their taps into bilinear fetches; the result must
/// stay within the CPU renderer's exact Gaussian, including where the
/// kernel runs off the canvas and under a fractional shadow offset.
#[test]
fn large_blurs_match_cpu() {
    use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let blob = |id: &str, effects: LayerEffects, layers: Vec<Layer>| Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 0.55,
        blend_mode: BlendMode::Screen,
        effects,
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: None,
            motion_blur: false,
        },
    };
    let mut scene = empty_scene(160, 112);
    scene.layers = vec![
        corner_rect("backdrop", 0.0, 0.0, 160.0, 112.0, "#101828"),
        blob(
            "blur",
            LayerEffects {
                blur: 24.0,
                ..LayerEffects::default()
            },
            vec![corner_rect("blur", 30.0, 20.0, 70.0, 50.0, "#ff40a0")],
        ),
        blob(
            "edge",
            LayerEffects {
                blur: 13.5,
                shadow: Some(Box::new(LayerShadow {
                    color: "#40c0ffd0".to_owned(),
                    blur: 17.0,
                    offset_x: -9.5,
                    offset_y: 6.75,
                })),
                glow: Some(Box::new(LayerGlow {
                    color: "#ffe060ff".to_owned(),
                    blur: 64.0,
                })),
                filters: Vec::new(),
            },
            vec![corner_rect("edge", 120.0, 70.0, 40.0, 42.0, "#a0ff60")],
        ),
    ];
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    let difference = max_channel_difference(&gpu, &cpu);
    assert!(
        difference <= 5,
        "large blur channels differ by up to {difference}"
    );

    // Tiny radii, where the Gaussian's tail underflows.
    for radius in [0.05, 0.3] {
        for layer in &mut scene.layers[1..] {
            layer.effects.blur = radius;
            if let Some(shadow) = &mut layer.effects.shadow {
                shadow.blur = radius;
            }
            if let Some(glow) = &mut layer.effects.glow {
                glow.blur = radius;
            }
        }
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "radius {radius}: channels differ by up to {difference}"
        );
    }
}

#[test]
fn clips_groups_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let scenes = [
        // A rounded clip in a translated group, with a translucent layer
        // straddling its edge.
        vec![clipped_group(
            EvaluatedTransform {
                position: Point { x: 4.0, y: 4.0 },
                ..EvaluatedTransform::default()
            },
            Clip {
                x: 8.0,
                y: 8.0,
                width: 40.0,
                height: 30.0,
                corner_radius: 10.0,
            },
            vec![
                corner_rect("red", 0.0, 0.0, 60.0, 60.0, "#FF0000FF"),
                Layer {
                    opacity: 0.5,
                    ..corner_rect("blue", 30.0, 20.0, 30.0, 30.0, "#0000FFFF")
                },
            ],
        )],
        // Nested clips inside a scaled group. The rect overhangs both
        // clips: the GPU filters a scaled layer's own edges where the CPU
        // renderer repeats texels, so only clip edges are compared.
        vec![clipped_group(
            EvaluatedTransform {
                scale: Point { x: 2.0, y: 2.0 },
                ..EvaluatedTransform::default()
            },
            Clip {
                x: 0.0,
                y: 0.0,
                width: 24.0,
                height: 24.0,
                corner_radius: 4.0,
            },
            vec![clipped_group(
                EvaluatedTransform {
                    position: Point { x: 10.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                Clip {
                    x: 0.0,
                    y: 0.0,
                    width: 30.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
                vec![corner_rect("red", -4.0, -4.0, 40.0, 40.0, "#FF0000FF")],
            )],
        )],
    ];
    for (index, layers) in scenes.into_iter().enumerate() {
        let mut scene = empty_scene(64, 64);
        scene.layers = layers;
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 2,
            "scene {index}: channels differ by up to {difference}"
        );
    }
}

#[test]
fn benchmark_path_transform_matches_prepared_layer() {
    use celesta_composition::{LineCap, LineJoin, PathCommand};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let layer = Layer {
        id: "transformed-path".to_owned(),
        transform: EvaluatedTransform {
            position: Point {
                x: 10.123,
                y: 24.456,
            },
            rotation: 37.123,
            scale: Point { x: 2.5, y: -0.75 },
            anchor: Point { x: 8.0, y: 12.0 },
            ..Default::default()
        },
        opacity: 0.7,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Path {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 16.0, y: 2.0 },
            ],
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Solid {
                    color: "#FFFFFF".to_owned(),
                },
                width: 1.5,
            }),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: 4.0,
            line_dash: None,
        },
    };
    renderer.scene_size = (64, 64);
    let mut items = Vec::new();
    renderer
        .prepare_layer(&layer, LayerState::default(), &mut items)
        .unwrap();
    renderer.resolve_pending_paths(&mut items).unwrap();
    let [
        PreparedItem::Layer(PreparedLayer {
            content: PreparedContent::Path(_),
            ..
        }),
    ] = items.as_slice()
    else {
        panic!("expected one path layer");
    };
    let LayerContent::Path {
        commands,
        fill,
        stroke,
        line_cap,
        line_join,
        miter_limit,
        line_dash,
    } = &layer.content
    else {
        unreachable!("the layer is a path");
    };
    let shape = PathShape {
        commands,
        fill: fill.as_ref(),
        stroke: stroke.as_ref(),
        line_cap: *line_cap,
        line_join: *line_join,
        miter_limit: *miter_limit,
        line_dash: line_dash.as_deref(),
    };
    let path = flatten_path(&shape, path_transform(&layer.transform), 64, 64)
        .unwrap()
        .expect("the path is visible");
    let mut entries = Vec::new();
    ShadedPath::new(&path, &mut Vec::new(), &mut entries, 1 << 24).unwrap();
    assert_eq!(renderer.path_entries, entries);
}

/// How far a path shaded on the GPU may stray from the CPU renderer's.
/// Both sample four scanlines per pixel row, but the CPU renderer rounds
/// where they cross edges to quarter pixels and flattens curves more
/// coarsely, so a pixel on an edge can be off by a few samples of 1/16
/// of its coverage. On the high-contrast, sub-pixel strokes and curves
/// below that is up to `PATH_MAX_DIFFERENCE` in a channel (59 at most
/// when measured), while across a frame the channels differ by under
/// `PATH_MEAN_DIFFERENCE` on average (0.43) and at most `PATH_FAR_SHARE`
/// of them by more than `PATH_CLOSE_DIFFERENCE` (0.59%). The GPU's are
/// the closer to exact: against an 8x supersampled CPU rendering, the
/// curves case differs by up to 28 (0.18 on average) on the GPU and 52
/// (0.54) on the CPU.
const PATH_MAX_DIFFERENCE: u8 = 64;
const PATH_MEAN_DIFFERENCE: f64 = 0.5;
const PATH_CLOSE_DIFFERENCE: u8 = 16;
const PATH_FAR_SHARE: f64 = 0.01;

fn assert_paths_match(case: &str, gpu: &[u8], cpu: &[u8]) {
    assert_eq!(gpu.len(), cpu.len());
    let differences: Vec<u8> = gpu.iter().zip(cpu).map(|(a, b)| a.abs_diff(*b)).collect();
    let max = differences.iter().copied().max().unwrap_or(0);
    let mean = differences.iter().map(|&d| f64::from(d)).sum::<f64>() / differences.len() as f64;
    let far = differences
        .iter()
        .filter(|&&d| d > PATH_CLOSE_DIFFERENCE)
        .count() as f64
        / differences.len() as f64;
    assert!(
        max <= PATH_MAX_DIFFERENCE && mean <= PATH_MEAN_DIFFERENCE && far <= PATH_FAR_SHARE,
        "{case}: channels differ by up to {max}, {mean:.3} on average, \
         {:.3}% by more than {PATH_CLOSE_DIFFERENCE}",
        far * 100.0
    );
}

// GPU canvases/readback are premultiplied; CPU frames are straight RGBA.
fn premultiplied_pixels(pixels: &[u8]) -> Vec<u8> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = u16::from(pixel[3]);
            [
                ((u16::from(pixel[0]) * alpha + 127) / 255) as u8,
                ((u16::from(pixel[1]) * alpha + 127) / 255) as u8,
                ((u16::from(pixel[2]) * alpha + 127) / 255) as u8,
                pixel[3],
            ]
        })
        .collect()
}

fn assert_paths_match_cpu(renderer: &mut GpuRenderer, case: &str, layers: Vec<Layer>) {
    let mut scene = empty_scene(96, 64);
    scene.layers = layers;
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    assert_paths_match(case, gpu.pixels(), cpu.pixels());
}

fn polyline(points: &[(f64, f64)], closed: bool) -> Vec<celesta_composition::PathCommand> {
    use celesta_composition::PathCommand;

    let mut commands: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(index, &(x, y))| match index {
            0 => PathCommand::MoveTo { x, y },
            _ => PathCommand::LineTo { x, y },
        })
        .collect();
    if closed {
        commands.push(PathCommand::Close);
    }
    commands
}

fn solid(color: &str) -> Paint {
    Paint::Solid {
        color: color.to_owned(),
    }
}

#[test]
fn path_entry_limits_respect_binding_buffer_and_float_precision() {
    let mut limits = wgpu::Limits::default();
    assert_eq!(path_entry_limit(&limits), 1 << 23);
    limits.max_buffer_size = 1023;
    assert_eq!(path_entry_limit(&limits), 63);
    limits.max_buffer_size = 1 << 30;
    limits.max_storage_buffer_binding_size = 1 << 30;
    assert_eq!(path_entry_limit(&limits), 1 << 24);
}

#[test]
fn tile_bins_preserve_full_outline_winding() {
    let winding = |edges: &[[f32; 4]], x: f64, y: f64| -> i32 {
        edges
            .iter()
            .map(|edge| {
                let [ax, ay, bx, by] = edge.map(f64::from);
                if (y < ay) == (y < by) || ax + (y - ay) * (bx - ax) / (by - ay) > x {
                    0
                } else if by > ay {
                    1
                } else {
                    -1
                }
            })
            .sum()
    };
    // Fixed seeds cover horizontal edges, tile boundaries and
    // self-intersections without a GPU adapter.
    for seed in 0..32 {
        let points: Vec<_> = (0..16)
            .map(|i| ((i * 17 + seed * 7) % 65, (i * 11 + seed * 13) % 33))
            .collect();
        let edges: Vec<_> = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(&(ax, ay), &(bx, by))| LineSegment {
                x0: ax as f32,
                y0: ay as f32,
                x1: bx as f32,
                y1: by as f32,
            })
            .collect();
        let bins = bin_tiles(&edges, 64, 32);
        let full: Vec<_> = edges.iter().map(|e| [e.x0, e.y0, e.x1, e.y1]).collect();
        for y in 0..128 {
            let sample_y = (f64::from(y) + 0.5) / 4.0;
            for x in 0..64 {
                let sample_x = f64::from(x) + 0.5;
                let index = (y / 32) * 8 + x / 8;
                let tiled = bins
                    .tiles
                    .iter()
                    .find(|tile| tile.index == index)
                    .map_or(0, |tile| {
                        tile.backdrop + winding(&bins.edges[tile.edges.clone()], sample_x, sample_y)
                    });
                assert_eq!(
                    tiled,
                    winding(&full, sample_x, sample_y),
                    "seed {seed}, ({sample_x}, {sample_y})"
                );
            }
        }
    }
}

#[test]
fn shaded_path_budget_rejection_preserves_both_buffers() {
    let paint = celesta_renderer::resolve_rect_paint(Some(&solid("#FFFFFF")), None)
        .unwrap()
        .fill
        .unwrap();
    let mut path = FlattenedPath {
        left: 0,
        top: 0,
        width: 16,
        height: 16,
        fill: Some((
            paint,
            vec![
                LineSegment {
                    x0: 1.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 16.0,
                },
                LineSegment {
                    x0: 15.0,
                    y0: 16.0,
                    x1: 15.0,
                    y1: 0.0,
                },
            ],
        )),
        stroke: None,
        inverse: PathTransform::scale_translate(1.0, 1.0, 0.0, 0.0),
    };
    let mut entries = vec![[7.0; 4]];
    let mut paints = vec![[8.0; 4]];
    assert!(ShadedPath::new(&path, &mut paints, &mut entries, 1).is_none());
    let edges = &mut path.fill.as_mut().unwrap().1;
    *edges = vec![edges[0]; MAX_PATH_TILE_EDGES + 1];
    assert!(ShadedPath::new(&path, &mut paints, &mut entries, 1 << 24).is_none());
    assert_eq!(entries, vec![[7.0; 4]]);
    assert_eq!(paints, vec![[8.0; 4]]);
}

#[test]
fn shades_top_boundary_vertices_on_transparent_backgrounds() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    for sides in [4, 6] {
        for epsilon in [0.0, 2.2e-14, 1e-9, 1e-7] {
            for reverse in [false, true] {
                let mut points: Vec<_> = (0..sides)
                    .map(|i| {
                        let angle = f64::from(i) * std::f64::consts::TAU / f64::from(sides);
                        (64.0 + 20.0 * angle.cos(), epsilon + 20.0 * angle.sin())
                    })
                    .collect();
                if reverse {
                    points.reverse();
                }
                let mut scene = empty_scene(128, 32);
                scene.layers = vec![path_layer(
                    "boundary",
                    polyline(&points, true),
                    Some(solid("#FFFFFF")),
                    None,
                )];
                let gpu = renderer.render(&scene).unwrap();
                let cpu = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
                    background: CpuColor::TRANSPARENT,
                })
                .render(&scene)
                .unwrap();
                assert_paths_match(
                    "top boundary",
                    gpu.pixels(),
                    &premultiplied_pixels(cpu.pixels()),
                );
                for x in 52..76 {
                    assert_eq!(pixel_at(&gpu, x, 0)[3], 255);
                }
            }
        }
    }
}

#[test]
fn shades_integer_holes_exactly_on_transparent_backgrounds() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut commands = polyline(&[(0.0, 0.0), (64.0, 0.0), (64.0, 32.0), (0.0, 32.0)], true);
    commands.extend(polyline(
        &[(8.0, 8.0), (8.0, 24.0), (56.0, 24.0), (56.0, 8.0)],
        true,
    ));
    let mut scene = empty_scene(64, 32);
    scene.layers = vec![path_layer("hole", commands, Some(solid("#FFFFFF")), None)];
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: CpuColor::TRANSPARENT,
    })
    .render(&scene)
    .unwrap();
    assert_eq!(gpu.pixels(), cpu.pixels());
}

#[test]
fn dense_zigzags_use_cpu_textures_and_leave_renderer_usable() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut scene = empty_scene(128, 64);
    scene.layers = vec![path_layer(
        "zigzag",
        polyline(
            &(0..4096)
                .map(|i| {
                    (
                        32.0 + f64::from(i) / 64.0,
                        if i % 2 == 0 { 12.0 } else { 52.0 },
                    )
                })
                .collect::<Vec<_>>(),
            false,
        ),
        None,
        Some((solid("#FFFFFF80"), 1.0)),
    )];
    let cpu = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: CpuColor::TRANSPARENT,
    })
    .render(&scene)
    .unwrap();
    for _ in 0..3 {
        let gpu = renderer.render(&scene).unwrap();
        assert!(
            renderer.path_entries.is_empty(),
            "dense layer must use a texture"
        );
        assert_paths_match(
            "dense fallback",
            gpu.pixels(),
            &premultiplied_pixels(cpu.pixels()),
        );
    }
    scene.layers = vec![path_layer(
        "simple",
        polyline(&[(8.0, 8.0), (24.0, 8.0), (24.0, 24.0), (8.0, 24.0)], true),
        Some(solid("#FFFFFF")),
        None,
    )];
    let gpu = renderer.render(&scene).unwrap();
    assert!(!renderer.path_entries.is_empty());
    assert_eq!(pixel_at(&gpu, 16, 16), [255; 4]);
}

#[test]
fn shrinks_path_fallback_textures_without_changing_output_bounds() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    renderer.max_texture_dimension = 16;
    for (width, height) in [(96, 32), (32, 96)] {
        let contour = polyline(
            &[
                (0.0, 0.0),
                (f64::from(width), 0.0),
                (f64::from(width), f64::from(height)),
                (0.0, f64::from(height)),
            ],
            true,
        );
        // Coincident contours force the tile-density fallback while
        // retaining a simple rectangle whose output bounds are exact.
        let commands = (0..=MAX_PATH_TILE_EDGES)
            .flat_map(|_| contour.iter().cloned())
            .collect();
        let mut layer = path_layer(
            "oversized-fallback",
            commands,
            Some(solid("#FFFFFF80")),
            None,
        );
        layer.transform.position = Point { x: 12.0, y: 8.0 };
        layer.opacity = 0.5;
        renderer.scene_size = (128, 128);
        renderer.path_entries.clear();
        let mut items = Vec::new();
        renderer
            .prepare_layer(&layer, LayerState::default(), &mut items)
            .unwrap();
        renderer.resolve_pending_paths(&mut items).unwrap();
        let [
            PreparedItem::Layer(PreparedLayer {
                content: PreparedContent::Texture(texture),
                state,
                ..
            }),
        ] = items.as_slice()
        else {
            panic!("dense path must use a texture");
        };
        assert_eq!(
            (texture.width, texture.height),
            fit_within(width, height, 16)
        );
        assert_eq!(state.transform.a * texture.width as f32, width as f32);
        assert_eq!(state.transform.d * texture.height as f32, height as f32);
        assert_eq!((state.transform.tx, state.transform.ty), (12.0, 8.0));
        let mut scene = empty_scene(128, 128);
        scene.layers = vec![layer];
        let frame = renderer.render(&scene).unwrap();
        assert_eq!(pixel_at(&frame, 12 + width / 2, 8 + height / 2), [64; 4]);
        // Enlarged texels have a filtered fringe; probe beyond it.
        assert_eq!(pixel_at(&frame, 4, 8 + height / 2), [0; 4]);
        assert_eq!(pixel_at(&frame, 20 + width, 8 + height / 2), [0; 4]);
        assert_eq!(pixel_at(&frame, 12 + width / 2, 0), [0; 4]);
        assert_eq!(pixel_at(&frame, 12 + width / 2, 16 + height), [0; 4]);
    }
}

#[test]
fn full_path_buffer_falls_back_for_only_the_next_layer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    renderer.scene_size = (64, 32);
    let limit = path_entry_limit(&renderer.device.limits());
    renderer.path_entries.resize(limit - 1, [0.0; 4]);
    let layer = path_layer(
        "overflow",
        polyline(&[(0.0, 0.0), (64.0, 0.0), (64.0, 32.0), (0.0, 32.0)], true),
        Some(solid("#FFFFFF")),
        None,
    );
    let mut items = Vec::new();
    renderer
        .prepare_layer(&layer, LayerState::default(), &mut items)
        .unwrap();
    renderer.resolve_pending_paths(&mut items).unwrap();
    assert!(matches!(
        items.as_slice(),
        [PreparedItem::Layer(PreparedLayer {
            content: PreparedContent::Texture(_),
            ..
        })]
    ));
    assert_eq!(renderer.path_entries.len(), limit - 1);
    // The following frame can shade normally again.
    let mut scene = empty_scene(64, 32);
    scene.layers = vec![layer];
    let frame = renderer.render(&scene).unwrap();
    assert!(renderer.path_entries.len() < limit);
    assert_eq!(pixel_at(&frame, 32, 16), [255; 4]);
}

fn path_layer(
    id: &str,
    commands: Vec<celesta_composition::PathCommand>,
    fill: Option<Paint>,
    stroke: Option<(Paint, f64)>,
) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Path {
            commands,
            fill,
            stroke: stroke.map(|(paint, width)| Stroke { paint, width }),
            line_cap: celesta_composition::LineCap::Butt,
            line_join: celesta_composition::LineJoin::Miter,
            miter_limit: celesta_composition::DEFAULT_MITER_LIMIT,
            line_dash: None,
        },
    }
}

fn styled(
    mut layer: Layer,
    cap: celesta_composition::LineCap,
    join: celesta_composition::LineJoin,
    limit: f64,
) -> Layer {
    if let LayerContent::Path {
        line_cap,
        line_join,
        miter_limit,
        ..
    } = &mut layer.content
    {
        (*line_cap, *line_join, *miter_limit) = (cap, join, limit);
    }
    layer
}

fn group(transform: EvaluatedTransform, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "group".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: None,
            motion_blur: false,
        },
    }
}

#[test]
fn shades_curves_and_closed_paths_like_the_cpu_renderer() {
    use celesta_composition::{LineCap, LineJoin, PathCommand};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let blob = vec![
        PathCommand::MoveTo { x: 8.0, y: 32.0 },
        PathCommand::CubicTo {
            x1: 8.0,
            y1: 4.0,
            x2: 50.0,
            y2: 2.0,
            x: 56.0,
            y: 28.0,
        },
        PathCommand::QuadTo {
            x1: 62.0,
            y1: 60.0,
            x: 30.0,
            y: 58.0,
        },
        PathCommand::Close,
    ];
    assert_paths_match_cpu(
        &mut renderer,
        "curves",
        vec![
            styled(
                path_layer(
                    "blob",
                    blob,
                    Some(solid("#3366FFAA")),
                    Some((solid("#FFCC00"), 2.5)),
                ),
                LineCap::Butt,
                LineJoin::Round,
                4.0,
            ),
            // A closed square's seam joins like its other corners.
            path_layer(
                "square",
                polyline(
                    &[(66.0, 8.0), (90.0, 8.0), (90.0, 32.0), (66.0, 32.0)],
                    true,
                ),
                None,
                Some((solid("#66FF99"), 3.0)),
            ),
            // The same square left open shows its butt ends instead.
            path_layer(
                "open",
                polyline(
                    &[
                        (66.0, 40.0),
                        (90.0, 40.0),
                        (90.0, 60.0),
                        (66.0, 60.0),
                        (66.0, 40.0),
                    ],
                    false,
                ),
                None,
                Some((solid("#FF6699"), 3.0)),
            ),
        ],
    );
}

#[test]
fn shades_joins_caps_and_zero_length_segments_like_the_cpu_renderer() {
    use celesta_composition::{LineCap, LineJoin};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let spike = |id: &str, y: f64, limit: f64| {
        styled(
            path_layer(
                id,
                polyline(&[(4.0, y), (60.0, y + 6.0), (4.0, y + 12.0)], false),
                None,
                Some((solid("#FF4040"), 3.0)),
            ),
            LineCap::Butt,
            LineJoin::Miter,
            limit,
        )
    };
    let dot = |id: &str, x: f64, cap: LineCap| {
        styled(
            path_layer(
                id,
                polyline(&[(x, 54.0), (x, 54.0)], false),
                None,
                Some((solid("#40C0FF"), 6.0)),
            ),
            cap,
            LineJoin::Miter,
            4.0,
        )
    };
    assert_paths_match_cpu(
        &mut renderer,
        "joins and caps",
        vec![
            // Past the miter limit, an acute join is beveled; within a
            // high one, it comes to a point.
            spike("beveled", 2.0, 4.0),
            spike("mitered", 18.0, 40.0),
            // A repeated point in the middle of a polyline.
            styled(
                path_layer(
                    "repeated",
                    polyline(
                        &[(66.0, 4.0), (80.0, 20.0), (80.0, 20.0), (92.0, 6.0)],
                        false,
                    ),
                    None,
                    Some((solid("#C0FF40"), 4.0)),
                ),
                LineCap::Square,
                LineJoin::Round,
                4.0,
            ),
            // Zero-length segments: round and square caps draw a dot,
            // butt caps nothing.
            dot("round", 70.0, LineCap::Round),
            dot("square", 80.0, LineCap::Square),
            dot("butt", 90.0, LineCap::Butt),
        ],
    );
}

#[test]
fn shades_dashed_and_dotted_strokes_like_the_cpu_renderer() {
    use celesta_composition::{LineCap, LineDash, LineJoin, PathCommand};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let dashed = |mut layer: Layer, pattern: &[f64], offset: f64| {
        if let LayerContent::Path { line_dash, .. } = &mut layer.content {
            *line_dash = Some(Box::new(LineDash {
                pattern: pattern.to_vec(),
                offset,
            }));
        }
        layer
    };
    let curve = vec![
        PathCommand::MoveTo { x: 6.0, y: 30.0 },
        PathCommand::CubicTo {
            x1: 20.0,
            y1: -6.0,
            x2: 44.0,
            y2: 62.0,
            x: 60.0,
            y: 24.0,
        },
    ];
    assert_paths_match_cpu(
        &mut renderer,
        "dashes",
        vec![
            // Dashes along a curve, a period and a half into the pattern.
            dashed(
                styled(
                    path_layer("curve", curve, None, Some((solid("#FFCC00"), 3.0))),
                    LineCap::Butt,
                    LineJoin::Miter,
                    4.0,
                ),
                &[7.0, 3.0],
                15.0,
            ),
            // Round dots around a closed square, whose last dash joins its
            // first at the seam, from an offset before the start.
            dashed(
                styled(
                    path_layer(
                        "dots",
                        polyline(
                            &[(66.0, 8.0), (90.0, 8.0), (90.0, 32.0), (66.0, 32.0)],
                            true,
                        ),
                        None,
                        Some((solid("#66FF99"), 4.0)),
                    ),
                    LineCap::Round,
                    LineJoin::Round,
                    4.0,
                ),
                &[0.0, 7.0],
                -2.0,
            ),
            // An odd pattern, repeated, under a translucent fill.
            dashed(
                path_layer(
                    "filled",
                    polyline(&[(66.0, 40.0), (90.0, 40.0), (78.0, 60.0)], true),
                    Some(solid("#3366FFAA")),
                    Some((solid("#FF6699"), 2.0)),
                ),
                &[5.0, 2.0, 1.0],
                0.0,
            ),
        ],
    );
}

#[test]
fn shades_thin_lines_and_scaled_strokes_like_the_cpu_renderer() {
    use celesta_composition::{LineCap, LineJoin, PathCommand};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let line = |id: &str, from: (f64, f64), to: (f64, f64), width: f64| {
        path_layer(
            id,
            polyline(&[from, to], false),
            None,
            Some((solid("#FFFFFF"), width)),
        )
    };
    assert_paths_match_cpu(
        &mut renderer,
        "thin and scaled",
        vec![
            line("shallow", (2.0, 4.0), (60.0, 14.0), 0.5),
            line("diagonal", (2.0, 16.0), (40.0, 60.0), 1.0),
            line("steep", (50.0, 20.0), (56.0, 62.0), 0.75),
            styled(
                path_layer(
                    "hair",
                    vec![
                        PathCommand::MoveTo { x: 2.0, y: 62.0 },
                        PathCommand::CubicTo {
                            x1: 20.0,
                            y1: 20.0,
                            x2: 40.0,
                            y2: 70.0,
                            x: 62.0,
                            y: 30.0,
                        },
                    ],
                    None,
                    Some((solid("#FFFFFF"), 0.4)),
                ),
                LineCap::Round,
                LineJoin::Bevel,
                4.0,
            ),
            // A non-uniform scale stretches the stroke with the layer,
            // and a negative one mirrors it.
            group(
                EvaluatedTransform {
                    position: Point { x: 64.5, y: 10.25 },
                    scale: Point { x: 2.5, y: 0.75 },
                    ..EvaluatedTransform::default()
                },
                vec![path_layer(
                    "stretched",
                    polyline(&[(0.0, 0.0), (10.0, 4.0), (6.0, 20.0), (1.0, 12.0)], true),
                    None,
                    Some((solid("#33CC66"), 1.5)),
                )],
            ),
            group(
                EvaluatedTransform {
                    position: Point { x: 92.0, y: 40.0 },
                    scale: Point { x: -1.0, y: 1.5 },
                    ..EvaluatedTransform::default()
                },
                vec![path_layer(
                    "mirrored",
                    polyline(&[(0.0, 0.0), (20.0, 4.0), (4.0, 14.0)], false),
                    None,
                    Some((solid("#FF9933"), 1.25)),
                )],
            ),
        ],
    );
}

#[test]
fn composites_translucent_strokes_and_fills_once_like_the_cpu_renderer() {
    use celesta_composition::{LineCap, LineJoin};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut eight = styled(
        path_layer(
            "eight",
            polyline(&[(4.0, 4.0), (40.0, 40.0), (40.0, 4.0), (4.0, 40.0)], true),
            None,
            Some((solid("#FF000080"), 6.0)),
        ),
        LineCap::Butt,
        LineJoin::Round,
        4.0,
    );
    eight.opacity = 0.7;
    let mut badge = path_layer(
        "badge",
        polyline(
            &[(52.0, 8.0), (90.0, 8.0), (90.0, 56.0), (52.0, 56.0)],
            true,
        ),
        Some(solid("#2060FFC0")),
        Some((solid("#FFFFFF80"), 8.0)),
    );
    badge.opacity = 0.6;
    let layers = vec![eight, badge];
    assert_paths_match_cpu(&mut renderer, "translucent", layers.clone());

    // The overlaps are drawn once: where the strokes cross, and where
    // the stroke covers the fill, the color is what one layer gives.
    let mut scene = empty_scene(96, 64);
    scene.layers = layers;
    let gpu = renderer.render(&scene).unwrap();
    let mut single = empty_scene(96, 64);
    single.layers = vec![path_layer(
        "plain",
        polyline(&[(4.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0)], true),
        Some(solid("#FF000080")),
        None,
    )];
    single.layers[0].opacity = 0.7;
    let plain = renderer.render(&single).unwrap();
    // The diagonals cross at (22, 22).
    assert_eq!(pixel_at(&gpu, 22, 22), pixel_at(&plain, 6, 6));
}

#[test]
fn paints_gradients_in_local_coordinates_like_the_cpu_renderer() {
    use celesta_composition::{GradientStop, PathCommand};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let stops = |from: &str, to: &str| {
        vec![
            GradientStop {
                offset: 0.0,
                color: from.to_owned(),
            },
            GradientStop {
                offset: 1.0,
                color: to.to_owned(),
            },
        ]
    };
    let ring = vec![
        PathCommand::MoveTo { x: 20.0, y: 10.0 },
        PathCommand::CubicTo {
            x1: 20.0,
            y1: 16.0,
            x2: 16.0,
            y2: 20.0,
            x: 10.0,
            y: 20.0,
        },
        PathCommand::CubicTo {
            x1: 4.0,
            y1: 20.0,
            x2: 0.0,
            y2: 16.0,
            x: 0.0,
            y: 10.0,
        },
        PathCommand::CubicTo {
            x1: 0.0,
            y1: 4.0,
            x2: 4.0,
            y2: 0.0,
            x: 10.0,
            y: 0.0,
        },
        PathCommand::CubicTo {
            x1: 16.0,
            y1: 0.0,
            x2: 20.0,
            y2: 4.0,
            x: 20.0,
            y: 10.0,
        },
        PathCommand::Close,
    ];
    assert_paths_match_cpu(
        &mut renderer,
        "gradients",
        vec![
            group(
                EvaluatedTransform {
                    position: Point { x: 4.0, y: 6.0 },
                    scale: Point { x: 2.0, y: 2.5 },
                    ..EvaluatedTransform::default()
                },
                vec![path_layer(
                    "linear",
                    ring.clone(),
                    Some(Paint::Linear {
                        start: Point { x: 0.0, y: 0.0 },
                        end: Point { x: 20.0, y: 20.0 },
                        stops: stops("#FF0000", "#0000FF80"),
                    }),
                    Some((
                        Paint::Radial {
                            center: Point { x: 10.0, y: 10.0 },
                            radius: 12.0,
                            stops: stops("#FFFFFF", "#00FF00"),
                        },
                        2.0,
                    )),
                )],
            ),
            group(
                EvaluatedTransform {
                    position: Point { x: 56.0, y: 10.0 },
                    scale: Point { x: 1.5, y: 1.5 },
                    ..EvaluatedTransform::default()
                },
                vec![path_layer(
                    "radial",
                    ring,
                    Some(Paint::Radial {
                        center: Point { x: 6.0, y: 6.0 },
                        radius: 16.0,
                        stops: stops("#FFE080", "#8000FF"),
                    }),
                    None,
                )],
            ),
        ],
    );
}

#[test]
fn clips_blends_and_filters_paths_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let star = |id: &str| {
        path_layer(
            id,
            polyline(
                &[
                    (16.0, 2.0),
                    (21.0, 12.0),
                    (32.0, 13.0),
                    (24.0, 21.0),
                    (26.0, 32.0),
                    (16.0, 27.0),
                    (6.0, 32.0),
                    (8.0, 21.0),
                    (0.0, 13.0),
                    (11.0, 12.0),
                ],
                true,
            ),
            Some(solid("#FFCC33")),
            Some((solid("#C04020"), 1.5)),
        )
    };
    let mut multiplied = star("multiplied");
    multiplied.transform.position = Point { x: 32.0, y: 4.0 };
    multiplied.blend_mode = BlendMode::Multiply;
    let mut blurred = group(
        EvaluatedTransform {
            position: Point { x: 62.0, y: 28.0 },
            ..EvaluatedTransform::default()
        },
        vec![star("blurred")],
    );
    blurred.effects.blur = 1.5;
    assert_paths_match_cpu(
        &mut renderer,
        "clips, blends and effects",
        vec![
            corner_rect("backdrop", 30.0, 0.0, 40.0, 40.0, "#40A0FF"),
            clipped_group(
                EvaluatedTransform {
                    position: Point { x: 2.0, y: 2.0 },
                    ..EvaluatedTransform::default()
                },
                Clip {
                    x: 4.0,
                    y: 4.0,
                    width: 22.0,
                    height: 20.0,
                    corner_radius: 6.0,
                },
                vec![star("clipped")],
            ),
            multiplied,
            blurred,
        ],
    );
}

/// The CPU renderer draws no rotated layers, so rotated paths are
/// compared with its rasterizer's pixels composited by hand.
#[test]
fn shades_rotated_paths_like_the_cpu_rasterizer() {
    use celesta_composition::{LineCap, LineJoin, PathCommand};
    use celesta_renderer::{PathDraw, PathShape, rasterize_paths};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let ellipse = |rx: f64, ry: f64| {
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
    };
    let mut scene = empty_scene(320, 180);
    // Rings like NEBULA's, overhanging the frame on every side.
    scene.layers = (0..8)
        .map(|k| {
            let mut ring = path_layer(
                &format!("ring-{k}"),
                ellipse(40.0 + f64::from(k) * 22.0, 15.0 + f64::from(k) * 9.0),
                (k == 0).then(|| solid("#20408080")),
                Some((solid("#8FB8FF"), 1.5)),
            );
            ring.transform = EvaluatedTransform {
                position: Point { x: 160.0, y: 90.0 },
                rotation: f64::from(k) * 23.0 + 7.0,
                ..EvaluatedTransform::default()
            };
            ring.opacity = 0.3 + 0.08 * f64::from(k);
            styled(ring, LineCap::Butt, LineJoin::Miter, 4.0)
        })
        .collect();
    let gpu = renderer.render(&scene).unwrap();

    let draws: Vec<_> = scene
        .layers
        .iter()
        .map(|layer| {
            let LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                miter_limit,
                line_dash,
            } = &layer.content
            else {
                unreachable!("the scene is paths");
            };
            PathDraw {
                shape: PathShape {
                    commands,
                    fill: fill.as_ref(),
                    stroke: stroke.as_ref(),
                    line_cap: *line_cap,
                    line_join: *line_join,
                    miter_limit: *miter_limit,
                    line_dash: line_dash.as_deref(),
                },
                transform: path_transform(&layer.transform),
                opacity: layer.opacity,
            }
        })
        .collect();
    let rasterized = rasterize_paths(&draws, scene.width, scene.height)
        .unwrap()
        .unwrap();
    let background = GpuRenderOptions::default().background;
    let mut cpu: Vec<u8> = (0..scene.width * scene.height)
        .flat_map(|_| [background.red, background.green, background.blue, 255])
        .collect();
    let image = &rasterized.image;
    for (index, texel) in image.pixels().as_chunks::<4>().0.iter().enumerate() {
        let x = rasterized.left + (index as u32 % image.width()) as i32;
        let y = rasterized.top + (index as u32 / image.width()) as i32;
        let pixel = &mut cpu[(y as usize * scene.width as usize + x as usize) * 4..][..3];
        let alpha = f64::from(texel[3]) / 255.0;
        for (channel, value) in pixel.iter_mut().zip(texel) {
            *channel =
                (f64::from(*value) * alpha + f64::from(*channel) * (1.0 - alpha)).round() as u8;
        }
    }
    assert_paths_match("rotated", gpu.pixels(), &cpu);
}

#[test]
fn exports_and_previews_paths_with_the_same_pixels() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(96, 64);
    let mut ring = path_layer(
        "ring",
        polyline(
            &[(10.0, 10.0), (80.0, 20.0), (60.0, 56.0), (14.0, 44.0)],
            true,
        ),
        Some(solid("#3060FF80")),
        Some((solid("#FFE080"), 2.0)),
    );
    ring.opacity = 0.75;
    ring.transform.rotation = 9.0;
    scene.layers = vec![ring];
    let rendered = renderer.render(&scene).unwrap();
    // Pipelined, as an export renders.
    assert!(renderer.submit(&scene).unwrap().is_none());
    let exported = renderer.drain().unwrap();
    assert_eq!(exported[0].pixels(), rendered.pixels());
    // Onto a BGRA target, as the preview renders.
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta preview test target"),
        size: wgpu::Extent3d {
            width: scene.width,
            height: scene.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render_to_target(
            &scene,
            GpuRenderTarget {
                view: &view,
                format: wgpu::TextureFormat::Bgra8Unorm,
                width: scene.width,
                height: scene.height,
            },
        )
        .unwrap();
    let layout = ReadbackLayout::new(scene.width, scene.height).unwrap();
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Celesta preview test readback"),
        size: layout.buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(layout.padded_bytes_per_row),
                rows_per_image: Some(scene.height),
            },
        },
        texture.size(),
    );
    renderer.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut previewed = layout.unpad(&mapped, scene.width, scene.height).unwrap();
    for pixel in previewed.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    assert_eq!(previewed, rendered.pixels());
}

#[test]
fn shades_paths_alongside_changing_filtered_and_unfiltered_images() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    for (index, rgba) in [[0, 255, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
        let image = seeded_image(&mut renderer, &format!("image-{index}"), 1.0, rgba);
        let mut filtered = seeded_image(&mut renderer, &format!("filtered-{index}"), 6.0, rgba);
        filtered.transform.position.y = 2.0;
        filtered.transform.scale = Point { x: 2.0, y: 2.0 };
        let mut scene = empty_scene(16, 8);
        scene.layers = vec![
            image,
            filtered,
            path_layer(
                "triangle",
                polyline(&[(9.0, 1.0), (15.0, 1.0), (9.0, 7.0)], true),
                Some(solid("#FF0000")),
                None,
            ),
        ];
        let frame = renderer.render(&scene).unwrap();
        assert_eq!(
            [
                pixel_at(&frame, 0, 0),
                pixel_at(&frame, 6, 2),
                pixel_at(&frame, 10, 2)
            ],
            [rgba, rgba, [255, 0, 0, 255]],
            "frame {index} must shade paths and both image sampling modes"
        );
    }
}

/// Many edges can cross one pixel on one scanline; the shader steps
/// through every crossing, winding by winding.
#[test]
fn measures_pixels_crossed_by_many_edges_exactly() {
    use celesta_composition::PathCommand;

    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let rect = |commands: &mut Vec<PathCommand>, left: f64, right: f64, clockwise: bool| {
        let mut corners = [(left, 4.0), (right, 4.0), (right, 28.0), (left, 28.0)];
        if !clockwise {
            corners.reverse();
        }
        commands.extend(polyline(&corners, true));
    };
    let mut commands = Vec::new();
    // Five slivers 0.06 px wide inside pixel column 10: ten crossings,
    // covering 0.3 of each pixel.
    for sliver in 0..5 {
        let left = 10.05 + f64::from(sliver) * 0.18;
        rect(&mut commands, left, left + 0.06, true);
    }
    // The same square five times each way inside column 20: twenty
    // crossings whose windings cancel.
    for _ in 0..5 {
        rect(&mut commands, 20.1, 20.9, true);
        rect(&mut commands, 20.1, 20.9, false);
    }
    let mut scene = empty_scene(32, 32);
    scene.layers = vec![path_layer(
        "crowded",
        commands,
        Some(solid("#FFFFFF")),
        None,
    )];
    let frame = renderer.render(&scene).unwrap();
    let alpha = |x| pixel_at(&frame, x, 16)[3];
    assert!(alpha(10).abs_diff(77) <= 2, "slivers cover {}", alpha(10));
    assert_eq!(alpha(20), 0);
}

/// An animated path changes shape every frame; what the GPU holds for
/// paths is sized by the largest frame, not by how many were drawn.
#[test]
fn keeps_path_buffers_bounded_while_shapes_animate() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let frame = |frame: u32| {
        let mut scene = empty_scene(96, 64);
        let t = f64::from(frame) * 0.37;
        scene.layers = vec![path_layer(
            "wobble",
            polyline(
                &(0..24)
                    .map(|i| {
                        let angle = f64::from(i) / 24.0 * std::f64::consts::TAU;
                        let radius = 20.0 + 8.0 * (angle * 3.0 + t).sin();
                        (48.0 + radius * angle.cos(), 32.0 + radius * angle.sin())
                    })
                    .collect::<Vec<_>>(),
                true,
            ),
            Some(solid("#80C0FF")),
            Some((solid("#FFFFFF"), 1.0 + (t * 0.5).sin().abs())),
        )];
        // Exceed the initial 1024 entries to exercise buffer growth and
        // bind-group replacement, then animate without accumulating data.
        let layer = scene.layers.pop().unwrap();
        scene.layers = (0..8)
            .map(|i| Layer {
                id: format!("wobble-{i}"),
                ..layer.clone()
            })
            .collect();
        scene
    };
    let initial_size = renderer.paths.size();
    let mut sizes = Vec::new();
    for index in 0..200 {
        renderer.submit(&frame(index)).unwrap();
        sizes.push(renderer.paths.size());
    }
    renderer.drain().unwrap();
    assert!(sizes[0] > initial_size, "path buffer must actually grow");
    let largest = renderer.path_entries.capacity() as u64 * 16;
    assert!(sizes.iter().all(|&size| size == sizes[20]), "{sizes:?}");
    assert!(sizes[20] <= (largest * 2).next_power_of_two(), "{sizes:?}");
    let mut scene = frame(199);
    scene.layers.truncate(1);
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    assert_paths_match("grown path buffer", gpu.pixels(), cpu.pixels());
}

#[test]
fn clips_groups_that_blend_like_the_cpu_renderer() {
    let background = Color::rgba(10, 20, 30, 255);
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let clip = Clip {
        x: 6.0,
        y: 6.0,
        width: 30.0,
        height: 24.0,
        corner_radius: 8.0,
    };
    let mut scene = empty_scene(48, 40);
    scene.layers = vec![
        blend_rect("light", 0.0, 0.0, 48.0, "#e0d0c0", BlendMode::Normal),
        // An isolated group blends as one layer; only its clipped part shows.
        Layer {
            blend_mode: BlendMode::Difference,
            opacity: 0.8,
            ..clipped_group(
                EvaluatedTransform::default(),
                clip,
                vec![
                    blend_rect("a", 0.0, 0.0, 30.0, "#3c6382", BlendMode::Normal),
                    blend_rect("b", 20.0, 10.0, 30.0, "#f8c291c0", BlendMode::Screen),
                ],
            )
        },
        // A group that is not isolated clips each child's own blend.
        clipped_group(
            EvaluatedTransform {
                position: Point { x: 10.0, y: 20.0 },
                ..EvaluatedTransform::default()
            },
            clip,
            vec![blend_rect(
                "c",
                0.0,
                0.0,
                40.0,
                "#6ab04c",
                BlendMode::Multiply,
            )],
        ),
    ];
    let expected = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: celesta_renderer::Color::rgba(10, 20, 30, 255),
    })
    .render(&scene)
    .unwrap();
    let frame = renderer.render(&scene).unwrap();
    let difference = max_channel_difference(&frame, &expected);
    assert!(difference <= 2, "channels differ by up to {difference}");
}

#[test]
fn a_clip_rotates_with_its_group() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // Turned 90 degrees clockwise about (20, 20), the group's x axis
    // points down the canvas and its y axis points left, so the clip's
    // 20x10 rectangle lands on canvas x 10..20, y 20..40.
    let mut scene = empty_scene(40, 40);
    scene.layers = vec![clipped_group(
        EvaluatedTransform {
            position: Point { x: 20.0, y: 20.0 },
            rotation: 90.0,
            ..EvaluatedTransform::default()
        },
        Clip {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 10.0,
            corner_radius: 0.0,
        },
        vec![corner_rect("red", -40.0, -40.0, 80.0, 80.0, "#FF0000FF")],
    )];
    let frame = renderer.render(&scene).unwrap();

    assert_eq!(pixel_at(&frame, 15, 30), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 10, 20), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&frame, 19, 39), [255, 0, 0, 255]);
    let background = renderer.options().background;
    let background = [
        background.red,
        background.green,
        background.blue,
        background.alpha,
    ];
    for (x, y) in [(9, 30), (20, 30), (15, 19)] {
        assert_eq!(pixel_at(&frame, x, y), background, "({x}, {y}) leaked");
    }
}

#[test]
fn a_frame_can_hold_more_clips_than_the_buffer_starts_with() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // 100 one-pixel clips, one per column, each over a whole-column rect.
    let mut scene = empty_scene(100, 4);
    scene.layers = (0..100)
        .map(|column| {
            clipped_group(
                EvaluatedTransform::default(),
                Clip {
                    x: f64::from(column),
                    y: 0.0,
                    width: 1.0,
                    height: 2.0,
                    corner_radius: 0.0,
                },
                vec![corner_rect(
                    "column",
                    f64::from(column),
                    0.0,
                    1.0,
                    4.0,
                    "#FF0000FF",
                )],
            )
        })
        .collect();
    let frame = renderer.render(&scene).unwrap();
    let background = renderer.options().background;

    for column in [0, 1, 63, 64, 65, 99] {
        assert_eq!(pixel_at(&frame, column, 0), [255, 0, 0, 255]);
        assert_eq!(pixel_at(&frame, column, 1), [255, 0, 0, 255]);
        assert_eq!(
            pixel_at(&frame, column, 3),
            [
                background.red,
                background.green,
                background.blue,
                background.alpha
            ]
        );
    }
}

#[test]
fn refuses_clips_nested_deeper_than_the_shader_walks() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let clip = Clip {
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 8.0,
        corner_radius: 0.0,
    };
    let nested = |depth: u32| {
        (0..depth).fold(
            vec![corner_rect("red", 0.0, 0.0, 8.0, 8.0, "#FF0000FF")],
            |layers, _| vec![clipped_group(EvaluatedTransform::default(), clip, layers)],
        )
    };

    let mut scene = empty_scene(8, 8);
    scene.layers = nested(MAX_CLIP_DEPTH);
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(pixel_at(&frame, 4, 4), [255, 0, 0, 255]);

    scene.layers = nested(MAX_CLIP_DEPTH + 1);
    assert!(matches!(
        renderer.render(&scene),
        Err(GpuRenderError::ClipsNestedTooDeep(depth)) if depth == MAX_CLIP_DEPTH + 1
    ));
}

/// A 256x10 scene on black: a linear gradient from black to `#080808`
/// across the top eight rows, eight codes over 256 pixels, and an opaque
/// `#102030` rect across the bottom two.
fn gentle_gradient_scene() -> Scene {
    let mut scene = empty_scene(256, 10);
    let mut gradient = solid_rect("gradient", 128.0, 256.0, 8.0, "#000000");
    gradient.content = LayerContent::Rect {
        width: 256.0,
        height: 8.0,
        fill: Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 256.0, y: 0.0 },
            stops: vec![
                celesta_composition::GradientStop {
                    offset: 0.0,
                    color: "#000000".to_owned(),
                },
                celesta_composition::GradientStop {
                    offset: 1.0,
                    color: "#080808".to_owned(),
                },
            ],
        }),
        stroke: None,
        corner_radius: 0.0,
    };
    let mut solid = solid_rect("solid", 128.0, 256.0, 2.0, "#102030");
    solid.transform.position.y = 9.0;
    scene.layers = vec![gradient, solid];
    scene
}

/// The longest run of equal values in `values`.
fn longest_run(values: &[u8]) -> usize {
    values
        .chunk_by(|first, second| first == second)
        .map(<[u8]>::len)
        .max()
        .unwrap_or(0)
}

/// Whether the renderer composites in half floats, which a dither needs to
/// have anything finer than 8-bit code values to spread.
fn composites_in_half_floats(renderer: &GpuRenderer) -> bool {
    let half_floats = renderer.canvas_format == wgpu::TextureFormat::Rgba16Float;
    if !half_floats {
        eprintln!("skipping dither test: the GPU cannot composite in half floats");
    }
    half_floats
}

#[test]
fn dithers_gentle_gradients_and_keeps_solid_fills() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    if !composites_in_half_floats(&renderer) {
        return;
    }
    let scene = gentle_gradient_scene();
    let frame = renderer.render(&scene).unwrap();
    let (gradient, solid) = frame.pixels().split_at(256 * 8 * 4);
    let red: Vec<u8> = gradient.iter().step_by(4).copied().collect();
    // Rounded alone, each code would be a flat band 32 pixels wide. Along
    // one row the pattern still leaves runs over half a code, as only every
    // eighth pixel has its lowest threshold, and values within 1/8 of a code
    // of a whole one are not dithered.
    for row in red.as_chunks::<256>().0 {
        assert!(longest_run(row) <= 24, "{row:?}");
    }
    // The dither keeps the gradient's average: over the eight rows its
    // pattern repeats in, each band's mean stays near the values it rounds.
    for band in 0..8 {
        let sum: u32 = red
            .as_chunks::<256>()
            .0
            .iter()
            .flat_map(|row| &row[band * 32..band * 32 + 32])
            .map(|&value| u32::from(value))
            .sum();
        let mean = f64::from(sum) / 256.0;
        let ideal = band as f64 + 0.5;
        assert!(
            (mean - ideal).abs() < 0.15,
            "band {band}: {mean} for {ideal}"
        );
    }
    assert_eq!(solid, [0x10, 0x20, 0x30, 255].repeat(256 * 2));
    // The pattern depends on the pixel only: the same frame dithers alike.
    assert_eq!(renderer.render(&scene).unwrap().pixels(), frame.pixels());
}

#[test]
fn dithers_gentle_gradients_converted_to_yuv420p() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    if !composites_in_half_floats(&renderer) {
        return;
    }
    if !renderer.supports_yuv420p_readback() {
        eprintln!("skipping yuv420p test: the GPU cannot convert to yuv420p");
        return;
    }
    renderer
        .set_readback_format(ReadbackFormat::Yuv420p)
        .unwrap();
    assert!(renderer.submit(&gentle_gradient_scene()).unwrap().is_none());
    let frame = renderer.drain().unwrap().remove(0);
    // Luma rises 219/255 of a code per code of gray, so rounded alone it
    // would step about every 37 pixels.
    let luma = &frame.pixels()[..256 * 10];
    for row in luma[..256 * 8].as_chunks::<256>().0 {
        assert!(longest_run(row) <= 28, "{row:?}");
        assert_eq!(row[0], 16);
    }
    // #102030, whose luma is 40.9 before rounding: within the dither's code
    // of 41.
    assert!(luma[256 * 8..].iter().all(|&value| value.abs_diff(41) <= 1));
}

#[test]
fn renders_a_gradient_filled_rect() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut scene = empty_scene(8, 1);
    let mut layer = solid_rect("gradient", 4.0, 8.0, 1.0, "#000000");
    layer.content = LayerContent::Rect {
        width: 8.0,
        height: 1.0,
        fill: Some(Paint::Linear {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 8.0, y: 0.0 },
            stops: vec![
                celesta_composition::GradientStop {
                    offset: 0.0,
                    color: "#ff0000".to_owned(),
                },
                celesta_composition::GradientStop {
                    offset: 1.0,
                    color: "#0000ff".to_owned(),
                },
            ],
        }),
        stroke: None,
        corner_radius: 0.0,
    };
    scene.layers = vec![layer];
    let frame = renderer.render(&scene).unwrap();
    let pixel = |x: usize| frame.pixels()[x * 4..x * 4 + 4].to_vec();
    assert!(pixel(0)[0] > 220 && pixel(0)[2] < 40, "{:?}", pixel(0));
    assert!(pixel(7)[2] > 220 && pixel(7)[0] < 40, "{:?}", pixel(7));
}

fn solid_rect(id: &str, x: f64, width: f64, height: f64, color: &str) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position: Point { x, y: height / 2.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
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

/// Asserts that every value of `plane` is within the dither's one code of
/// `expected`, the value libswscale rounds to, and is `expected` itself
/// where the unrounded value is a whole code (`exact`).
fn assert_dithered_plane(plane: &[u8], expected: &[u8], exact: &[bool]) {
    for (row, values) in plane.chunks_exact(expected.len()).enumerate() {
        for (column, ((value, expected), exact)) in
            values.iter().zip(expected).zip(exact).enumerate()
        {
            let allowed = if *exact { 0 } else { 1 };
            assert!(
                value.abs_diff(*expected) <= allowed,
                "({column}, {row}): {value}, expected {expected}"
            );
        }
    }
}

#[test]
fn converts_frames_to_within_a_dithered_code_of_libswscale() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    if !renderer.supports_yuv420p_readback() {
        eprintln!("skipping yuv420p test: the GPU cannot convert to yuv420p");
        return;
    }
    renderer
        .set_readback_format(ReadbackFormat::Yuv420p)
        .unwrap();
    // Red, blue, white, and black columns, two pixels wide each.
    let mut scene = empty_scene(8, 4);
    for (index, color) in ["#ff0000", "#0000ff", "#ffffff", "#000000"]
        .into_iter()
        .enumerate()
    {
        let x = index as f64 * 2.0 + 1.0;
        scene
            .layers
            .push(solid_rect(&format!("column-{index}"), x, 2.0, 4.0, color));
    }

    assert!(renderer.submit(&scene).unwrap().is_none());
    let frame = renderer.drain().unwrap().remove(0);
    assert_eq!(frame.format(), ReadbackFormat::Yuv420p);
    assert_eq!(frame.pixels().len(), 8 * 4 * 3 / 2);
    let (luma, chroma) = frame.pixels().split_at(32);
    let (u, v) = chroma.split_at(8);
    // `ffmpeg -f rawvideo -pix_fmt rgba -i … -pix_fmt yuv420p` output
    // for the same pixels. White, black, and the chroma of the neutral
    // columns are whole codes before rounding, so the dither keeps them.
    assert_dithered_plane(
        luma,
        &[81, 81, 41, 41, 235, 235, 16, 16],
        &[false, false, false, false, true, true, true, true],
    );
    assert_dithered_plane(u, &[90, 240, 128, 128], &[false, false, true, true]);
    assert_dithered_plane(v, &[240, 110, 128, 128], &[false, false, true, true]);
}

#[test]
fn pipelines_yuv420p_frames_with_padded_plane_rows() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    if !renderer.supports_yuv420p_readback() {
        eprintln!("skipping yuv420p test: the GPU cannot convert to yuv420p");
        return;
    }
    renderer
        .set_readback_format(ReadbackFormat::Yuv420p)
        .unwrap();
    // 6x2 packs into 18 bytes: 12 of luma and a 3x1 U and V plane each,
    // every plane row padded to the copy alignment on the GPU.
    // Each color's Y, U, and V, and whether its luma is a whole code before
    // rounding, which the dither keeps.
    let colors = [
        (Color::rgba(255, 0, 0, 255), [81, 90, 240], false),
        (Color::rgba(0, 0, 255, 255), [41, 240, 110], false),
        (Color::rgba(255, 255, 255, 255), [235, 128, 128], true),
        (Color::rgba(0, 0, 0, 255), [16, 128, 128], true),
        (Color::rgba(128, 128, 128, 255), [126, 128, 128], false),
    ];
    let mut frames = Vec::new();
    for (color, _, _) in colors {
        renderer.options.background = color;
        frames.extend(renderer.submit(&empty_scene(6, 2)).unwrap());
    }
    frames.extend(renderer.drain().unwrap());

    assert_eq!(frames.len(), colors.len());
    for (frame, (_, [y, u, v], neutral)) in frames.iter().zip(colors) {
        let (luma, chroma) = frame.pixels().split_at(12);
        let (u_plane, v_plane) = chroma.split_at(3);
        assert_dithered_plane(luma, &[y; 6], &[neutral; 6]);
        // Gray's chroma is a whole code too.
        let exact = [neutral || u == 128; 3];
        assert_dithered_plane(u_plane, &[u; 3], &exact);
        assert_dithered_plane(v_plane, &[v; 3], &exact);
    }

    assert!(matches!(
        renderer.submit(&empty_scene(6, 3)),
        Err(GpuRenderError::OddYuv420pSize {
            width: 6,
            height: 3
        })
    ));

    // Switching back reads RGBA again, reusing the freed slots.
    renderer.set_readback_format(ReadbackFormat::Rgba8).unwrap();
    renderer.options.background = Color::rgba(1, 2, 3, 255);
    assert!(renderer.submit(&empty_scene(6, 2)).unwrap().is_none());
    let frame = renderer.drain().unwrap().remove(0);
    assert_eq!(frame.format(), ReadbackFormat::Rgba8);
    assert_eq!(frame.pixels(), [1, 2, 3, 255].repeat(12));
}

/// The little-endian 16-bit samples of `bytes`.
fn samples16(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| u16::from_le_bytes([sample[0], sample[1]]))
        .collect()
}

#[test]
fn converts_frames_to_ten_bit_yuv420p() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    if !renderer.supports_yuv420p_readback() {
        eprintln!("skipping yuv420p10 test: the GPU cannot convert to yuv420p");
        return;
    }
    renderer
        .set_readback_format(ReadbackFormat::Yuv420p10)
        .unwrap();
    // Each color's Y, U, and V: the 8-bit codes times 4. Only red's Y and U
    // fall between codes (325.9 and 360.8), which the dither rounds either
    // way.
    let colors = [
        (
            Color::rgba(255, 0, 0, 255),
            [326, 361, 960],
            [false, false, true],
        ),
        (Color::rgba(255, 255, 255, 255), [940, 512, 512], [true; 3]),
        (Color::rgba(0, 0, 0, 255), [64, 512, 512], [true; 3]),
    ];
    let mut frames = Vec::new();
    for (color, _, _) in colors {
        renderer.options.background = color;
        frames.extend(renderer.submit(&empty_scene(6, 2)).unwrap());
    }
    frames.extend(renderer.drain().unwrap());
    for (frame, (_, expected, exact)) in frames.iter().zip(colors) {
        assert_eq!(frame.format(), ReadbackFormat::Yuv420p10);
        // 12 luma and 3 + 3 chroma samples of 2 bytes each.
        assert_eq!(frame.pixels().len(), 36);
        let samples = samples16(frame.pixels());
        let planes = [&samples[..12], &samples[12..15], &samples[15..]];
        for ((plane, expected), exact) in planes.into_iter().zip(expected).zip(exact) {
            let allowed = if exact { 0 } else { 1 };
            assert!(
                plane
                    .iter()
                    .all(|value| value.abs_diff(expected) <= allowed),
                "{plane:?}, expected {expected}"
            );
        }
    }

    // Ten bits step four times finer than eight: the 8-code gradient spans
    // about 28 luma codes instead of 7.
    renderer.options.background = Color::rgba(0, 0, 0, 255);
    if !composites_in_half_floats(&renderer) {
        return;
    }
    assert!(renderer.submit(&gentle_gradient_scene()).unwrap().is_none());
    let frame = renderer.drain().unwrap().remove(0);
    let luma = samples16(&frame.pixels()[..256 * 10 * 2]);
    for row in luma[..256 * 8].as_chunks::<256>().0 {
        // 64.05 and 91.4 before rounding.
        assert!(row[0].abs_diff(64) <= 1, "{}", row[0]);
        assert!(row[255].abs_diff(91) <= 1, "{}", row[255]);
        assert!(longest_run16(row) <= 8, "{row:?}");
    }
}

/// The longest run of equal values in `values`.
fn longest_run16(values: &[u16]) -> usize {
    values
        .chunk_by(|first, second| first == second)
        .map(<[u16]>::len)
        .max()
        .unwrap_or(0)
}

#[test]
fn reads_frames_back_in_sixteen_bits() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    if !composites_in_half_floats(&renderer) {
        return;
    }
    renderer
        .set_readback_format(ReadbackFormat::Rgba16)
        .unwrap();
    assert!(renderer.submit(&gentle_gradient_scene()).unwrap().is_none());
    let frame = renderer.drain().unwrap().remove(0);
    assert_eq!(frame.format(), ReadbackFormat::Rgba16);
    assert_eq!(frame.pixels().len(), 256 * 10 * 8);
    let samples = samples16(frame.pixels());
    let (gradient, solid) = samples.split_at(256 * 8 * 4);
    // Not rounded to 8 bits: the red channel rises smoothly, sample by
    // sample, from black to 8 of 255.
    let red: Vec<u16> = gradient[..256 * 4].iter().step_by(4).copied().collect();
    assert!(red.is_sorted(), "{red:?}");
    assert!(red.chunk_by(|first, second| first == second).count() > 200);
    let last = f64::from(red[255]) / 257.0;
    assert!((last - 7.98).abs() < 0.05, "{last}");
    // #102030 within the half float's precision of 257 times each code.
    for pixel in solid.as_chunks::<4>().0 {
        for (value, code) in pixel.iter().zip([0x10_u16, 0x20, 0x30, 255]) {
            assert!(value.abs_diff(code * 257) <= 8, "{pixel:?}");
        }
    }
}

/// A translucent red rectangle (`rotation` degrees about its center) on a
/// transparent 64x64 frame.
fn translucent_red_scene(rotation: f64) -> Scene {
    let mut scene = empty_scene(64, 64);
    let mut red = solid_rect("red", 32.0, 32.0, 32.0, "#ff0000");
    red.transform = EvaluatedTransform {
        position: Point { x: 32.0, y: 32.0 },
        rotation,
        ..EvaluatedTransform::default()
    };
    red.opacity = 0.5;
    scene.layers = vec![red];
    scene
}

#[test]
fn reads_frames_back_with_straight_alpha() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    if !composites_in_half_floats(&renderer) {
        return;
    }
    renderer
        .set_readback_format(ReadbackFormat::Rgba16Straight)
        .unwrap();
    assert!(
        renderer
            .submit(&translucent_red_scene(0.0))
            .unwrap()
            .is_none()
    );
    let frame = renderer.drain().unwrap().remove(0);
    assert_eq!(frame.format(), ReadbackFormat::Rgba16Straight);
    assert_eq!(frame.pixels().len(), 64 * 64 * 8);
    let samples = samples16(frame.pixels());
    let pixel = |x: usize, y: usize| &samples[(y * 64 + x) * 4..][..4];
    // Inside the rectangle: straight red, half alpha.
    let inside = pixel(32, 32);
    assert!(
        inside[0].abs_diff(65535) <= 64 && inside[1] <= 64 && inside[2] <= 64,
        "{inside:?}"
    );
    assert!(inside[3].abs_diff(32768) <= 64, "{inside:?}");
    // Outside: nothing drawn.
    assert_eq!(pixel(2, 2), [0, 0, 0, 0]);
}

#[test]
fn straight_alpha_matches_premultiplied_divided_by_alpha() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    if !composites_in_half_floats(&renderer) {
        return;
    }
    let scene = translucent_red_scene(20.0);
    let mut read = |format| {
        renderer.set_readback_format(format).unwrap();
        assert!(renderer.submit(&scene).unwrap().is_none());
        samples16(renderer.drain().unwrap().remove(0).pixels())
    };
    let premultiplied = read(ReadbackFormat::Rgba16);
    let straight = read(ReadbackFormat::Rgba16Straight);
    assert_eq!(premultiplied.len(), straight.len());
    let mut checked = 0;
    let mut partial = 0;
    for (premultiplied, straight) in premultiplied
        .as_chunks::<4>()
        .0
        .iter()
        .zip(straight.as_chunks::<4>().0.iter())
    {
        let alpha = u32::from(premultiplied[3]);
        if alpha < 4096 {
            continue;
        }
        checked += 1;
        partial += usize::from(alpha < 32768 - 64);
        assert!(
            straight[3].abs_diff(premultiplied[3]) <= 1,
            "{premultiplied:?} {straight:?}"
        );
        // Half a code of premultiplied rounding, scaled up by the division.
        let allowed = 65535 / alpha + 1;
        for (premultiplied, straight) in premultiplied[..3].iter().zip(&straight[..3]) {
            let expected = u32::from(*premultiplied) * 65535 / alpha;
            assert!(
                u32::from(*straight).abs_diff(expected) <= allowed,
                "{premultiplied:?} {straight:?}"
            );
        }
    }
    assert!(checked > 1000, "{checked}");
    assert!(partial > 10, "the rotated edge is antialiased: {partial}");
}

#[test]
fn fits_a_widescreen_scene_inside_a_square_preview() {
    let viewport = PreviewViewport::fit(1920, 1080, 1000, 1000).unwrap();
    assert!((viewport.x - 0.0).abs() < 0.001);
    assert!((viewport.y - 218.75).abs() < 0.001);
    assert!((viewport.width - 1000.0).abs() < 0.001);
    assert!((viewport.height - 562.5).abs() < 0.001);
}

#[test]
fn svg_display_size_and_animated_scale_match_cpu() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logo.svg");
    std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="var(--fill, white)"/></svg>"#).unwrap();
    let mut scene = empty_scene(100, 100);
    scene.layers.push(Layer {
        id: "logo".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 50.0, y: 50.0 },
            ..Default::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            asset: ResolvedAsset {
                id: "logo".to_owned(),
                location: AssetLocation::File {
                    path: path.to_string_lossy().into_owned(),
                },
            },
            width: Some(20.0),
            height: None,
            fit: None,
            animation: None,
        },
    });
    let mut cpu = celesta_renderer::CpuRenderer::default();
    for scale in [0.8, 1.0, 2.0] {
        scene.layers[0].transform.scale = Point { x: scale, y: scale };
        let actual = renderer.render(&scene).unwrap();
        let expected = cpu.render(&scene).unwrap();
        let difference = max_channel_difference(&actual, &expected);
        assert!(difference <= 1, "scale {scale}: difference {difference}");
        assert_eq!(pixel_at(&actual, 50, 50), [255, 255, 255, 255]);
    }
}

#[test]
fn animated_images_draw_the_frame_at_their_time_as_the_cpu_does() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../renderer/tests/fixtures/two-frames.webp");
    let mut scene = empty_scene(40, 20);
    let image = |fit, seconds| LayerContent::Image {
        asset: ResolvedAsset {
            id: "two-frames".to_owned(),
            location: AssetLocation::File {
                path: path.to_string_lossy().into_owned(),
            },
        },
        width: Some(40.0),
        height: Some(20.0),
        fit,
        animation: Some(ImageAnimation {
            source_time_seconds: seconds,
            r#loop: true,
        }),
    };
    scene.layers.push(Layer {
        id: "animation".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..Default::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: image(None, 0.0),
    });
    let mut cpu = celesta_renderer::CpuRenderer::default();
    // The first frame is red on the left; the second, blue on the right,
    // over the background where the other was. Each is drawn again on the
    // way back, with and without a fit.
    for (fit, seconds, blue) in [
        (None, 0.0, false),
        (None, 0.1, true),
        (None, 0.35, false),
        (Some(ImageFit::Contain), 0.1, true),
        (Some(ImageFit::Contain), 0.0, false),
    ] {
        scene.layers[0].content = image(fit, seconds);
        let gpu = renderer.render(&scene).unwrap();
        let cpu = cpu.render(&scene).unwrap();
        // Inside the enlarged texels: between them, and at the image's
        // edges, the GPU filters and the CPU does not.
        for pixels in [cpu.pixels(), gpu.pixels()] {
            let at = |x: usize| &pixels[(10 * 40 + x) * 4..][..4];
            let (red, blue_side) = (at(7)[0] == 255, at(32)[2] >= 254);
            assert_eq!((red, blue_side), (!blue, blue), "{fit:?} {seconds} s");
        }
    }
}

#[test]
fn renders_an_offscreen_background_when_a_gpu_is_available() {
    let background = Color::rgba(51, 102, 153, 255);
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let frame = renderer.render(&empty_scene(3, 2)).unwrap();
    assert_eq!((frame.width(), frame.height()), (3, 2));
    assert_eq!(frame.pixels().len(), 24);
    assert!(
        frame
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [51, 102, 153, 255])
    );
}

#[test]
fn submit_and_drain_return_frames_in_submission_order_with_correct_content() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };

    // More scenes than PIPELINE_DEPTH so this exercises both the
    // reclaim-while-submitting path and the final drain, each scene
    // filled with a distinct background color to catch a slot mix-up.
    let colors = [
        Color::rgba(10, 20, 30, 255),
        Color::rgba(40, 50, 60, 255),
        Color::rgba(70, 80, 90, 255),
        Color::rgba(100, 110, 120, 255),
        Color::rgba(130, 140, 150, 255),
    ];
    let mut ready = Vec::new();
    for color in colors {
        renderer.options.background = color;
        if let Some(frame) = renderer.submit(&empty_scene(2, 2)).unwrap() {
            ready.push(frame);
        }
    }
    ready.extend(renderer.drain().unwrap());

    assert_eq!(ready.len(), colors.len());
    for (frame, color) in ready.iter().zip(colors) {
        assert_eq!((frame.width(), frame.height()), (2, 2));
        assert!(
            frame
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [color.red, color.green, color.blue, color.alpha])
        );
    }
}

#[test]
fn dropping_a_renderer_finishes_the_frames_in_flight() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // Large enough that the thread is still copying them out when the
    // renderer is dropped.
    for _ in 0..3 {
        assert!(renderer.submit(&empty_scene(3840, 2160)).unwrap().is_none());
    }
    let finished = renderer
        .readback_worker
        .as_ref()
        .expect("submit starts the readback worker")
        .finished
        .clone();
    // Waits for the readback thread instead of leaving it behind.
    drop(renderer);
    assert_eq!(finished.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[test]
fn few_path_pieces_sort_in_place_into_the_counted_order() {
    let (rows, columns) = (5, 7);
    let mut seed = 7_u32;
    for count in [0, 1, 2, 9, 40, 64] {
        // Pieces of the same tile keep apart by their first coordinate.
        let pieces: Vec<_> = (0..count)
            .map(|index| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as usize % (rows * columns), [index as f32; 4])
            })
            .collect();
        assert_eq!(
            sort_pieces_in_place(pieces.clone(), columns),
            count_pieces_into_rows(pieces, rows, columns),
            "{count} pieces"
        );
    }
}

#[test]
fn a_failed_frame_leaves_no_paths_for_the_next() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let square = path_layer(
        "square",
        polyline(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)], true),
        Some(solid("#FFFFFF")),
        None,
    );
    let missing = Layer {
        id: "missing".to_owned(),
        content: LayerContent::MissingComponent {
            component: "Missing".to_owned(),
            props: Default::default(),
        },
        ..square.clone()
    };
    let mut scene = empty_scene(8, 8);
    scene.layers = vec![
        solid_rect("a", 6.0, 1.0, 1.0, "#000000"),
        solid_rect("b", 7.0, 1.0, 1.0, "#000000"),
        square.clone(),
        missing,
    ];
    assert!(renderer.render(&scene).is_err());
    // The failed frame's path pointed past this frame's only item.
    scene.layers = vec![square];
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(pixel_at(&frame, 2, 2), [255; 4]);
}

#[test]
fn falls_back_to_cpu_preview_for_odd_dimensions() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    match renderer.render_preview(&empty_scene(3, 2)).unwrap() {
        PreviewFrame::Cpu(frame) => assert_eq!((frame.width(), frame.height()), (3, 2)),
        #[cfg(target_os = "macos")]
        PreviewFrame::Native(_) => panic!("NV12 preview requires even dimensions"),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn keeps_native_preview_backings_alive_across_dialogue_frame_churn() {
    use core_video::pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;

    // Keep the thread-bound lock here until the preview worker has joined.
    let Some(TestRenderer { renderer, _lock }) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    std::thread::spawn(move || {
        use std::collections::VecDeque;

        let mut renderer = renderer;
        let mut frames = VecDeque::new();
        for index in 0..120 {
            let mut scene = empty_scene(1280, 720);
            scene.layers.push(Layer {
                id: "changing-dialogue".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 640.0, y: 600.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Text {
                    text: format!("Dialogue preview frame {index}"),
                    style: Box::new(TextStyle {
                        font_size: Some(48.0),
                        ..TextStyle::default()
                    }),
                    max_width: Some(1000.0),
                    baseline_anchor: false,
                },
            });
            match renderer.render_preview(&scene).unwrap() {
                PreviewFrame::Native(frame) => {
                    let buffer = frame.pixel_buffer();
                    assert_eq!((buffer.get_width(), buffer.get_height()), (1280, 720));
                    assert_eq!(
                        buffer.get_pixel_format(),
                        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                    );
                    assert_eq!(buffer.get_plane_count(), 2);
                    frames.push_back(frame);
                    if frames.len() > 3 {
                        frames.pop_front();
                    }
                }
                PreviewFrame::Cpu(_) => {
                    panic!("Metal preview unexpectedly used CPU readback")
                }
            }
        }
    })
    .join()
    .unwrap();
}

#[test]
fn renders_to_a_bgra_preview_target_without_readback() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta preview test target"),
        size: wgpu::Extent3d {
            width: 320,
            height: 240,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    renderer
        .render_to_target(
            &empty_scene(1920, 1080),
            GpuRenderTarget {
                view: &view,
                format: wgpu::TextureFormat::Bgra8Unorm,
                width: 320,
                height: 240,
            },
        )
        .unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}

#[test]
fn decodes_and_rotates_a_nested_image_on_the_gpu() {
    let Some(renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut renderer = renderer.with_asset_root(env!("CARGO_MANIFEST_DIR"));
    let mut scene = empty_scene(2, 2);
    scene.layers.push(Layer {
        id: "group".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 1.0, y: 1.0 },
            rotation: 180.0,
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers: vec![Layer {
                id: "checker".to_owned(),
                transform: EvaluatedTransform::default(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: Default::default(),
                content: LayerContent::Image {
                    width: None,
                    height: None,
                    fit: None,
                    animation: None,
                    asset: ResolvedAsset {
                        id: "checker".to_owned(),
                        location: AssetLocation::File {
                            path: "tests/assets/checker.ppm".to_owned(),
                        },
                    },
                },
            }],
            clip: None,
            mask: None,
            motion_blur: false,
        },
    });

    let frame = renderer.render(&scene).unwrap();
    // Filtering through the rotation leaves the edge texels a hair off whole
    // codes, which the readback's dither can round either way.
    let expected = [
        255, 255, 255, 255, 0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255,
    ];
    assert!(
        frame
            .pixels()
            .iter()
            .zip(expected)
            .all(|(pixel, expected)| pixel.abs_diff(expected) <= 1),
        "{:?}",
        frame.pixels()
    );
}

#[test]
fn decodes_positions_and_fades_a_video_frame_on_the_gpu() {
    struct Decoder;

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(
            &mut self,
            path: &Path,
            source_time_seconds: f64,
        ) -> Result<VideoFrame, MediaError> {
            assert_eq!(path, Path::new("./clip.mp4"));
            assert_eq!(source_time_seconds, 2.0);
            Ok(VideoFrame {
                width: 1,
                height: 1,
                pixels: vec![12, 34, 56, 255].into(),
            })
        }
    }

    let Some(renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut renderer = renderer.with_video_decoder(Decoder);
    let mut scene = empty_scene(2, 1);
    scene.layers.push(Layer {
        id: "video".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 1.5, y: 0.5 },
            ..EvaluatedTransform::default()
        },
        opacity: 0.5,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Video {
            asset: ResolvedAsset {
                id: "clip".to_owned(),
                location: AssetLocation::File {
                    path: "clip.mp4".to_owned(),
                },
            },
            timing: MediaTiming {
                local_time: Time::new(1, 2),
                source_start: Time::new(1, 1),
                source_time_seconds: 2.0,
                playback_rate: 2.0,
                r#loop: false,
            },
            width: None,
            height: None,
        },
    });

    let frame = renderer.render(&scene).unwrap();
    assert_eq!(frame.pixels(), &[0, 0, 0, 255, 6, 17, 28, 255]);
}

/// A 320 x 180 video frame, all red.
struct RedFrameDecoder;

impl VideoFrameDecoder for RedFrameDecoder {
    fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
        Ok(VideoFrame {
            width: 320,
            height: 180,
            pixels: [255, 0, 0, 255].repeat(320 * 180).into(),
        })
    }
}

/// How far right and down the pixels `covered` picks reach in a frame
/// `width` pixels wide: the drawn size of a layer placed at the origin.
fn drawn_size(pixels: &[u8], width: u32, covered: impl Fn(&[u8; 4]) -> bool) -> (u32, u32) {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, pixel)| covered(pixel))
        .fold((0, 0), |(right, bottom), (index, _)| {
            let (x, y) = (index as u32 % width, index as u32 / width);
            (right.max(x + 1), bottom.max(y + 1))
        })
}

#[test]
fn sizes_a_video_frame_like_an_image() {
    let Some(renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut renderer = renderer.with_video_decoder(RedFrameDecoder);
    for (width, height, drawn) in [
        (Some(160.0), None, (160, 90)),
        (Some(160.0), Some(160.0), (160, 160)),
        (None, Some(90.0), (160, 90)),
        (None, None, (320, 180)),
    ] {
        let mut scene = empty_scene(320, 180);
        scene.layers.push(Layer {
            id: "video".to_owned(),
            transform: EvaluatedTransform {
                anchor: Point { x: 0.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Video {
                asset: ResolvedAsset {
                    id: "clip".to_owned(),
                    location: AssetLocation::File {
                        path: "clip.mp4".to_owned(),
                    },
                },
                timing: MediaTiming {
                    local_time: Time::ZERO,
                    source_start: Time::ZERO,
                    source_time_seconds: 0.0,
                    playback_rate: 1.0,
                    r#loop: false,
                },
                width,
                height,
            },
        });
        let frame = renderer.render(&scene).unwrap();
        assert_eq!(
            drawn_size(frame.pixels(), 320, |pixel| pixel[0] >= 128),
            drawn,
            "width {width:?}, height {height:?}"
        );
    }
}

/// 8x4 pixels: red on the left half, blue on the right.
fn split_pixels() -> Vec<u8> {
    (0..4)
        .flat_map(|_| {
            [[255, 0, 0, 255]; 4]
                .into_iter()
                .chain([[0, 0, 255, 255]; 4])
        })
        .flatten()
        .collect()
}

/// Drawn over the whole 8x4 frame from a texture half that size, each
/// half keeps its color. Shrinking and enlarging blur the seam a little,
/// and the outermost pixels soften into the transparent surroundings as
/// any enlarged layer's do, so inner pixels are read for their hue.
fn assert_split(frame: &GpuFrame) {
    let pixel = |x: usize, y: usize| &frame.pixels()[(y * 8 + x) * 4..][..4];
    let red = |p: &[u8]| p[0] > 200 && p[1] == 0 && p[2] < 50 && p[3] == 255;
    let blue = |p: &[u8]| p[0] < 50 && p[1] == 0 && p[2] > 200 && p[3] == 255;
    for y in 1..3 {
        for x in [1, 2] {
            assert!(red(pixel(x, y)), "({x}, {y}): {:?}", pixel(x, y));
        }
        for x in [5, 6] {
            assert!(blue(pixel(x, y)), "({x}, {y}): {:?}", pixel(x, y));
        }
    }
}

#[test]
fn shrinks_images_and_video_frames_larger_than_the_texture_limit() {
    struct Decoder;

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
            Ok(VideoFrame {
                width: 8,
                height: 4,
                pixels: split_pixels().into(),
            })
        }
    }

    let Some(renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut renderer = renderer.with_video_decoder(Decoder);
    renderer.max_texture_dimension = 4;
    let transform = EvaluatedTransform {
        position: Point { x: 4.0, y: 2.0 },
        ..EvaluatedTransform::default()
    };
    let asset = |id: &str, path: &str| ResolvedAsset {
        id: id.to_owned(),
        location: AssetLocation::File {
            path: path.to_owned(),
        },
    };

    renderer.image_sources.insert_raster(
        "split",
        image::RgbaImage::from_raw(8, 4, split_pixels()).unwrap(),
    );
    let mut scene = empty_scene(8, 4);
    scene.layers.push(Layer {
        id: "image".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            width: None,
            height: None,
            fit: None,
            animation: None,
            asset: asset("split", "split.png"),
        },
    });
    // Uploaded as is, the 8x4 image would exceed the 4-texel limit.
    assert_split(&renderer.render(&scene).unwrap());
    let texture = &renderer.textures.values().next().unwrap().texture;
    assert_eq!((texture.width, texture.height), (4, 2));

    scene.layers[0].content = LayerContent::Video {
        asset: asset("clip", "clip.mp4"),
        timing: MediaTiming {
            local_time: Time::ZERO,
            source_start: Time::ZERO,
            source_time_seconds: 0.0,
            playback_rate: 1.0,
            r#loop: false,
        },
        width: None,
        height: None,
    };
    assert_split(&renderer.render(&scene).unwrap());
}

#[test]
fn rejects_a_malformed_video_frame_larger_than_the_texture_limit() {
    struct Decoder;

    impl VideoFrameDecoder for Decoder {
        fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
            // One pixel short of 8x4.
            Ok(VideoFrame {
                width: 8,
                height: 4,
                pixels: vec![0; 31 * 4].into(),
            })
        }
    }

    let Some(renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut renderer = renderer.with_video_decoder(Decoder);
    renderer.max_texture_dimension = 4;
    let mut scene = empty_scene(8, 4);
    scene.layers.push(Layer {
        id: "video".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Video {
            asset: ResolvedAsset {
                id: "clip".to_owned(),
                location: AssetLocation::File {
                    path: "clip.mp4".to_owned(),
                },
            },
            timing: MediaTiming {
                local_time: Time::ZERO,
                source_start: Time::ZERO,
                source_time_seconds: 0.0,
                playback_rate: 1.0,
                r#loop: false,
            },
            width: None,
            height: None,
        },
    });
    assert!(matches!(
        renderer.render(&scene),
        Err(GpuRenderError::InvalidImageData { .. })
    ));
}

#[test]
fn keeps_psd_composites_of_different_levels_apart_at_the_same_size() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd");
    renderer.max_texture_dimension = 80;
    let portrait = |id: &str, scale: f64| Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 120.0, y: 160.0 },
            scale: Point { x: scale, y: scale },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Psd {
            asset: ResolvedAsset {
                id: "fixture".to_owned(),
                location: AssetLocation::File {
                    path: fixture.to_string_lossy().into_owned(),
                },
            },
            visible_layers: vec!["body".to_owned(), "body/base".to_owned()],
            enabled_layers: Vec::new(),
            disabled_layers: Vec::new(),
        },
    };
    // At full size the 240x320 canvas is composited at 120x160 and
    // shrunk to 60x80; at a quarter it is composited at 60x80 directly.
    let mut scene = empty_scene(240, 320);
    scene.layers = vec![portrait("full", 1.0), portrait("quarter", 0.25)];
    renderer.render(&scene).unwrap();
    assert_eq!(renderer.textures.len(), 2);
    assert!(
        renderer
            .textures
            .values()
            .all(|cached| (cached.texture.width, cached.texture.height) == (60, 80))
    );
}

#[test]
fn composites_a_psd_larger_than_the_texture_limit_at_the_limit() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd");
    renderer.max_texture_dimension = 100;
    let preset: Vec<String> = ["body", "body/base", "body/outfit-navy"]
        .map(str::to_owned)
        .to_vec();
    let mut scene = empty_scene(240, 320);
    scene.layers.push(Layer {
        id: "portrait".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 120.0, y: 160.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Psd {
            asset: ResolvedAsset {
                id: "fixture".to_owned(),
                location: AssetLocation::File {
                    path: fixture.to_string_lossy().into_owned(),
                },
            },
            visible_layers: preset.clone(),
            enabled_layers: Vec::new(),
            disabled_layers: Vec::new(),
        },
    });
    let frame = renderer.render(&scene).unwrap();
    let texture = &renderer.textures.values().next().unwrap().texture;
    assert_eq!((texture.width, texture.height), (75, 100));

    // Drawn at the PSD's full size: the shrunk composite is enlarged
    // back over the whole canvas, so its solid areas match it.
    let full = celesta_renderer::psd_source::PsdSources::default()
        .render("fixture", &fixture, &preset, &[], &[], 1.0)
        .unwrap();
    for (x, y) in [(120, 236), (70, 190), (170, 280), (10, 10)] {
        let index = (y * 240 + x) * 4;
        assert_eq!(
            frame.pixels()[index..index + 4],
            full.pixels[index..index + 4],
            "pixel ({x}, {y})"
        );
    }
}

#[test]
fn rasterizes_and_rotates_styled_text_on_the_gpu() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut scene = empty_scene(160, 80);
    scene.layers.push(Layer {
        id: "title".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 80.0, y: 40.0 },
            rotation: 12.0,
            ..EvaluatedTransform::default()
        },
        opacity: 0.75,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "Celesta".to_owned(),
            style: Box::new(TextStyle {
                font_size: Some(32.0),
                fill: Some(Paint::Solid {
                    color: "#ff8000".to_owned(),
                }),
                stroke: Some(
                    Stroke {
                        paint: Paint::Solid {
                            color: "#0040ff".to_owned(),
                        },
                        width: 1.0,
                    }
                    .into(),
                ),
                ..TextStyle::default()
            }),
            max_width: None,
            baseline_anchor: false,
        },
    });

    let frame = renderer.render(&scene).unwrap();
    assert!(
        frame
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| { pixel[3] > 0 && pixel[0] > pixel[1] && pixel[1] > pixel[2] })
    );
    assert!(
        frame
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| { pixel[3] > 0 && pixel[2] > pixel[0] })
    );
}

#[test]
fn lists_text_layers_whose_family_has_no_face() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let missing = |id: &str, weight| {
        let mut layer = text_layer(id, Point { x: 0.0, y: 0.0 }, 1.0, 24.0);
        if let LayerContent::Text { style, .. } = &mut layer.content {
            style.font_family = Some("Celesta Missing Family".into());
            style.font_weight = weight;
        }
        layer
    };
    let mut scene = empty_scene(320, 80);
    scene.layers = vec![
        missing("title", None),
        // Same family and weight: listed once, with the first layer.
        missing("subtitle", Some(400)),
        missing("caption", Some(700)),
        text_layer("default-font", Point { x: 0.0, y: 40.0 }, 1.0, 24.0),
    ];
    renderer.render(&scene).unwrap();
    let listed = renderer
        .font_fallbacks()
        .iter()
        .map(|fallback| (fallback.layer.as_str(), fallback.weight))
        .collect::<Vec<_>>();
    assert_eq!(listed, [("title", 400), ("caption", 700)]);

    // A cached text texture still reports its fallback on later frames.
    renderer.render(&scene).unwrap();
    assert_eq!(renderer.font_fallbacks().len(), 2);

    scene.layers.clear();
    renderer.render(&scene).unwrap();
    assert!(renderer.font_fallbacks().is_empty());
}

#[test]
fn lists_text_layers_with_characters_their_family_has_no_glyph_for() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    renderer.set_asset_root(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/prism"));
    let text = |id: &str, family: &str, value: &str| {
        let mut layer = text_layer(id, Point { x: 0.0, y: 0.0 }, 1.0, 24.0);
        if let LayerContent::Text { text, style, .. } = &mut layer.content {
            *text = value.to_owned();
            style.font_family = Some(family.into());
        }
        layer
    };
    let mut scene = empty_scene(320, 80);
    scene.fonts = vec![ResolvedAsset {
        id: "bebas".to_owned(),
        location: AssetLocation::File {
            path: "assets/fonts/BebasNeue-Regular.ttf".to_owned(),
        },
    }];
    scene.layers = vec![
        text("title", "Bebas Neue", "CELESTA ずんだもん"),
        // The same characters again: listed once, with the first layer.
        text("subtitle", "Bebas Neue", "ずんだもん"),
        text("caption", "Bebas Neue", "めたん"),
        text("complete", "Bebas Neue", "CELESTA 2026"),
        text("emoji", "Bebas Neue", "CELESTA 🎉"),
        // A family with no face gets only the font fallback warning.
        text("missing", "Celesta Missing Family", "ずんだもん"),
    ];
    renderer.render(&scene).unwrap();
    let listed = renderer
        .missing_glyphs()
        .iter()
        .map(|missing| {
            (
                missing.layer.as_str(),
                missing.characters.iter().collect::<String>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        listed,
        [
            ("title", "ずんだも".to_owned()),
            ("caption", "めたん".to_owned()),
        ]
    );
    assert_eq!(renderer.font_fallbacks().len(), 1);

    // A cached text texture still reports its missing glyphs.
    renderer.render(&scene).unwrap();
    assert_eq!(renderer.missing_glyphs().len(), 2);

    scene.layers.clear();
    renderer.render(&scene).unwrap();
    assert!(renderer.missing_glyphs().is_empty());
}

#[test]
fn a_text_s_anchor_box_is_its_layout_box() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(800, 800);
    // A stroke pads the texture past the layout box on its left and right;
    // a single line's box keeps the stroked rows, so not above or below.
    let stroked = |id: &str, rotation: f64| {
        let mut text = text_layer(id, Point { x: 100.0, y: 50.0 }, 1.0, 24.0);
        text.transform.rotation = rotation;
        if let LayerContent::Text {
            style, max_width, ..
        } = &mut text.content
        {
            style.stroke = Some(
                Stroke {
                    paint: Paint::Solid {
                        color: "#000000".to_owned(),
                    },
                    width: 16.0,
                }
                .into(),
            );
            *max_width = Some(300.0);
        }
        text
    };
    scene.layers = vec![
        stroked("flat", 0.0),
        stroked("turned", 90.0),
        corner_rect("r", 5.0, 5.0, 10.0, 10.0, "#ff0000"),
    ];
    renderer.set_record_hit_regions(true);
    renderer.render(&scene).unwrap();
    let regions = renderer.hit_regions();
    let near = |a: f64, b: f64| (a - b).abs() <= 1.0;

    let flat = &regions[0];
    let [top_left, top_right, _, bottom_left] = flat.anchor_box.expect("a text has an anchor box");
    // `x`/`y` place the layout box's top-left; it is `maxWidth` wide.
    assert!(
        near(top_left.x, 100.0) && near(top_left.y, 50.0),
        "{top_left:?}"
    );
    assert!(near(top_right.x - top_left.x, 300.0), "{top_right:?}");
    assert!(bottom_left.y - top_left.y > 10.0, "{bottom_left:?}");
    // The texture's quad reaches past it, by the stroke on either side.
    assert!(flat.corners[0].x < top_left.x - 8.0, "{:?}", flat.corners);
    assert!(flat.corners[1].x > top_right.x + 8.0, "{:?}", flat.corners);

    // Mapped like the corners: turned 90°, its top edge runs down.
    let [top_left, top_right, ..] = regions[1].anchor_box.expect("an anchor box");
    assert!(
        near(top_left.x, 100.0) && near(top_left.y, 50.0),
        "{top_left:?}"
    );
    assert!(near(top_right.x, 100.0), "{top_right:?}");
    assert!(near(top_right.y - top_left.y, 300.0), "{top_right:?}");

    // A rect's quad is its anchor box: nothing more to record.
    assert_eq!(regions[2].anchor_box, None);
}

/// `text_layer` saying `text` at 40 px, wrapped at `max_width`.
fn text_saying(id: &str, text: &str, position: Point, max_width: Option<f64>) -> Layer {
    let mut layer = text_layer(id, position, 1.0, 40.0);
    if let LayerContent::Text {
        text: said,
        max_width: wrap,
        ..
    } = &mut layer.content
    {
        *said = text.to_owned();
        *wrap = max_width;
    }
    layer
}

/// The text, style and `maxWidth` of text layer `layer`.
fn text_content(layer: &Layer) -> (&str, &TextStyle, Option<f64>) {
    let LayerContent::Text {
        text,
        style,
        max_width,
        ..
    } = &layer.content
    else {
        panic!("{} is not a text", layer.id);
    };
    (text, style, *max_width)
}

/// Sets `layer` in IBM Plex Mono from the prism example's fonts, so its
/// metrics are the same on every platform's system fonts.
fn in_bundled_font(renderer: &mut GpuRenderer, scene: &mut Scene, layer: &mut Layer) {
    renderer.set_asset_root(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/prism"));
    scene.fonts = vec![ResolvedAsset {
        id: "IBMPlexMono-Regular.ttf".to_owned(),
        location: AssetLocation::File {
            path: "assets/fonts/IBMPlexMono-Regular.ttf".to_owned(),
        },
    }];
    if let LayerContent::Text { style, .. } = &mut layer.content {
        style.font_family = Some("IBM Plex Mono".into());
    }
}

/// Asserts each of `actual`'s corners is within a pixel of `expected`'s.
fn assert_near_corners(actual: [Point; 4], expected: [Point; 4]) {
    assert_corners_within(actual, expected, 1.0);
}

/// Asserts each of `actual`'s corners is within `tolerance` of `expected`'s.
fn assert_corners_within(actual: [Point; 4], expected: [Point; 4], tolerance: f64) {
    for (corner, want) in actual.iter().zip(expected) {
        assert!(
            (corner.x - want.x).abs() <= tolerance && (corner.y - want.y).abs() <= tolerance,
            "{actual:?} is not {expected:?} within {tolerance}"
        );
    }
}

/// The scene's only region's text box, rendering `layer` alone.
fn rendered_text_box(renderer: &mut GpuRenderer, scene: &mut Scene, layer: Layer) -> HitRegion {
    scene.layers = vec![layer];
    renderer.set_record_hit_regions(true);
    renderer.render(scene).unwrap();
    let region = renderer.hit_regions()[0].clone();
    assert!(region.text_box.is_some(), "a drawn text has a text box");
    region
}

#[test]
fn measure_text_matches_the_rendered_layout() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(400, 300);
    let layer = text_saying(
        "t",
        "Celesta\nrenders text",
        Point { x: 20.0, y: 20.0 },
        None,
    );
    scene.layers = vec![layer.clone()];
    renderer.render(&scene).unwrap();
    let (text, style, max_width) = text_content(&layer);
    let used = renderer.text_sizes[&format!("{text}\0{style:?}\0{max_width:?}")];
    let metrics = renderer.measure_text(text, style, max_width);
    assert_eq!(
        (metrics.width, metrics.height, metrics.ascent),
        (used.width, used.height, used.ascent)
    );
    assert_eq!(metrics.lines, 2);
}

#[test]
fn text_layout_finds_a_text_inside_a_group() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut inner = text_saying("inner", "Inside", Point { x: 20.0, y: 20.0 }, Some(200.0));
    if let LayerContent::Text { style, .. } = &mut inner.content {
        style.align = Some(TextAlign::Center);
    }
    let masked = text_saying("masked", "Mask", Point { x: 20.0, y: 20.0 }, None);
    let mut outer = group(
        EvaluatedTransform::default(),
        vec![
            inner.clone(),
            corner_rect("r", 5.0, 5.0, 10.0, 10.0, "#ff0000"),
        ],
    );
    if let LayerContent::Group { mask, .. } = &mut outer.content {
        *mask = Some(alpha_mask(vec![masked]));
    }
    let mut scene = empty_scene(400, 300);
    scene.layers = vec![outer];
    renderer.render(&scene).unwrap();

    let (text, style, max_width) = text_content(&inner);
    let metrics = renderer.measure_text(text, style, max_width);
    assert_eq!(
        renderer.text_layout(&scene, "inner"),
        Some(TextLayout {
            text: text.to_owned(),
            metrics,
            max_width,
            align: TextAlign::Center,
        })
    );
    // Through a group's mask too; an unset `align` is `Left`.
    let layout = renderer
        .text_layout(&scene, "masked")
        .expect("a masked text");
    assert_eq!(
        (layout.text.as_str(), layout.align),
        ("Mask", TextAlign::Left)
    );
    assert_eq!(renderer.text_layout(&scene, "r"), None);
    assert_eq!(renderer.text_layout(&scene, "group"), None);
    assert_eq!(renderer.text_layout(&scene, "missing"), None);
}

#[test]
fn a_multi_line_text_s_box_is_its_anchor_box() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(500, 300);
    let layer = text_saying(
        "t",
        "First line\nSecond line",
        Point { x: 100.0, y: 50.0 },
        Some(300.0),
    );
    let (text, style, max_width) = text_content(&layer);
    let metrics = renderer.measure_text(text, style, max_width);
    let region = rendered_text_box(&mut renderer, &mut scene, layer.clone());
    let text_box = region.text_box.unwrap();
    // Unstroked, the texture is the anchor box: it records none.
    assert_near_corners(text_box, region.anchor_box.unwrap_or(region.corners));
    let height = 2.0 * metrics.line_height;
    assert_near_corners(
        text_box,
        [
            (100.0, 50.0),
            (400.0, 50.0),
            (400.0, 50.0 + height),
            (100.0, 50.0 + height),
        ]
        .map(|(x, y)| Point { x, y }),
    );
}

#[test]
fn a_single_line_text_s_box_is_its_line_box() {
    let Some(mut renderer) = transform_3d::transparent_renderer() else {
        return;
    };
    let mut scene = empty_scene(400, 300);
    let mut layer = text_saying("t", "HHH", Point { x: 200.0, y: 150.0 }, None);
    layer.transform.anchor = Point { x: 0.5, y: 0.5 };
    if let LayerContent::Text { style, .. } = &mut layer.content {
        style.font_size = Some(96.0);
    }
    in_bundled_font(&mut renderer, &mut scene, &mut layer);
    scene.layers = vec![layer.clone()];
    renderer.set_record_hit_regions(true);
    let frame = renderer.render(&scene).unwrap();
    // Measured once the frame loaded the scene's fonts.
    let (text, style, max_width) = text_content(&layer);
    let metrics = renderer.measure_text(text, style, max_width);
    assert!(metrics.cap_height > 0.0, "the font says how tall an H is");
    let [top_left, top_right, _, bottom_left] =
        renderer.hit_regions()[0].text_box.expect("a text box");
    assert!(
        (bottom_left.y - top_left.y - metrics.line_height).abs() <= 1.0,
        "{top_left:?} {bottom_left:?} is not {} tall",
        metrics.line_height
    );
    assert!((top_right.x - top_left.x - metrics.width).abs() <= 1.0);

    // The glyphs' ink: how much of each row the H's stems cover, so a row
    // the ink's top edge crosses counts for the part of it inked.
    let coverage = (0..frame.height())
        .map(|y| {
            let alpha = (0..frame.width())
                .map(|x| pixel_at(&frame, x, y)[3])
                .max()
                .unwrap_or(0);
            f64::from(alpha) / 255.0
        })
        .collect::<Vec<_>>();
    let last = coverage.iter().rposition(|&row| row > 0.5).expect("ink");
    let ink_bottom = (last + 1) as f64;
    let ink_top = ink_bottom - coverage.iter().sum::<f64>();
    assert!(
        ink_top >= top_left.y && ink_bottom <= bottom_left.y,
        "ink {ink_top}..{ink_bottom} is outside {top_left:?}..{bottom_left:?}"
    );
    // A pixel each way, and one more: cosmic-text draws glyphs on their
    // baseline moved up to a whole row (`LayoutGlyph::physical`), and the
    // font's cap height is a table's value, not the H's drawn outline.
    let cap_top = top_left.y + metrics.ascent - metrics.cap_height;
    let baseline = top_left.y + metrics.ascent;
    assert!(
        (ink_top - cap_top).abs() <= 2.0,
        "ink from {ink_top}, caps from {cap_top}"
    );
    assert!(
        (ink_bottom - baseline).abs() <= 2.0,
        "ink to {ink_bottom}, baseline at {baseline}"
    );
}

#[test]
fn a_cached_text_texture_keeps_its_box() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(400, 300);
    let mut layer = text_saying("t", "Cached", Point { x: 40.0, y: 60.0 }, None);
    in_bundled_font(&mut renderer, &mut scene, &mut layer);
    let first = rendered_text_box(&mut renderer, &mut scene, layer.clone());
    let (text, style, max_width) = text_content(&layer);
    let metrics = renderer.measure_text(text, style, max_width);
    let second = rendered_text_box(&mut renderer, &mut scene, layer.clone());
    assert_eq!(second.text_box, first.text_box);
    let [top_left, top_right, _, bottom_left] = first.text_box.unwrap();
    assert!((bottom_left.y - top_left.y - metrics.line_height).abs() <= 1.0);
    assert!((top_right.x - top_left.x - metrics.width).abs() <= 1.0);

    // Rasterized smaller than drawn, to fit a smaller texture limit.
    renderer.textures.clear();
    renderer.max_texture_dimension = 64;
    let shrunk = rendered_text_box(&mut renderer, &mut scene, layer.clone());
    let raster_scale = renderer
        .textures
        .values()
        .map(|cached| cached.texture.raster_scale)
        .fold(f32::INFINITY, f32::min);
    assert!(raster_scale < 1.0);
    let cached = rendered_text_box(&mut renderer, &mut scene, layer);
    assert_eq!(cached.text_box, shrunk.text_box);
    // The glyphs sit on a baseline moved to a whole texture row, which is
    // `1 / raster_scale` drawn pixels tall once the texture is shrunk.
    assert_corners_within(
        shrunk.text_box.unwrap(),
        first.text_box.unwrap(),
        1.0 / f64::from(raster_scale) + 1.0,
    );
}

#[test]
fn a_baseline_anchored_text_s_box_starts_an_ascent_above_its_position() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(400, 300);
    let mut layer = text_saying("t", "Baseline", Point { x: 50.0, y: 120.0 }, None);
    if let LayerContent::Text {
        baseline_anchor, ..
    } = &mut layer.content
    {
        *baseline_anchor = true;
    }
    let (text, style, max_width) = text_content(&layer);
    let metrics = renderer.measure_text(text, style, max_width);
    let region = rendered_text_box(&mut renderer, &mut scene, layer);
    let top = 120.0 - metrics.ascent;
    let (right, bottom) = (50.0 + metrics.width, top + metrics.line_height);
    assert_near_corners(
        region.text_box.unwrap(),
        [(50.0, top), (right, top), (right, bottom), (50.0, bottom)].map(|(x, y)| Point { x, y }),
    );
}

#[test]
fn a_mirrored_text_s_box_reverses_its_order() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(400, 300);
    let mut layer = text_saying(
        "t",
        "First\nSecond",
        Point { x: 300.0, y: 50.0 },
        Some(200.0),
    );
    layer.transform.scale = Point { x: -1.0, y: 1.0 };
    let (text, style, max_width) = text_content(&layer);
    let metrics = renderer.measure_text(text, style, max_width);
    let region = rendered_text_box(&mut renderer, &mut scene, layer);
    let text_box = region.text_box.unwrap();
    // The layout's top-left is drawn at the right, as the quad's is.
    let bottom = 50.0 + 2.0 * metrics.line_height;
    assert_near_corners(
        text_box,
        [
            (300.0, 50.0),
            (100.0, 50.0),
            (100.0, bottom),
            (300.0, bottom),
        ]
        .map(|(x, y)| Point { x, y }),
    );
    assert!(
        region.corners[0].x > region.corners[1].x,
        "{:?}",
        region.corners
    );
}

#[test]
fn a_tilted_text_s_box_is_projected() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(600, 400);
    let layer = transform_3d::in_perspective(
        text_saying(
            "t",
            "First line\nSecond line",
            Point { x: 100.0, y: 100.0 },
            Some(300.0),
        ),
        40.0,
        0.0,
        800.0,
    );
    let homography = layer.transform.homography();
    let (text, style, max_width) = text_content(&layer);
    let metrics = renderer.measure_text(text, style, max_width);
    let region = rendered_text_box(&mut renderer, &mut scene, layer);
    let text_box = region.text_box.unwrap();
    // The layout rectangle, from the anchor (its top-left), projected.
    let height = 2.0 * metrics.line_height;
    let projected = [(0.0, 0.0), (300.0, 0.0), (300.0, height), (0.0, height)].map(|(x, y)| {
        let [x, y, _] = celesta_composition::project(&homography, x, y);
        Point { x, y }
    });
    assert_near_corners(text_box, projected);
    assert_near_corners(text_box, region.anchor_box.unwrap_or(region.corners));
}

fn assert_corners(region: &HitRegion, expected: [(f64, f64); 4]) {
    for (corner, (x, y)) in region.corners.iter().zip(expected) {
        assert!(
            (corner.x - x).abs() <= 0.01 && (corner.y - y).abs() <= 0.01,
            "{}: {:?} is not ({x}, {y})",
            region.layer_id,
            region.corners
        );
    }
}

#[test]
fn records_where_each_leaf_layer_was_drawn() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(400, 200);
    let mut hidden = corner_rect("z", 50.0, 50.0, 10.0, 10.0, "#ff0000");
    hidden.opacity = 0.0;
    // Nothing to draw either: an animated rect's first frame.
    let empty = corner_rect("empty", 60.0, 60.0, 0.0, 0.0, "#ff0000");
    let mut text = text_layer("t", Point { x: 300.0, y: 20.0 }, 1.0, 24.0);
    text.transform.rotation = 90.0;
    scene.layers = vec![
        corner_rect("a", 5.0, 5.0, 10.0, 10.0, "#ff0000"),
        hidden,
        empty,
        group(
            EvaluatedTransform {
                position: Point { x: 100.0, y: 0.0 },
                ..EvaluatedTransform::default()
            },
            vec![corner_rect("b", 5.0, 5.0, 10.0, 10.0, "#00ff00")],
        ),
        text,
        clipped_group(
            EvaluatedTransform {
                position: Point { x: 0.0, y: 100.0 },
                ..EvaluatedTransform::default()
            },
            Clip {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            vec![corner_rect("c", 0.0, 0.0, 40.0, 40.0, "#0000ff")],
        ),
    ];

    // Off by default.
    renderer.render(&scene).unwrap();
    assert!(renderer.hit_regions().is_empty());

    renderer.set_record_hit_regions(true);
    renderer.render(&scene).unwrap();
    let regions = renderer.hit_regions();
    let ids = regions
        .iter()
        .map(|region| region.layer_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["a", "b", "t", "c"]);
    assert_corners(
        &regions[0],
        [(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)],
    );
    assert_corners(
        &regions[1],
        [(105.0, 5.0), (115.0, 5.0), (115.0, 15.0), (105.0, 15.0)],
    );
    // Turned 90°: the text's top edge runs down the scene.
    let [first, second, ..] = regions[2].corners;
    assert!(
        (second.x - first.x).abs() <= 0.01,
        "{:?}",
        regions[2].corners
    );
    assert!(second.y - first.y > 1.0, "{:?}", regions[2].corners);
    assert!(regions[0].clips.is_empty());
    assert_eq!(regions[3].clips.len(), 1);
    assert_eq!(hit_test(regions, Point { x: 5.0, y: 105.0 }), Some("c"));
    // Inside the rect, outside its group's clip.
    assert_eq!(hit_test(regions, Point { x: 30.0, y: 130.0 }), None);

    // Taking the regions leaves none behind.
    let taken = renderer.take_hit_regions();
    assert_eq!(taken.len(), 4);
    assert!(renderer.hit_regions().is_empty());
    renderer.render(&scene).unwrap();
    assert_eq!(renderer.hit_regions(), taken.as_slice());

    // Turning recording off forgets the last frame's regions at once.
    renderer.set_record_hit_regions(false);
    assert!(renderer.hit_regions().is_empty());
    renderer.render(&scene).unwrap();
    assert!(renderer.hit_regions().is_empty());
}

#[test]
fn records_a_paths_shape_box_and_nothing_for_a_path_it_drops() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    renderer.set_record_hit_regions(true);
    let mut scene = empty_scene(200, 100);
    let square = |id: &str, from: f64, to: f64| {
        path_layer(
            id,
            polyline(&[(from, from), (to, from), (to, to), (from, to)], true),
            Some(solid("#ffffff")),
            None,
        )
    };
    scene.layers = vec![
        // Wholly outside the scene, so it covers no pixel.
        square("outside", 300.0, 340.0),
        square("p", 20.0, 40.0),
        corner_rect("after", 60.0, 10.0, 10.0, 10.0, "#ff0000"),
    ];
    renderer.render(&scene).unwrap();
    let regions = renderer.hit_regions();
    let ids = regions
        .iter()
        .map(|region| region.layer_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["p", "after"]);
    assert_corners(
        &regions[0],
        [(20.0, 20.0), (40.0, 20.0), (40.0, 40.0), (20.0, 40.0)],
    );
}

#[test]
fn records_the_hole_of_an_unfilled_stroked_rect() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    renderer.set_record_hit_regions(true);
    let outlined = |id: &str, x: f64, fill: Option<&str>, width: f64| {
        let mut layer = corner_rect(id, x, 10.0, 20.0, 20.0, "#ffffff");
        layer.content = LayerContent::Rect {
            width: 20.0,
            height: 20.0,
            fill: fill.map(solid),
            stroke: Some(Stroke {
                paint: solid("#ff0000"),
                width,
            }),
            corner_radius: 0.0,
        };
        layer
    };
    let mut scene = empty_scene(300, 100);
    scene.layers = vec![
        outlined("outline", 10.0, None, 2.0),
        outlined("clear", 40.0, Some("#00000000"), 2.0),
        outlined("filled-outline", 70.0, Some("#ffffff"), 2.0),
        // The stroke covers the whole rect: no hole.
        outlined("thick", 100.0, None, 10.0),
        corner_rect("filled", 130.0, 10.0, 20.0, 20.0, "#ffffff"),
    ];
    renderer.render(&scene).unwrap();
    let regions = renderer.hit_regions();
    let ids = regions
        .iter()
        .map(|region| region.layer_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        ["outline", "clear", "filled-outline", "thick", "filled"]
    );
    // The stroke is inside the rect's edge, as `rect_color` shades it.
    let hole = HitRegion {
        corners: regions[0].hole.expect("an unfilled outline has a hole"),
        ..regions[0].clone()
    };
    assert_corners(
        &hole,
        [(12.0, 12.0), (28.0, 12.0), (28.0, 28.0), (12.0, 28.0)],
    );
    assert!(regions[1].hole.is_some());
    for region in &regions[2..] {
        assert_eq!(region.hole, None, "{}", region.layer_id);
    }
    assert_eq!(
        hit_test(regions, Point { x: 11.0, y: 20.0 }),
        Some("outline")
    );
    assert_eq!(hit_test(regions, Point { x: 20.0, y: 20.0 }), None);
    assert_eq!(
        hit_test(regions, Point { x: 110.0, y: 20.0 }),
        Some("thick")
    );
}

#[test]
fn centers_visible_single_line_text_on_its_transform() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(1280, 720);
    scene.layers.push(Layer {
        id: "title".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 640.0, y: 360.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "Celesta".to_owned(),
            style: Box::new(TextStyle {
                font_size: Some(96.0),
                fill: Some(Paint::Solid {
                    color: "#FFA13BFF".to_owned(),
                }),
                align: Some(celesta_composition::TextAlign::Center),
                ..TextStyle::default()
            }),
            max_width: None,
            baseline_anchor: false,
        },
    });
    let frame = renderer.render(&scene).unwrap();
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    for (index, pixel) in frame.pixels().as_chunks::<4>().0.iter().enumerate() {
        if *pixel != [20, 22, 28, 255] {
            let x = index as u32 % frame.width();
            let y = index as u32 / frame.width();
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    let center_x = (min_x + max_x) as f32 / 2.0;
    let center_y = (min_y + max_y) as f32 / 2.0;
    // Horizontally the advance box is centered, so uneven side bearings
    // leave the ink a few pixels off.
    assert!((center_x - 640.0).abs() <= 4.0, "center x was {center_x}");
    assert!(
        (center_y - 360.0).abs() <= 0.5,
        "center y was {center_y} ({min_y}..{max_y})"
    );
}

#[test]
fn places_layers_on_half_pixel_positions_like_the_cpu_renderer() {
    // Nearest sampling picks a texel per pixel centre. With the quad's
    // left edge on a half pixel, every centre lands exactly on a texel
    // boundary, and f32 rounding chose a different neighbour per column:
    // glyph stems came out notched and shifted. Full HD, because the
    // error comes from the clip-space round trip at real canvas sizes.
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let text = |x: f64, anchor_x: f64, size: f64| Layer {
        id: format!("text-{x}"),
        transform: EvaluatedTransform {
            position: Point { x, y: 300.0 },
            anchor: Point {
                x: anchor_x,
                y: 0.0,
            },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "0 Hello, 15,000".to_owned(),
            style: Box::new(TextStyle {
                font_size: Some(size),
                font_weight: Some(900),
                fill: Some(Paint::Solid {
                    color: "#ffffff".to_owned(),
                }),
                ..TextStyle::default()
            }),
            max_width: None,
            baseline_anchor: false,
        },
    };
    let layers = [
        text(400.5, 0.0, 104.0),
        text(1001.5, 0.0, 104.0),
        text(37.5, 0.0, 30.0),
        text(960.0, 0.5, 104.0),
        text(961.0, 0.5, 104.0),
        corner_rect("odd-rect", 400.5, 700.0, 9.0, 40.0, "#ffe080"),
        corner_rect("fractional-rect", 1203.5, 700.0, 10.3, 40.0, "#40c0ff"),
    ];
    for layer in layers {
        let mut scene = empty_scene(1920, 1080);
        let id = layer.id.clone();
        scene.layers = vec![layer];
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 1,
            "{id}: channels differ by up to {difference}"
        );
    }
}

fn text_layer(id: &str, position: Point, scale: f64, font_size: f64) -> Layer {
    Layer {
        id: id.to_owned(),
        transform: EvaluatedTransform {
            position,
            scale: Point { x: scale, y: scale },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "Celesta 15,000".to_owned(),
            style: Box::new(TextStyle {
                font_size: Some(font_size),
                fill: Some(Paint::Solid {
                    color: "#ffffff".to_owned(),
                }),
                ..TextStyle::default()
            }),
            max_width: None,
            baseline_anchor: false,
        },
    }
}

/// An image layer drawing `image`, registered with `renderer` as `id`.
fn image_layer(
    renderer: &mut GpuRenderer,
    id: &str,
    image: DecodedImage,
    transform: EvaluatedTransform,
) -> Layer {
    renderer.image_sources.insert_raster(
        id,
        image::RgbaImage::from_raw(image.width, image.height, image.pixels.as_ref().clone())
            .unwrap(),
    );
    Layer {
        id: id.to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            asset: ResolvedAsset {
                id: id.to_owned(),
                location: AssetLocation::File {
                    path: id.to_owned(),
                },
            },
            width: None,
            height: None,
            fit: None,
            animation: None,
        },
    }
}

fn max_frame_difference(first: &GpuFrame, second: &GpuFrame) -> u8 {
    first
        .pixels()
        .iter()
        .zip(second.pixels())
        .map(|(first, second)| first.abs_diff(*second))
        .max()
        .unwrap()
}

#[test]
fn final_quality_rasterizes_scaled_text_at_its_drawn_size() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let position = Point { x: 40.0, y: 60.0 };
    let mut scene = empty_scene(640, 200);
    scene.layers = vec![text_layer("big", position, 1.0, 64.0)];
    let reference = renderer.render(&scene).unwrap();
    scene.layers = vec![text_layer("scaled", position, 2.0, 32.0)];

    assert_eq!(renderer.render_quality(), RenderQuality::Final);
    let scaled = renderer.render(&scene).unwrap();
    let difference = max_frame_difference(&scaled, &reference);
    assert!(difference <= 1, "final differs by up to {difference}");

    // Draft enlarges the scale-1 texture instead: visibly softer.
    renderer.set_render_quality(RenderQuality::Draft);
    let draft = renderer.render(&scene).unwrap();
    assert!(max_frame_difference(&draft, &reference) > 64);
}

#[test]
fn filters_enlarged_images_instead_of_repeating_texels() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    // A horizontal ramp. Nearest sampling at 1.28x repeats some columns,
    // which shows as stems of uneven width in enlarged artwork.
    let width = 40;
    let pixels = (0..width)
        .flat_map(|x| {
            let value = (x * 6) as u8;
            [value, value, value, 255]
        })
        .collect();
    let ramp = DecodedImage::new(width, 1, pixels).unwrap();
    let transform = EvaluatedTransform {
        position: Point { x: 4.0, y: 4.0 },
        scale: Point { x: 1.28, y: 8.0 },
        anchor: Point { x: 0.0, y: 0.0 },
        ..EvaluatedTransform::default()
    };
    let mut scene = empty_scene(64, 16);
    scene.layers = vec![image_layer(&mut renderer, "ramp", ramp, transform)];
    let frame = renderer.render(&scene).unwrap();
    // Row 8 is well inside the 8-pixel-tall layer; columns away from the
    // anti-aliased ends must rise strictly.
    let row: Vec<u8> = (6..50).map(|x| pixel_at(&frame, x, 8)[0]).collect();
    assert!(
        row.windows(2).all(|pair| pair[1] > pair[0]),
        "repeated or falling columns: {row:?}"
    );
}

#[test]
fn shrinks_cached_images_through_mipmaps() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    // A one-texel checkerboard averages to mid grey. Without mipmaps a
    // draw at under a quarter size picks a few texels per pixel and
    // aliases.
    let size = 128;
    let pixels = (0..size * size)
        .flat_map(|index| {
            let value = if (index % size + index / size) % 2 == 0 {
                255
            } else {
                0
            };
            [value, value, value, 255]
        })
        .collect();
    let checkerboard = DecodedImage::new(size, size, pixels).unwrap();
    // An uneven scale and offset, so bilinear taps do not happen to
    // straddle a black and a white texel evenly.
    let transform = EvaluatedTransform {
        position: Point { x: 0.37, y: 0.61 },
        scale: Point { x: 0.23, y: 0.23 },
        anchor: Point { x: 0.0, y: 0.0 },
        ..EvaluatedTransform::default()
    };
    let mut scene = empty_scene(32, 32);
    scene.layers = vec![image_layer(
        &mut renderer,
        "checkerboard",
        checkerboard,
        transform,
    )];
    let frame = renderer.render(&scene).unwrap();
    for y in 2..27 {
        for x in 2..27 {
            let value = pixel_at(&frame, x, y)[0];
            assert!(
                value.abs_diff(128) <= 16,
                "pixel ({x}, {y}) is {value}, not mid grey"
            );
        }
    }
}

#[test]
fn anti_aliases_the_edges_of_rotated_images() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let square = DecodedImage::new(20, 20, vec![255; 20 * 20 * 4]).unwrap();
    let transform = EvaluatedTransform {
        position: Point { x: 32.0, y: 32.0 },
        rotation: 30.0,
        ..EvaluatedTransform::default()
    };
    let mut scene = empty_scene(64, 64);
    scene.layers = vec![image_layer(&mut renderer, "square", square, transform)];
    let frame = renderer.render(&scene).unwrap();
    let partial = frame
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0 && pixel[3] < 255)
        .count();
    // About the perimeter (80 pixels) of partially covered edge pixels.
    assert!(partial >= 40, "only {partial} edge pixels are partial");
}

#[test]
fn rounds_text_raster_scales_up_to_eighths_of_an_octave() {
    let scaled = |scale: f32| {
        text_raster_scale(Affine {
            a: scale,
            d: scale,
            ..Affine::IDENTITY
        })
    };
    assert_eq!(scaled(1.0), 1.0);
    assert_eq!(scaled(2.0), 2.0);
    assert_eq!(scaled(0.5), 0.5);
    assert!((scaled(1.5) - 2_f32.powf(5.0 / 8.0)).abs() < 1e-6);
    assert!(scaled(1.01) >= 1.01);
    // Rotation does not change the size text is drawn at.
    let rotated = Affine::from_transform(&EvaluatedTransform {
        rotation: 33.0,
        scale: Point { x: 2.0, y: 2.0 },
        ..EvaluatedTransform::default()
    });
    assert!((text_raster_scale(rotated) - 2.0).abs() < 1e-5);
}

#[test]
fn mipmaps_average_color_by_coverage() {
    // An opaque white texel next to a transparent black one: the color
    // stays white rather than greying toward the transparent texel.
    let (width, height, pixels) = downsample(2, 1, &[255, 255, 255, 255, 0, 0, 0, 0]);
    assert_eq!((width, height), (1, 1));
    assert_eq!(pixels, [255, 255, 255, 128]);
    // Odd sizes fold the last column into its neighbour.
    let (width, _, pixels) = downsample(3, 1, &[30, 30, 30, 255].repeat(3));
    assert_eq!(width, 1);
    assert_eq!(pixels, [30, 30, 30, 255]);
}

#[test]
fn render_qualities_round_trip_through_their_names() {
    for &quality in RenderQuality::VARIANTS {
        assert_eq!(quality.as_str().parse::<RenderQuality>(), Ok(quality));
    }
    assert_eq!(
        "best".parse::<RenderQuality>().unwrap_err().to_string(),
        "unknown render quality 'best' (expected draft or final)"
    );
}

#[test]
fn a_stroked_text_layer_keeps_its_place_and_its_whole_stroke() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // Columns that differ from the black background, and the columns of
    // the white fill on its own.
    let mut extent = |stroke: Option<Stroke>| {
        let mut scene = empty_scene(640, 240);
        scene.layers.push(Layer {
            id: "title".to_owned(),
            transform: EvaluatedTransform {
                position: Point { x: 320.0, y: 120.0 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: "MW".to_owned(),
                style: Box::new(TextStyle {
                    font_size: Some(96.0),
                    fill: Some(Paint::Solid {
                        color: "#FFFFFFFF".to_owned(),
                    }),
                    stroke: stroke.map(Into::into),
                    ..TextStyle::default()
                }),
                max_width: None,
                baseline_anchor: false,
            },
        });
        let frame = renderer.render(&scene).unwrap();
        let (mut ink, mut fill) = ((u32::MAX, 0), (u32::MAX, 0));
        for (index, pixel) in frame.pixels().as_chunks::<4>().0.iter().enumerate() {
            let x = index as u32 % frame.width();
            if pixel[0] > 40 || pixel[1] > 40 {
                ink = (ink.0.min(x), ink.1.max(x));
            }
            if pixel[0] > 200 && pixel[2] > 200 {
                fill = (fill.0.min(x), fill.1.max(x));
            }
        }
        (ink, fill)
    };
    let (plain_ink, plain_fill) = extent(None);
    let (stroked_ink, stroked_fill) = extent(Some(Stroke {
        paint: Paint::Solid {
            color: "#FF0000FF".to_owned(),
        },
        width: 16.0,
    }));
    // The fill sits where it did without a stroke...
    assert!(
        plain_fill.0.abs_diff(stroked_fill.0) <= 1,
        "{plain_fill:?} {stroked_fill:?}"
    );
    assert!(
        plain_fill.1.abs_diff(stroked_fill.1) <= 1,
        "{plain_fill:?} {stroked_fill:?}"
    );
    // ...and the stroke reaches its full width past both ends of it.
    assert!(
        plain_ink.0 - stroked_ink.0 >= 14,
        "{plain_ink:?} {stroked_ink:?}"
    );
    assert!(
        stroked_ink.1 - plain_ink.1 >= 14,
        "{plain_ink:?} {stroked_ink:?}"
    );
}

#[test]
fn baseline_anchored_text_layers_share_a_baseline() {
    const BASELINE: u32 = 200;
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut scene = empty_scene(400, 300);
    for (text, x, font_size) in [("x", 20.0, 32.0), ("H", 110.0, 96.0), ("o", 230.0, 64.0)] {
        scene.layers.push(Layer {
            id: text.to_owned(),
            transform: EvaluatedTransform {
                position: Point {
                    x,
                    y: f64::from(BASELINE),
                },
                anchor: Point { x: 0.0, y: 0.5 },
                ..EvaluatedTransform::default()
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: Default::default(),
            content: LayerContent::Text {
                text: text.to_owned(),
                style: Box::new(TextStyle {
                    font_size: Some(font_size),
                    ..TextStyle::default()
                }),
                max_width: None,
                baseline_anchor: true,
            },
        });
    }
    let frame = renderer.render(&scene).unwrap();
    for (name, left, right) in [("x", 20, 100), ("H", 110, 220), ("o", 230, 310)] {
        let bottom = (0..frame.height())
            .rev()
            .find(|&y| {
                (left..right).any(|x| {
                    let offset = ((y * frame.width() + x) * 4) as usize;
                    frame.pixels()[offset] > 128
                })
            })
            .unwrap();
        assert!(
            bottom.abs_diff(BASELINE - 1) <= 2,
            "{name} ends on row {bottom}, not above the baseline at {BASELINE}"
        );
    }
}

/// Large blurs run at a half, a quarter, or an eighth of the resolution
/// (see `effect::reduced_blur`). Each factor must stay within the CPU
/// renderer's exact Gaussian, also where the kernel runs off the scene and
/// where a shadow's offset reads past its canvas.
#[test]
fn reduced_resolution_blurs_match_cpu() {
    use celesta_composition::{LayerEffects, LayerGlow, LayerShadow};

    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let shadow = |blur: f64, offset_x: f64, offset_y: f64| LayerEffects {
        shadow: Some(Box::new(LayerShadow {
            color: "#40c0ffd0".to_owned(),
            blur,
            offset_x,
            offset_y,
        })),
        ..LayerEffects::default()
    };
    let cases = [
        // Factor 2, 4 and 8, with the kernel running off the top edge.
        (
            LayerEffects {
                blur: 10.0,
                ..LayerEffects::default()
            },
            [30.0, 20.0, 70.0, 50.0],
        ),
        (
            LayerEffects {
                blur: 24.0,
                ..LayerEffects::default()
            },
            [30.0, 20.0, 70.0, 50.0],
        ),
        (
            LayerEffects {
                blur: 48.0,
                ..LayerEffects::default()
            },
            [100.0, 80.0, 70.0, 50.0],
        ),
        // Content against the left edge, its shadow shifted away from it,
        // so the shadow reads past the canvas.
        (shadow(17.0, 21.5, -6.25), [0.0, 70.0, 40.0, 42.0]),
        (
            LayerEffects {
                glow: Some(Box::new(LayerGlow {
                    color: "#ffe060ff".to_owned(),
                    blur: 64.0,
                })),
                ..LayerEffects::default()
            },
            [250.0, 150.0, 70.0, 74.0],
        ),
    ];
    for (effects, [x, y, width, height]) in cases {
        let mut scene = empty_scene(320, 224);
        scene.layers = vec![
            corner_rect("backdrop", 0.0, 0.0, 320.0, 224.0, "#101828"),
            Layer {
                id: "fx".to_owned(),
                transform: EvaluatedTransform::default(),
                opacity: 0.9,
                blend_mode: BlendMode::Normal,
                effects: effects.clone(),
                content: LayerContent::Group {
                    layers: vec![corner_rect("content", x, y, width, height, "#ff40a0")],
                    clip: None,
                    mask: None,
                    motion_blur: false,
                },
            },
        ];
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "{effects:?}: channels differ by up to {difference}"
        );
    }
}

#[test]
fn colored_text_and_reveals_match_cpu_through_cached_gpu_textures() {
    use celesta_composition::TextColorRun;
    let Some(mut gpu) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..Default::default()
    }) else {
        return;
    };
    let mut cpu = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: CpuColor::rgba(0, 0, 0, 0),
    });
    let text = "AV office e\u{301}\nאבג 😀";
    let mut scene = empty_scene(400, 180);
    scene.layers.push(Layer {
        id: "colored".into(),
        transform: EvaluatedTransform {
            position: Point { x: 12.0, y: 48.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..Default::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: text.into(),
            baseline_anchor: true,
            max_width: None,
            style: Box::new(TextStyle {
                font_size: Some(32.0),
                line_height: Some(44.0),
                color_runs: (0..text.chars().count())
                    .map(|start| TextColorRun {
                        start,
                        end: start + 1,
                        color: if start % 2 == 0 { "#ff0000" } else { "#00ff00" }.into(),
                    })
                    .collect(),
                ..Default::default()
            }),
        },
    });
    for visible in [None, Some(1), Some(8), Some(0), None] {
        let LayerContent::Text { style, .. } = &mut scene.layers[0].content else {
            unreachable!()
        };
        style.visible_characters = visible;
        let reference = cpu.render(&scene).unwrap();
        let frame = gpu.render(&scene).unwrap();
        // GPU readback is premultiplied; the CPU reference stores straight RGBA.
        assert_eq!(reference.pixels().len(), frame.pixels().len());
        assert!(
            reference
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(frame.pixels().as_chunks::<4>().0.iter())
                .all(|(cpu, gpu)| {
                    (0..4).all(|channel| {
                        let expected = if channel == 3 {
                            cpu[channel]
                        } else {
                            (u16::from(cpu[channel]) * u16::from(cpu[3]) / 255) as u8
                        };
                        expected.abs_diff(gpu[channel]) <= 1
                    })
                }),
            "visible={visible:?}"
        );
    }
}

#[test]
fn font_run_text_matches_cpu() {
    use celesta_composition::{TextColorRun, TextFontRun};
    let Some(mut gpu) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..Default::default()
    }) else {
        return;
    };
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/prism");
    gpu.set_asset_root(root);
    let mut cpu = celesta_renderer::CpuRenderer::new(celesta_renderer::RenderOptions {
        background: CpuColor::rgba(0, 0, 0, 0),
    })
    .with_asset_root(root);
    let mut scene = empty_scene(400, 180);
    scene.fonts = ["BebasNeue-Regular.ttf", "IBMPlexMono-Regular.ttf"]
        .map(|file| ResolvedAsset {
            id: file.to_owned(),
            location: AssetLocation::File {
                path: format!("assets/fonts/{file}"),
            },
        })
        .to_vec();
    scene.layers.push(Layer {
        id: "spanned".into(),
        transform: EvaluatedTransform {
            position: Point { x: 12.0, y: 48.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..Default::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "CELESTA plex\nRICH text".into(),
            baseline_anchor: true,
            max_width: None,
            style: Box::new(TextStyle {
                font_family: Some("Bebas Neue".into()),
                font_size: Some(40.0),
                line_height: Some(48.0),
                font_runs: vec![TextFontRun {
                    start: 8,
                    end: 12,
                    font_family: Some("IBM Plex Mono".into()),
                    ..TextFontRun::default()
                }],
                color_runs: vec![TextColorRun {
                    start: 18,
                    end: 22,
                    color: "#ff0000".into(),
                }],
                ..Default::default()
            }),
        },
    });
    let reference = cpu.render(&scene).unwrap();
    let frame = gpu.render(&scene).unwrap();
    assert_eq!(reference.pixels().len(), frame.pixels().len());
    assert!(
        reference
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(frame.pixels().as_chunks::<4>().0.iter())
            .all(|(cpu, gpu)| (0..4).all(|channel| {
                let expected = if channel == 3 {
                    cpu[channel]
                } else {
                    (u16::from(cpu[channel]) * u16::from(cpu[3]) / 255) as u8
                };
                expected.abs_diff(gpu[channel]) <= 1
            }))
    );
    // The run changes the glyphs: without it, the CPU reference differs.
    let mut plain_scene = scene.clone();
    let LayerContent::Text { style, .. } = &mut plain_scene.layers[0].content else {
        unreachable!()
    };
    style.font_runs.clear();
    assert_ne!(
        reference.pixels(),
        cpu.render(&plain_scene).unwrap().pixels()
    );
}

#[test]
fn an_unwrapped_colored_line_fits_the_gpu_texture_limit() {
    use crate::text::{PendingText, TextSize, rasterize_text};
    use celesta_composition::TextColorRun;
    let mut rasterizer = celesta_renderer::TextRasterizer::new();
    let text = "const n = 1; ".repeat(100);
    let style = TextStyle {
        font_size: Some(24.0),
        color_runs: vec![TextColorRun {
            start: 0,
            end: text.chars().count(),
            color: "#ff0000".into(),
        }],
        ..Default::default()
    };
    let job = PendingText {
        item: 0,
        key: "long-line".into(),
        size: TextSize::of(&rasterizer.measure(&text, &style, None), 0),
        style,
        text,
        max_width: None,
        raster_scale: 1.0,
        anchor: Point { x: 0.0, y: 0.0 },
        baseline_anchor: true,
        state: LayerState::default(),
        window: None,
        shape: None,
        blend_mode: BlendMode::Normal,
    };
    let image = rasterize_text(&mut rasterizer, &job, 1024).unwrap();
    assert!(image.width <= 1024 && image.height <= 1024 && image.raster_scale < 1.0);
}

fn alpha_mask(layers: Vec<Layer>) -> GroupMask {
    GroupMask {
        layers,
        mode: MaskMode::Alpha,
        invert: false,
    }
}

fn masked_group(transform: EvaluatedTransform, mask: GroupMask, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "masked".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: Some(mask),
            motion_blur: false,
        },
    }
}

/// A disc of `radius` centred on `(x, y)`, drawn as a fully rounded rect.
fn disc(id: &str, x: f64, y: f64, radius: f64, color: &str) -> Layer {
    let mut layer = corner_rect(
        id,
        x - radius,
        y - radius,
        radius * 2.0,
        radius * 2.0,
        color,
    );
    if let LayerContent::Rect { corner_radius, .. } = &mut layer.content {
        *corner_radius = radius;
    }
    layer
}

#[test]
fn masks_groups_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let red = || corner_rect("red", 0.0, 0.0, 64.0, 48.0, "#FF0000FF");
    let cases: Vec<(&str, Vec<Layer>)> = vec![
        (
            "alpha disc",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")]),
                vec![red()],
            )],
        ),
        (
            "luminance",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    mode: MaskMode::Luminance,
                    ..alpha_mask(vec![
                        corner_rect("gray", 0.0, 0.0, 64.0, 48.0, "#808080FF"),
                        disc("white", 30.0, 24.0, 10.0, "#FFFFFFFF"),
                    ])
                },
                vec![red()],
            )],
        ),
        (
            "translucent luminance",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    mode: MaskMode::Luminance,
                    ..alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFF80")])
                },
                vec![red()],
            )],
        ),
        (
            "blurred disc",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(vec![{
                    let mut matte = disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF");
                    matte.effects.blur = 4.0;
                    matte
                }]),
                vec![red()],
            )],
        ),
        (
            "inverted",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    invert: true,
                    ..alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")])
                },
                vec![red()],
            )],
        ),
        (
            "empty",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(Vec::new()),
                vec![red()],
            )],
        ),
        (
            "empty inverted",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    invert: true,
                    ..alpha_mask(Vec::new())
                },
                vec![red()],
            )],
        ),
        (
            "translated and scaled",
            vec![masked_group(
                EvaluatedTransform {
                    position: Point { x: 6.0, y: 4.0 },
                    scale: Point { x: 2.0, y: 2.0 },
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                alpha_mask(vec![disc("matte", 10.0, 8.0, 6.0, "#FFFFFFFF")]),
                vec![corner_rect("red", -4.0, -4.0, 40.0, 30.0, "#FF0000FF")],
            )],
        ),
        (
            "with a clip",
            vec![{
                let mut group = masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![disc("matte", 30.0, 24.0, 18.0, "#FFFFFFFF")]),
                    vec![red()],
                );
                if let LayerContent::Group { clip, .. } = &mut group.content {
                    *clip = Some(Clip {
                        x: 4.0,
                        y: 4.0,
                        width: 30.0,
                        height: 40.0,
                        corner_radius: 6.0,
                    });
                }
                group
            }],
        ),
        (
            "isolated multiply over a backdrop",
            vec![
                corner_rect("backdrop", 0.0, 0.0, 64.0, 48.0, "#80C0E0FF"),
                masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")]),
                    vec![Layer {
                        blend_mode: BlendMode::Multiply,
                        ..corner_rect("blue", 0.0, 0.0, 64.0, 48.0, "#3060F0FF")
                    }],
                ),
            ],
        ),
        (
            "the group's own blend mode",
            vec![
                corner_rect("backdrop", 0.0, 0.0, 64.0, 48.0, "#80C0E0FF"),
                Layer {
                    blend_mode: BlendMode::Multiply,
                    ..masked_group(
                        EvaluatedTransform::default(),
                        alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")]),
                        vec![red()],
                    )
                },
            ],
        ),
        (
            "a blended layer inside the mask",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    mode: MaskMode::Luminance,
                    ..alpha_mask(vec![
                        corner_rect("dark", 0.0, 0.0, 64.0, 48.0, "#404040FF"),
                        Layer {
                            blend_mode: BlendMode::Screen,
                            ..disc("light", 30.0, 24.0, 14.0, "#808080FF")
                        },
                    ])
                },
                vec![red()],
            )],
        ),
        (
            "off the scene edge",
            vec![masked_group(
                EvaluatedTransform {
                    position: Point { x: 40.0, y: 30.0 },
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                alpha_mask(vec![disc("matte", 16.0, 12.0, 20.0, "#FFFFFFFF")]),
                vec![corner_rect("red", 0.0, 0.0, 60.0, 60.0, "#FF0000FF")],
            )],
        ),
        (
            "nested",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(vec![corner_rect(
                    "outer",
                    0.0,
                    0.0,
                    36.0,
                    48.0,
                    "#FFFFFFFF",
                )]),
                vec![masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![disc("inner", 30.0, 24.0, 16.0, "#FFFFFFFF")]),
                    vec![red()],
                )],
            )],
        ),
        (
            "a mask inside a mask's layers",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(vec![masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![corner_rect(
                        "stripe",
                        0.0,
                        0.0,
                        36.0,
                        48.0,
                        "#FFFFFFFF",
                    )]),
                    vec![disc("inner", 30.0, 24.0, 16.0, "#FFFFFFFF")],
                )]),
                vec![red()],
            )],
        ),
        (
            "group and mask layer opacity",
            vec![Layer {
                opacity: 0.5,
                ..masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![Layer {
                        opacity: 0.5,
                        ..disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")
                    }]),
                    vec![red()],
                )
            }],
        ),
    ];
    for (case, layers) in cases {
        let mut scene = empty_scene(64, 48);
        scene.layers = layers;
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 3,
            "{case}: channels differ by up to {difference}"
        );
    }
}

#[test]
fn a_shadow_follows_a_masked_shape_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut group = masked_group(
        EvaluatedTransform::default(),
        alpha_mask(vec![disc("matte", 24.0, 24.0, 12.0, "#FFFFFFFF")]),
        vec![corner_rect("red", 0.0, 0.0, 64.0, 48.0, "#FF0000FF")],
    );
    group.effects.shadow = Some(Box::new(LayerShadow {
        color: "#000000C0".to_owned(),
        blur: 2.0,
        offset_x: 12.0,
        offset_y: 4.0,
    }));
    let mut scene = empty_scene(64, 48);
    scene.layers = vec![group];
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    let difference = max_channel_difference(&gpu, &cpu);
    assert!(difference <= 5, "channels differ by up to {difference}");
}

#[test]
fn masks_on_cropped_canvases_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // Far bigger than `CanvasRegion::STEP`, so a small mask's canvas is
    // cropped and sits away from the scene's origin.
    let masked = || {
        masked_group(
            EvaluatedTransform::default(),
            GroupMask {
                mode: MaskMode::Luminance,
                ..alpha_mask(vec![
                    corner_rect("dark", 170.0, 130.0, 60.0, 60.0, "#404040FF"),
                    Layer {
                        blend_mode: BlendMode::Screen,
                        ..disc("light", 200.0, 160.0, 20.0, "#C0C0C0FF")
                    },
                ])
            },
            vec![corner_rect("red", 150.0, 120.0, 150.0, 140.0, "#FF0000FF")],
        )
    };
    let cases: Vec<(&str, Vec<Layer>)> = vec![
        ("cropped", vec![masked()]),
        (
            "cropped inside a glowing, translated group",
            vec![Layer {
                effects: celesta_composition::LayerEffects {
                    glow: Some(Box::new(celesta_composition::LayerGlow {
                        color: "#00FF00C0".to_owned(),
                        blur: 6.0,
                    })),
                    ..Default::default()
                },
                ..group(
                    EvaluatedTransform {
                        position: Point { x: 40.0, y: 30.0 },
                        anchor: Point { x: 0.0, y: 0.0 },
                        ..EvaluatedTransform::default()
                    },
                    vec![masked()],
                )
            }],
        ),
    ];
    for (case, layers) in cases {
        let mut scene = empty_scene(512, 384);
        scene.layers = layers;
        let draws = renderer.prepare_draws(&scene).unwrap();
        let canvas = draws
            .steps
            .iter()
            .find_map(|step| match step {
                GpuStep::BeginMask { canvas } => Some(*canvas),
                _ => None,
            })
            .expect("the scene has a mask");
        assert!(
            canvas.x > 0 && canvas.y > 0 && canvas.width < 512 && canvas.height < 384,
            "{case}: the mask's canvas is not cropped: {canvas:?}"
        );
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(
            difference <= 5,
            "{case}: channels differ by up to {difference}"
        );
    }
}

#[test]
fn a_path_masks_a_group_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let diamond = path_layer(
        "diamond",
        polyline(
            &[(48.0, 4.0), (88.0, 32.0), (48.0, 60.0), (8.0, 32.0)],
            true,
        ),
        Some(Paint::Solid {
            color: "#FFFFFFFF".to_owned(),
        }),
        None,
    );
    assert_paths_match_cpu(
        &mut renderer,
        "path mask",
        vec![masked_group(
            EvaluatedTransform::default(),
            alpha_mask(vec![diamond]),
            vec![corner_rect("red", 0.0, 0.0, 96.0, 64.0, "#FF0000FF")],
        )],
    );
}

#[test]
fn text_masks_a_group() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let text = Layer {
        id: "word".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 48.0, y: 32.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "MASK".to_owned(),
            style: Box::new(TextStyle {
                font_size: Some(36.0),
                ..TextStyle::default()
            }),
            max_width: None,
            baseline_anchor: false,
        },
    };
    let mut scene = empty_scene(96, 64);
    scene.layers = vec![masked_group(
        EvaluatedTransform::default(),
        alpha_mask(vec![text]),
        vec![corner_rect("red", 0.0, 0.0, 96.0, 64.0, "#FF0000FF")],
    )];
    let frame = renderer.render(&scene).unwrap();
    let red = frame
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel == [255, 0, 0, 255])
        .count();
    // The glyphs show some of the red, and their gaps hide the rest.
    assert!(red > 50, "only {red} red pixels");
    assert!(red < 96 * 64 / 2, "{red} red pixels: the text did not mask");
    assert_ne!(pixel_at(&frame, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn an_image_masks_a_group_by_luminance() {
    let Some(renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut renderer = renderer.with_asset_root(env!("CARGO_MANIFEST_DIR"));
    // The 2x2 checker holds red, green / blue, white, row by row.
    let checker = Layer {
        id: "checker".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            width: None,
            height: None,
            fit: None,
            animation: None,
            asset: ResolvedAsset {
                id: "checker".to_owned(),
                location: AssetLocation::File {
                    path: "tests/assets/checker.ppm".to_owned(),
                },
            },
        },
    };
    let mut scene = empty_scene(2, 2);
    scene.layers = vec![masked_group(
        EvaluatedTransform::default(),
        GroupMask {
            mode: MaskMode::Luminance,
            ..alpha_mask(vec![checker])
        },
        vec![corner_rect("white", 0.0, 0.0, 2.0, 2.0, "#FFFFFFFF")],
    )];
    let frame = renderer.render(&scene).unwrap();
    // White shown through each texel's luma: premultiplied, every channel
    // is the alpha.
    for ((x, y), luma) in [((0, 0), 54), ((1, 0), 182), ((0, 1), 18), ((1, 1), 255)] {
        let pixel = pixel_at(&frame, x, y);
        for channel in pixel {
            assert!(
                channel.abs_diff(luma) <= 1,
                "({x}, {y}): {pixel:?}, expected {luma} in every channel"
            );
        }
    }
}

#[test]
fn a_mask_rotates_with_its_group() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // As in `a_clip_rotates_with_its_group`: turned 90 degrees clockwise
    // about (20, 20), the mask's 20x10 rect lands on canvas x 10..20,
    // y 20..40.
    let mut scene = empty_scene(40, 40);
    scene.layers = vec![masked_group(
        EvaluatedTransform {
            position: Point { x: 20.0, y: 20.0 },
            rotation: 90.0,
            ..EvaluatedTransform::default()
        },
        alpha_mask(vec![corner_rect(
            "matte",
            0.0,
            0.0,
            20.0,
            10.0,
            "#FFFFFFFF",
        )]),
        vec![corner_rect("red", -40.0, -40.0, 80.0, 80.0, "#FF0000FF")],
    )];
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(pixel_at(&frame, 15, 30), [255, 0, 0, 255]);
    let background = renderer.options().background;
    let background = [
        background.red,
        background.green,
        background.blue,
        background.alpha,
    ];
    for (x, y) in [(8, 30), (22, 30), (15, 18)] {
        assert_eq!(pixel_at(&frame, x, y), background, "({x}, {y}) leaked");
    }
}

#[test]
fn a_mask_off_the_children_draws_nothing_unless_inverted() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let group = |invert| {
        masked_group(
            EvaluatedTransform::default(),
            GroupMask {
                invert,
                ..alpha_mask(vec![corner_rect(
                    "matte",
                    30.0,
                    30.0,
                    8.0,
                    8.0,
                    "#FFFFFFFF",
                )])
            },
            vec![corner_rect("red", 0.0, 0.0, 16.0, 16.0, "#FF0000FF")],
        )
    };
    let background = renderer.options().background;
    let background = [
        background.red,
        background.green,
        background.blue,
        background.alpha,
    ];
    let mut scene = empty_scene(40, 40);
    scene.layers = vec![group(false)];
    let hidden = renderer.render(&scene).unwrap();
    for (x, y) in [(0, 0), (8, 8), (15, 15), (34, 34)] {
        assert_eq!(pixel_at(&hidden, x, y), background, "({x}, {y}) drawn");
    }
    scene.layers = vec![group(true)];
    let shown = renderer.render(&scene).unwrap();
    for (x, y) in [(0, 0), (8, 8), (15, 15)] {
        assert_eq!(
            pixel_at(&shown, x, y),
            [255, 0, 0, 255],
            "({x}, {y}) hidden"
        );
    }
    // The mask itself never draws, inverted or not.
    assert_eq!(pixel_at(&shown, 34, 34), background);
}
