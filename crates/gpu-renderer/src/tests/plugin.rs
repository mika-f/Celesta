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

/// `Quads` under any plugin name, giving each name a model id of its own,
/// listing the models it opens and closes.
struct Counting {
    quads: Quads,
    log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Counting {
    fn id(plugin: &str) -> ModelId {
        ModelId(u64::from(plugin.as_bytes()[0]))
    }
}

impl ContentProvider for Counting {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        self.log.lock().unwrap().push(format!("open {plugin}"));
        let opened = self.quads.open("quads", assets)?;
        Ok(Opened {
            model: Self::id(plugin),
            ..opened
        })
    }

    fn frame(
        &mut self,
        _model: ModelId,
        time: f64,
        data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        self.quads.frame(ModelId(7), time, data)
    }

    fn close(&mut self, model: ModelId) {
        let name = char::from(model.0 as u8);
        self.log.lock().unwrap().push(format!("close {name}"));
    }
}

/// `scene`'s layer drawn by `plugin`.
fn scene_of(plugin: &str) -> Scene {
    let mut scene = scene(0.0, serde_json::Value::Null);
    let LayerContent::Plugin { plugin: name, .. } = &mut scene.layers[0].content else {
        unreachable!("a plugin layer");
    };
    *name = plugin.to_owned();
    scene
}

#[test]
fn models_no_layer_draws_for_a_while_are_closed() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let log = Arc::default();
    renderer.renderer = renderer.renderer.with_content_provider(Counting {
        quads: Quads::new(),
        log: Arc::clone(&log),
    });
    let logged = || log.lock().unwrap().clone();
    renderer.render(&scene_of("a")).unwrap();
    // Only "b" is drawn now: "a" stays open while a cut back to it is
    // still likely.
    for _ in 1..crate::mesh::MODEL_IDLE_RENDERS {
        renderer.render(&scene_of("b")).unwrap();
    }
    assert_eq!(logged(), ["open a", "open b"]);
    renderer.render(&scene_of("b")).unwrap();
    assert_eq!(logged(), ["open a", "open b", "close a"]);
    // Renders that draw no plugin layer count too: "b", drawn in each
    // render until now, closes after as many of them.
    for _ in 1..crate::mesh::MODEL_IDLE_RENDERS {
        renderer.render(&empty_scene(64, 64)).unwrap();
    }
    assert_eq!(logged(), ["open a", "open b", "close a"]);
    renderer.render(&empty_scene(64, 64)).unwrap();
    assert_eq!(logged(), ["open a", "open b", "close a", "close b"]);
    // A closed model opens again when a layer draws it.
    renderer.render(&scene_of("a")).unwrap();
    assert_eq!(
        logged(),
        ["open a", "open b", "close a", "close b", "open a"]
    );
}

/// `provider`, counting how often a model is opened.
pub(super) struct Counted<P> {
    provider: P,
    opens: Arc<std::sync::atomic::AtomicUsize>,
}

impl<P: ContentProvider> Counted<P> {
    pub(super) fn new(provider: P) -> (Self, Arc<std::sync::atomic::AtomicUsize>) {
        let opens = Arc::default();
        let counted = Self {
            provider,
            opens: Arc::clone(&opens),
        };
        (counted, opens)
    }
}

impl<P: ContentProvider> ContentProvider for Counted<P> {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.provider.open(plugin, assets)
    }

    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        self.provider.frame(model, time, data)
    }

    fn close(&mut self, model: ModelId) {
        self.provider.close(model);
    }
}

pub(super) fn opens(count: &std::sync::atomic::AtomicUsize) -> usize {
    count.load(std::sync::atomic::Ordering::SeqCst)
}

/// A provider that cannot open any model.
struct Refusing;

impl ContentProvider for Refusing {
    fn open(
        &mut self,
        plugin: &str,
        _assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        Err(PluginError::Failed {
            plugin: plugin.to_owned(),
            message: "no model here".to_owned(),
        })
    }

    fn frame(
        &mut self,
        _model: ModelId,
        _time: f64,
        _data: &serde_json::Value,
    ) -> Result<ContentFrame, PluginError> {
        unreachable!("nothing was opened")
    }
}

/// `scene`'s layer, drawn by `plugin` from `assets`, files by name.
fn plugin_scene(plugin: &str, assets: &[(&str, &Path)]) -> Scene {
    let mut scene = scene(0.0, serde_json::Value::Null);
    let LayerContent::Plugin {
        plugin: layer_plugin,
        assets: layer_assets,
        ..
    } = &mut scene.layers[0].content
    else {
        unreachable!()
    };
    *layer_plugin = plugin.to_owned();
    for (name, path) in assets {
        layer_assets.insert(
            (*name).to_owned(),
            ResolvedAsset {
                id: (*name).to_owned(),
                location: AssetLocation::File {
                    path: path.display().to_string(),
                },
            },
        );
    }
    scene
}

#[test]
fn a_model_that_fails_to_open_is_not_opened_again() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let (provider, count) = Counted::new(Refusing);
    renderer.renderer = renderer.renderer.with_content_provider(provider);
    let scene = plugin_scene("refusing", &[]);
    let first = renderer.render(&scene).unwrap_err();
    assert!(matches!(first, GpuRenderError::Plugin { .. }), "{first}");
    // Each frame still fails, as the first did, so the editor shows it.
    let second = renderer.render(&scene).unwrap_err();
    assert!(matches!(second, GpuRenderError::Plugin { .. }), "{second}");
    assert_eq!(second.to_string(), first.to_string());
    assert_eq!(opens(&count), 1);
}

