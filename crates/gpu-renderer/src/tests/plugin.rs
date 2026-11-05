//! `LayerContent::Plugin`: meshes from a content provider, drawn with masks,
//! colors, opacity, and culling. A provider made up here stands in for a
//! plugin process.

use super::*;
use celesta_plugin::{
    ContentFrame, ContentProvider, MeshBlend, MeshFrame, MeshInfo, MeshMask, MeshModel, MeshState,
    ModelId, Notice, NoticeLevel, Opened, PluginError, TextureSource,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A 64x64 canvas: mesh 0 covers its left half and is hidden, only masking;
/// mesh 1 covers it all in green, inside mesh 0; mesh 2 covers it all in
/// half-transparent blue, outside mesh 0. Meshes sample one white texture.
/// With `shift`, every mesh lists positions moved right by `time` pixels.
/// Opening the model reports `notices`.
struct Quads {
    model: MeshModel,
    notices: Vec<Notice>,
}

/// A quad whose corners `positions` gives, top-left first, clockwise.
fn quad(mask: Option<MeshMask>) -> MeshInfo {
    MeshInfo {
        texture: 0,
        blend: MeshBlend::Normal,
        double_sided: true,
        mask,
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        indices: vec![0, 2, 1, 0, 3, 2],
    }
}

fn positions(left: f32, right: f32) -> Vec<[f32; 2]> {
    vec![[left, 0.0], [right, 0.0], [right, 64.0], [left, 64.0]]
}

impl Quads {
    fn new() -> Self {
        Self {
            model: MeshModel {
                canvas: [64.0, 64.0],
                textures: vec![TextureSource::Rgba {
                    width: 2,
                    height: 2,
                    pixels: Arc::new(vec![255; 16]),
                }],
                meshes: vec![
                    quad(None),
                    quad(Some(MeshMask {
                        meshes: vec![0],
                        inverted: false,
                    })),
                    quad(Some(MeshMask {
                        meshes: vec![0],
                        inverted: true,
                    })),
                ],
            },
            notices: vec![],
        }
    }
}

impl ContentProvider for Quads {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        if plugin != "quads" {
            return Err(PluginError::UnknownPlugin(plugin.to_owned()));
        }
        assert!(assets.is_empty());
        Ok(Opened {
            model: ModelId(7),
            content: self.model.clone().into(),
            notices: self.notices.clone(),
        })
    }

    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        assert_eq!(model, ModelId(7));
        let shift = time as f32;
        let state = |index, opacity, color: [f32; 3], left: f32, right: f32| MeshState {
            index,
            visible: index != 0,
            opacity,
            multiply: color,
            screen: [0.0; 3],
            positions: positions(left + shift, right + shift),
        };
        let mut meshes = vec![
            state(0, 1.0, [1.0; 3], 0.0, 32.0),
            state(1, 1.0, [0.0, 1.0, 0.0], 0.0, 64.0),
            state(2, 0.5, [0.0, 0.0, 1.0], 0.0, 64.0),
        ];
        if data["broken"] == true {
            meshes[1].index = 9;
        }
        Ok(MeshFrame { meshes }.into())
    }
}

fn scene(time: f64, data: serde_json::Value) -> Scene {
    let mut scene = empty_scene(64, 64);
    scene.layers.push(Layer {
        id: "quads".into(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Plugin {
            plugin: "quads".into(),
            assets: BTreeMap::new(),
            time,
            data,
        },
    });
    scene
}

fn transparent() -> GpuRenderOptions {
    GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 0),
        ..GpuRenderOptions::default()
    }
}

fn pixel(frame: &GpuFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width + x) * 4) as usize;
    frame.pixels[offset..offset + 4].try_into().unwrap()
}

fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2)
}

#[test]
fn draws_meshes_through_their_masks() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let mut renderer = renderer;
    renderer.renderer = renderer.renderer.with_content_provider(Quads::new());
    let frame = renderer
        .render(&scene(0.0, serde_json::Value::Null))
        .unwrap();
    // Inside the hidden mask: the green mesh; outside: the half-blue one.
    // Readback is premultiplied.
    assert!(
        near(pixel(&frame, 16, 32), [0, 255, 0, 255]),
        "{:?}",
        pixel(&frame, 16, 32)
    );
    assert!(
        near(pixel(&frame, 48, 32), [0, 0, 128, 128]),
        "{:?}",
        pixel(&frame, 48, 32)
    );

    // The time moves the meshes, and the mask with them.
    let moved = renderer
        .render(&scene(16.0, serde_json::Value::Null))
        .unwrap();
    assert!(
        near(pixel(&moved, 8, 32), [0, 0, 0, 0]),
        "{:?}",
        pixel(&moved, 8, 32)
    );
    assert!(
        near(pixel(&moved, 40, 32), [0, 255, 0, 255]),
        "{:?}",
        pixel(&moved, 40, 32)
    );
}

#[test]
fn refuses_frames_that_do_not_fit_the_model() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let mut renderer = renderer;
    renderer.renderer = renderer.renderer.with_content_provider(Quads::new());
    let error = renderer
        .render(&scene(0.0, serde_json::json!({ "broken": true })))
        .unwrap_err();
    assert!(
        matches!(error, GpuRenderError::InvalidPluginContent { .. }),
        "{error}"
    );
    // A later frame still draws.
    renderer
        .render(&scene(0.0, serde_json::Value::Null))
        .unwrap();
}

#[test]
fn plugin_notices_are_reported_per_frame() {
    let Some(renderer) = renderer(transparent()) else {
        return;
    };
    let mut renderer = renderer;
    let mut quads = Quads::new();
    quads.notices = vec![
        Notice {
            level: NoticeLevel::Info,
            message: "made by someone".into(),
        },
        Notice {
            level: NoticeLevel::Warning,
            message: "not for commercial use".into(),
        },
    ];
    renderer.renderer = renderer.renderer.with_content_provider(quads);
    // Two layers draw the one model: each notice is listed once.
    let mut twice = scene(0.0, serde_json::Value::Null);
    let mut copy = twice.layers[0].clone();
    copy.id = "copy".into();
    twice.layers.push(copy);
    let expected = [
        "note: quads (layer quads): made by someone",
        "quads (layer quads): not for commercial use",
    ];
    renderer.render(&twice).unwrap();
    let listed: Vec<String> = renderer
        .plugin_notices()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(listed, expected);

    // The model is already open, and its notices still hold.
    renderer.render(&twice).unwrap();
    let listed: Vec<String> = renderer
        .plugin_notices()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(listed, expected);

    renderer.render(&empty_scene(64, 64)).unwrap();
    assert!(renderer.plugin_notices().is_empty());
}

#[test]
fn plugin_content_needs_a_provider() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let error = renderer
        .render(&scene(0.0, serde_json::Value::Null))
        .unwrap_err();
    assert!(
        matches!(error, GpuRenderError::MissingContentProvider(_)),
        "{error}"
    );
}

#[test]
fn the_cpu_renderer_refuses_plugin_content() {
    let error = celesta_renderer::CpuRenderer::new(Default::default())
        .render(&scene(0.0, serde_json::Value::Null))
        .unwrap_err();
    assert!(error.to_string().contains("plugin"), "{error}");
}

/// Why the GPU renderer, with at most `max` texels a texture side (its own
/// limit without one), refuses the model with only `texture`: the layer it
/// names and the message. `None` without a GPU.
fn refused_texture(texture: TextureSource, max: Option<u32>) -> Option<(String, String)> {
    let mut renderer = renderer(transparent())?;
    if let Some(max) = max {
        renderer.max_texture_dimension = max;
    }
    let mut quads = Quads::new();
    quads.model.textures = vec![texture];
    renderer.renderer = renderer.renderer.with_content_provider(quads);
    match renderer.render(&scene(0.0, serde_json::Value::Null)) {
        Err(GpuRenderError::InvalidPluginContent { layer, message }) => Some((layer, message)),
        Err(error) => panic!("expected invalid plugin content, got {error}"),
        Ok(_) => panic!("expected invalid plugin content, got a frame"),
    }
}

fn rgba(width: u32, height: u32) -> TextureSource {
    TextureSource::Rgba {
        width,
        height,
        pixels: Arc::new(vec![255; width as usize * height as usize * 4]),
    }
}

#[test]
fn refuses_mesh_textures_without_a_texel() {
    for (width, height) in [(0, 0), (0, 4), (4, 0)] {
        let Some((layer, message)) = refused_texture(rgba(width, height), None) else {
            return;
        };
        assert_eq!(layer, "quads");
        assert!(
            message.starts_with(&format!("texture 0 is {width}x{height}, outside 1 to ")),
            "{message}"
        );
    }
}

#[test]
fn refuses_mesh_textures_past_the_gpu_limit() {
    // The renderer asks for wgpu's default limits: 8192 texels a side.
    let Some((layer, message)) = refused_texture(rgba(8193, 1), None) else {
        return;
    };
    assert_eq!(layer, "quads");
    assert_eq!(
        message,
        "texture 0 is 8193x1, outside 1 to 8192 texels a side"
    );
}

#[test]
fn refuses_an_oversized_texture_file_from_its_header() {
    // A 5x1 PNG cut off inside its pixel data: its header reads, its pixels
    // do not decode.
    let mut png = Vec::new();
    image::RgbaImage::new(5, 1)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let data = png.windows(4).position(|name| name == b"IDAT").unwrap();
    png.truncate(data + 6);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.png");
    std::fs::write(&path, &png).unwrap();

    let Some((layer, message)) = refused_texture(TextureSource::Path(path.clone()), Some(4)) else {
        return;
    };
    assert_eq!(layer, "quads");
    assert_eq!(message, "texture 0 is 5x1, outside 1 to 4 texels a side");

    // Within the limit, the file is decoded, and its cut-off pixels fail.
    let mut renderer = renderer(transparent()).unwrap();
    let mut quads = Quads::new();
    quads.model.textures = vec![TextureSource::Path(path)];
    renderer.renderer = renderer.renderer.with_content_provider(quads);
    let error = renderer
        .render(&scene(0.0, serde_json::Value::Null))
        .unwrap_err();
    assert!(
        matches!(error, GpuRenderError::ImageDecode { .. }),
        "{error}"
    );
}