#[test]
fn a_model_that_fails_to_load_is_not_opened_again() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.png");
    std::fs::write(&path, b"not an image").unwrap();
    let mut quads = Quads::new();
    quads.model.textures = vec![TextureSource::Path(path)];
    let (provider, count) = Counted::new(quads);
    renderer.renderer = renderer.renderer.with_content_provider(provider);
    let scene = scene(0.0, serde_json::Value::Null);
    let first = renderer.render(&scene).unwrap_err();
    assert!(
        matches!(first, GpuRenderError::ImageDecode { .. }),
        "{first}"
    );
    let second = renderer.render(&scene).unwrap_err();
    assert!(
        matches!(second, GpuRenderError::ImageDecode { .. }),
        "{second}"
    );
    assert_eq!(second.to_string(), first.to_string());
    assert_eq!(opens(&count), 1);

    // A refused model too.
    let Some(mut renderer) = self::renderer(transparent()) else {
        return;
    };
    let mut quads = Quads::new();
    quads.model.textures = vec![rgba(0, 0)];
    let (provider, count) = Counted::new(quads);
    renderer.renderer = renderer.renderer.with_content_provider(provider);
    let first = renderer.render(&scene).unwrap_err();
    let second = renderer.render(&scene).unwrap_err();
    assert!(
        matches!(&second, GpuRenderError::InvalidPluginContent { layer, .. } if layer == "quads"),
        "{second}"
    );
    assert_eq!(second.to_string(), first.to_string());
    assert_eq!(opens(&count), 1);
}

#[test]
fn a_failed_model_is_opened_again_once_its_assets_change() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.bin");
    std::fs::write(&path, b"a").unwrap();
    let (provider, count) = Counted::new(Refusing);
    renderer.renderer = renderer.renderer.with_content_provider(provider);
    let scene = plugin_scene("refusing", &[("model", &path)]);
    renderer.render(&scene).unwrap_err();
    renderer.render(&scene).unwrap_err();
    assert_eq!(opens(&count), 1);
    // Replaced in place, as by fixing it: the same path, another length.
    std::fs::write(&path, b"bb").unwrap();
    renderer.render(&scene).unwrap_err();
    renderer.render(&scene).unwrap_err();
    assert_eq!(opens(&count), 2);
}

#[test]
fn a_failed_model_is_opened_again_once_a_texture_file_it_lists_changes() {
    let Some(mut renderer) = renderer(transparent()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("texture.png");
    std::fs::write(&path, b"not an image").unwrap();
    let mut quads = Quads::new();
    quads.model.textures = vec![TextureSource::Path(path.clone())];
    let (provider, count) = Counted::new(quads);
    renderer.renderer = renderer.renderer.with_content_provider(provider);
    let scene = scene(0.0, serde_json::Value::Null);
    renderer.render(&scene).unwrap_err();
    renderer.render(&scene).unwrap_err();
    assert_eq!(opens(&count), 1);
    // The layer names no asset; the plugin's model names the file.
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
        .save(&path)
        .unwrap();
    renderer.render(&scene).unwrap();
    assert_eq!(opens(&count), 2);
}

/// Plugin processes that never open a model: one that says it is another
/// plugin, and one that exits as it is asked to open. Two frames start each
/// at most twice in all: the second asks for nothing.
#[cfg(unix)]
#[test]
fn a_plugin_that_cannot_open_is_not_started_for_every_frame() {
    use celesta_plugin::protocol::{Message, PROTOCOL_VERSION, write_message};
    use celesta_plugin_host::{Manifest, PluginEntry, PluginHost, PluginRole, PluginSource};
    use std::os::unix::fs::PermissionsExt;

    for (says, body, most) in [
        ("someone-else", "cat hello\nexec cat >/dev/null", 1),
        ("p", "cat hello", 2),
    ] {
        let Some(mut renderer) = renderer(transparent()) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let mut hello = Vec::new();
        write_message(
            &mut hello,
            &Message::new(
                serde_json::json!({ "type": "hello", "protocol": PROTOCOL_VERSION, "plugin": says }),
            ),
        )
        .unwrap();
        std::fs::write(dir.path().join("hello"), hello).unwrap();
        let program = dir.path().join("plugin.sh");
        std::fs::write(&program, format!("#!/bin/sh\necho >> starts\n{body}\n")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let entry = PluginEntry {
            manifest: Manifest {
                id: "p".into(),
                version: "1".into(),
                protocol: PROTOCOL_VERSION,
                celesta: None,
                executable: [(
                    celesta_plugin_host::platform().0.to_owned(),
                    "plugin.sh".to_owned(),
                )]
                .into(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![PluginRole::Content],
                media: None,
            },
            directory: dir.path().to_owned(),
            source: PluginSource::Explicit,
        };
        renderer.renderer = renderer
            .renderer
            .with_content_provider(PluginHost::new([entry]));
        let scene = plugin_scene("p", &[]);
        let first = renderer.render(&scene).unwrap_err();
        let second = renderer.render(&scene).unwrap_err();
        assert_eq!(second.to_string(), first.to_string());
        let starts = std::fs::read_to_string(dir.path().join("starts"))
            .unwrap()
            .lines()
            .count();
        assert_eq!(starts, most, "{first}");
    }
}
