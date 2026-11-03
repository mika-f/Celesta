//! `LayerContent::Plugin`: a content provider (a plugin) supplies textured
//! meshes, and they are drawn here onto a canvas of their own,
//! premultiplied, which then composites as the layer's texture. A mesh can
//! be clipped by other meshes, tinted with multiply and screen colors,
//! culled when it faces away, and blended normally, additively, or
//! multiplicatively. A provider can supply a 3D scene instead; `scene.rs`
//! draws those onto the same kind of canvas.

use crate::error::GpuRenderError;
use crate::renderer::GpuRenderer;
use crate::scene::{LoadedScene, SceneFrameResources, ScenePipelines, scene_canvas_size};
use crate::texture::{LayerTexture, TextureData};
use celesta_composition::ResolvedAsset;
use celesta_plugin::{
    ContentFrame, ContentModel, ContentProvider, MeshBlend, MeshModel, ModelId, Notice,
    NoticeLevel, TextureSource,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::PathBuf;
use wgpu::util::DeviceExt;

pub(crate) const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// What a `Plugin` layer is drawn from.
pub(crate) struct PluginContent<'a> {
    pub(crate) plugin: &'a str,
    pub(crate) assets: &'a BTreeMap<String, ResolvedAsset>,
    pub(crate) time: f64,
    pub(crate) data: &'a Value,
}

/// Something the plugin of a `Plugin` layer wants the user to know about the
/// model it opened for the layer, such as the model's terms of use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginNotice {
    pub layer: String,
    pub plugin: String,
    pub level: NoticeLevel,
    pub message: String,
}

impl fmt::Display for PluginNotice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.level == NoticeLevel::Info {
            formatter.write_str("note: ")?;
        }
        write!(
            formatter,
            "{} (layer {}): {}",
            self.plugin, self.layer, self.message
        )
    }
}

/// The provider plugin content comes from, the models it opened, and the
/// pipelines that draw them (created by the first `Plugin` layer).
#[derive(Default)]
pub(crate) struct MeshState {
    pub(crate) provider: Option<Box<dyn ContentProvider>>,
    pipelines: Option<Pipelines>,
    /// What draws scenes, created by the first scene opened.
    pub(crate) scene_pipelines: Option<ScenePipelines>,
    /// The canvases and buffers scene frames draw with, kept between
    /// frames.
    pub(crate) scene_frames: SceneFrameResources,
    /// By plugin and asset paths.
    models: HashMap<(String, BTreeMap<String, PathBuf>), OpenModel>,
}

impl MeshState {
    /// Installs `provider`. The models the previous provider opened are its
    /// own, by ids `provider` never gave: they are closed and forgotten, so
    /// `provider` opens its own.
    pub(crate) fn replace_provider(&mut self, provider: Box<dyn ContentProvider>) {
        if let Some(previous) = self.provider.as_mut() {
            for (_, open) in self.models.drain() {
                previous.close(open.loaded.id());
            }
        }
        self.models.clear();
        self.provider = Some(provider);
    }
}

/// A model the provider opened, and what it said about the model then.
struct OpenModel {
    loaded: LoadedModel,
    notices: Vec<Notice>,
}

/// A model opened as 2D meshes or as a 3D scene.
enum LoadedModel {
    Meshes(LoadedMeshes),
    Scene(LoadedScene),
}

impl LoadedModel {
    fn id(&self) -> ModelId {
        match self {
            Self::Meshes(meshes) => meshes.id,
            Self::Scene(scene) => scene.id,
        }
    }

    fn canvas(&self) -> [f32; 2] {
        match self {
            Self::Meshes(meshes) => meshes.model.canvas,
            Self::Scene(scene) => scene.model.canvas,
        }
    }
}

struct LoadedMeshes {
    id: ModelId,
    model: MeshModel,
    /// Each model texture, bound as group 0.
    textures: Vec<wgpu::BindGroup>,
}

struct Pipelines {
    texture_layout: wgpu::BindGroupLayout,
    mask_layout: wgpu::BindGroupLayout,
    meshes_layout: wgpu::BindGroupLayout,
    /// By blend and whether back faces are culled.
    draw: HashMap<(MeshBlend, bool), wgpu::RenderPipeline>,
    /// By whether back faces are culled.
    mask: [wgpu::RenderPipeline; 2],
    sampler: wgpu::Sampler,
    /// Bound as the mask of meshes without one.
    no_mask: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    mesh: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshUniform {
    multiply: [f32; 4],
    screen: [f32; 4],
    opacity_mask: [f32; 4],
}

impl Pipelines {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Celesta mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta mesh texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta mesh mask layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let meshes_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Celesta mesh uniforms layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta mesh pipeline layout"),
            bind_group_layouts: &[
                Some(&texture_layout),
                Some(&mask_layout),
                Some(&meshes_layout),
            ],
            immediate_size: 0,
        });
        let vertex_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32],
        })];
        let pipeline = |label: &str, entry: &str, format, blend: wgpu::BlendState, cull: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &vertex_buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: cull.then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        // Blend states for premultiplied color. Additive and multiplicative
        // blends leave the canvas's alpha as it is.
        let component = |src_factor, dst_factor| wgpu::BlendComponent {
            src_factor,
            dst_factor,
            operation: wgpu::BlendOperation::Add,
        };
        use wgpu::BlendFactor::{Dst, One, OneMinusSrc, OneMinusSrcAlpha, Zero};
        let keep_alpha = component(Zero, One);
        let blends = [
            (
                MeshBlend::Normal,
                wgpu::BlendState {
                    color: component(One, OneMinusSrcAlpha),
                    alpha: component(One, OneMinusSrcAlpha),
                },
            ),
            (
                MeshBlend::Additive,
                wgpu::BlendState {
                    color: component(One, One),
                    alpha: keep_alpha,
                },
            ),
            (
                MeshBlend::Multiplicative,
                wgpu::BlendState {
                    color: component(Dst, OneMinusSrcAlpha),
                    alpha: keep_alpha,
                },
            ),
        ];
        let mut draw = HashMap::new();
        for (blend, state) in blends {
            for cull in [false, true] {
                draw.insert(
                    (blend, cull),
                    pipeline("Celesta mesh draw", "fs_main", OUTPUT_FORMAT, state, cull),
                );
            }
        }
        let mask_blend = wgpu::BlendState {
            color: component(Zero, OneMinusSrc),
            alpha: component(Zero, OneMinusSrcAlpha),
        };
        let mask = [false, true].map(|cull| {
            pipeline(
                "Celesta mesh mask",
                "fs_mask",
                MASK_FORMAT,
                mask_blend,
                cull,
            )
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Celesta mesh sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        // Fully white: `1 - mask` covers nothing, but unmasked meshes never read it.
        let blank = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Celesta mesh no mask"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: MASK_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255],
        );
        let no_mask = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta mesh no mask"),
            layout: &mask_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &blank.create_view(&Default::default()),
                ),
            }],
        });
        Self {
            texture_layout,
            mask_layout,
            meshes_layout,
            draw,
            mask,
            sampler,
            no_mask,
        }
    }
}

/// A straight-alpha RGBA image premultiplied, with every mip level down to
/// 1x1, each the box average of the one above.
fn premultiplied_mips(width: u32, height: u32, pixels: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut level: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|texel| {
            let alpha = u32::from(texel[3]);
            let scale = |channel: u8| ((u32::from(channel) * alpha + 127) / 255) as u8;
            [scale(texel[0]), scale(texel[1]), scale(texel[2]), texel[3]]
        })
        .collect();
    let (mut width, mut height) = (width, height);
    let mut levels = Vec::new();
    loop {
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let done = width == 1 && height == 1;
        if done {
            levels.push((width, height, level));
            break;
        }
        let mut next = vec![0_u8; (next_width * next_height * 4) as usize];
        for y in 0..next_height {
            for x in 0..next_width {
                for channel in 0..4 {
                    let mut sum = 0_u32;
                    let mut count = 0_u32;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = (x * 2 + dx).min(width - 1);
                            let sy = (y * 2 + dy).min(height - 1);
                            sum += u32::from(level[((sy * width + sx) * 4 + channel) as usize]);
                            count += 1;
                        }
                    }
                    next[((y * next_width + x) * 4 + channel) as usize] =
                        ((sum + count / 2) / count) as u8;
                }
            }
        }
        levels.push((width, height, level));
        level = next;
        width = next_width;
        height = next_height;
    }
    levels
}

/// Why `model` cannot be drawn, if it refers to a texture or mesh it lacks.
fn invalid_model(model: &MeshModel) -> Option<String> {
    let meshes = model.meshes.len();
    for (index, mesh) in model.meshes.iter().enumerate() {
        if mesh.texture >= model.textures.len() {
            return Some(format!(
                "mesh {index} uses texture {}, of {}",
                mesh.texture,
                model.textures.len()
            ));
        }
        if mesh.indices.len() % 3 != 0
            || mesh
                .indices
                .iter()
                .any(|vertex| *vertex as usize >= mesh.uvs.len())
        {
            return Some(format!(
                "mesh {index} has triangles outside its {} vertices",
                mesh.uvs.len()
            ));
        }
        if let Some(mask) = &mesh.mask
            && let Some(by) = mask.meshes.iter().find(|by| **by >= meshes)
        {
            return Some(format!("mesh {index} is masked by mesh {by}, of {meshes}"));
        }
    }
    if !(model.canvas[0] > 0.0 && model.canvas[1] > 0.0) {
        return Some(format!("the canvas is {:?}", model.canvas));
    }
    None
}

/// A model texture's pixels, read from its file or taken as sent.
pub(crate) fn texture_image(
    source: &TextureSource,
    layer: &str,
) -> Result<image::RgbaImage, GpuRenderError> {
    Ok(match source {
        TextureSource::Path(path) => image::open(path)
            .map_err(|source| GpuRenderError::ImageDecode {
                asset: path.display().to_string(),
                source,
            })?
            .to_rgba8(),
        TextureSource::Rgba {
            width,
            height,
            pixels,
        } => image::RgbaImage::from_raw(*width, *height, pixels.as_ref().clone()).ok_or_else(
            || GpuRenderError::InvalidPluginContent {
                layer: layer.to_owned(),
                message: format!("a {width}x{height} texture of {} bytes", pixels.len()),
            },
        )?,
    })
}

impl LoadedMeshes {
    fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &Pipelines,
        id: ModelId,
        model: MeshModel,
        layer: &str,
    ) -> Result<Self, GpuRenderError> {
        if let Some(message) = invalid_model(&model) {
            return Err(GpuRenderError::InvalidPluginContent {
                layer: layer.to_owned(),
                message,
            });
        }
        let mut textures = Vec::new();
        for source in &model.textures {
            let image = texture_image(source, layer)?;
            let levels = premultiplied_mips(image.width(), image.height(), image.as_raw());
            let data: Vec<u8> = levels
                .iter()
                .flat_map(|(_, _, pixels)| pixels.iter().copied())
                .collect();
            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("Celesta mesh model texture"),
                    size: wgpu::Extent3d {
                        width: image.width(),
                        height: image.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: levels.len() as u32,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &data,
            );
            textures.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Celesta mesh model texture"),
                layout: &pipelines.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipelines.sampler),
                    },
                ],
            }));
        }
        Ok(Self {
            id,
            model,
            textures,
        })
    }
}

impl GpuRenderer {
    /// The model's canvas, drawn at `density` texels per layer unit, and the
    /// canvas's size in layer units.
    pub(crate) fn plugin_texture(
        &mut self,
        layer: &str,
        content: &PluginContent<'_>,
        density: f64,
    ) -> Result<(LayerTexture, [f32; 2]), GpuRenderError> {
        let mut paths = BTreeMap::new();
        for (name, asset) in content.assets {
            paths.insert(name.clone(), self.local_asset_path(asset)?);
        }
        let key = (content.plugin.to_owned(), paths);
        let plugin_error = |source| GpuRenderError::Plugin {
            layer: layer.to_owned(),
            source,
        };
        if self.meshes.pipelines.is_none() {
            self.meshes.pipelines = Some(Pipelines::new(&self.device, &self.queue));
        }
        if !self.meshes.models.contains_key(&key) {
            let provider = self
                .meshes
                .provider
                .as_mut()
                .ok_or_else(|| GpuRenderError::MissingContentProvider(layer.to_owned()))?;
            let opened = provider
                .open(content.plugin, &key.1)
                .map_err(plugin_error)?;
            let id = opened.model;
            let notices = opened.notices;
            let loaded = match opened.content {
                ContentModel::Meshes(model) => LoadedMeshes::load(
                    &self.device,
                    &self.queue,
                    pipelines_ref(&self.meshes),
                    id,
                    model,
                    layer,
                )
                .map(LoadedModel::Meshes),
                ContentModel::Scene(model) => {
                    // Loading makes the pipelines the model's materials use.
                    let pipelines = self
                        .meshes
                        .scene_pipelines
                        .get_or_insert_with(|| ScenePipelines::new(&self.device, &self.queue));
                    LoadedScene::load(
                        &self.device,
                        &self.queue,
                        pipelines,
                        self.max_texture_dimension,
                        id,
                        model,
                        layer,
                    )
                    .map(LoadedModel::Scene)
                }
            };
            let loaded = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    // Nothing draws a model that failed to load.
                    if let Some(provider) = self.meshes.provider.as_mut() {
                        provider.close(id);
                    }
                    return Err(error);
                }
            };
            self.meshes
                .models
                .insert(key.clone(), OpenModel { loaded, notices });
        }
        let open = &self.meshes.models[&key];
        for notice in &open.notices {
            if !self.plugin_notices.iter().any(|reported| {
                reported.plugin == content.plugin && reported.message == notice.message
            }) {
                self.plugin_notices.push(PluginNotice {
                    layer: layer.to_owned(),
                    plugin: content.plugin.to_owned(),
                    level: notice.level,
                    message: notice.message.clone(),
                });
            }
        }
        let canvas = open.loaded.canvas();
        let (width, height) = match &open.loaded {
            // A scene's canvas costs many times a mesh canvas's texels.
            LoadedModel::Scene(_) => scene_canvas_size(canvas, density, self.max_texture_dimension),
            LoadedModel::Meshes(_) => {
                let largest = canvas[0].max(canvas[1]).max(1.0);
                let scale = density.min(f64::from(self.max_texture_dimension) / f64::from(largest));
                let width = ((f64::from(canvas[0]) * scale).ceil() as u32).max(1);
                let height = ((f64::from(canvas[1]) * scale).ceil() as u32).max(1);
                (width, height)
            }
        };
        if self.measuring_plane {
            return Ok((self.stand_in_texture(width, height), canvas));
        }
        if matches!(self.meshes.models[&key].loaded, LoadedModel::Scene(_)) {
            // Before the plugin's frame and its checks, any of which can
            // fail: one bad frame does not cost the next its canvas.
            self.meshes.scene_frames.mark_used((width, height));
        }

        let id = self.meshes.models[&key].loaded.id();
        let provider = self
            .meshes
            .provider
            .as_mut()
            .ok_or_else(|| GpuRenderError::MissingContentProvider(layer.to_owned()))?;
        let frame = provider
            .frame(id, content.time, content.data)
            .map_err(plugin_error)?;
        let mismatch = |message: &str| GpuRenderError::InvalidPluginContent {
            layer: layer.to_owned(),
            message: message.to_owned(),
        };
        let (frame, loaded) = match (frame, &self.meshes.models[&key].loaded) {
            (ContentFrame::Meshes(frame), LoadedModel::Meshes(loaded)) => (frame, loaded),
            (ContentFrame::Scene(frame), LoadedModel::Scene(scene)) => {
                let mut resources = std::mem::take(&mut self.meshes.scene_frames);
                let texture =
                    self.scene_texture(layer, scene, &frame, width, height, &mut resources);
                self.meshes.scene_frames = resources;
                return Ok((texture?, canvas));
            }
            (ContentFrame::Scene(_), LoadedModel::Meshes(_)) => {
                return Err(mismatch("the plugin sent a scene frame for meshes"));
            }
            (ContentFrame::Meshes(_), LoadedModel::Scene(_)) => {
                return Err(mismatch("the plugin sent a meshes frame for a scene"));
            }
        };
        let pipelines = pipelines_ref(&self.meshes);
        let infos = &loaded.model.meshes;
        for state in &frame.meshes {
            let Some(info) = infos.get(state.index) else {
                return Err(GpuRenderError::InvalidPluginContent {
                    layer: layer.to_owned(),
                    message: format!("frame lists mesh {}, of {}", state.index, infos.len()),
                });
            };
            if state.positions.len() != info.uvs.len() {
                return Err(GpuRenderError::InvalidPluginContent {
                    layer: layer.to_owned(),
                    message: format!(
                        "mesh {} has {} positions for {} vertices",
                        state.index,
                        state.positions.len(),
                        info.uvs.len()
                    ),
                });
            }
        }

        // Every mesh's vertices and triangles, wherever it draws.
        let mut vertices = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut ranges = vec![0..0; infos.len()];
        let mut uniforms = vec![<MeshUniform as bytemuck::Zeroable>::zeroed(); infos.len()];
        for mesh in &frame.meshes {
            let info = &infos[mesh.index];
            let base = vertices.len() as u32;
            for (position, uv) in mesh.positions.iter().zip(&info.uvs) {
                vertices.push(Vertex {
                    position: [
                        position[0] / canvas[0] * 2.0 - 1.0,
                        1.0 - position[1] / canvas[1] * 2.0,
                    ],
                    uv: *uv,
                    mesh: mesh.index as u32,
                });
            }
            let start = indices.len() as u32;
            indices.extend(info.indices.iter().map(|index| base + *index));
            ranges[mesh.index] = start..indices.len() as u32;
            let mask = match &info.mask {
                None => 0.0,
                Some(mask) if mask.inverted => 2.0,
                Some(_) => 1.0,
            };
            uniforms[mesh.index] = MeshUniform {
                multiply: [mesh.multiply[0], mesh.multiply[1], mesh.multiply[2], 1.0],
                screen: [mesh.screen[0], mesh.screen[1], mesh.screen[2], 1.0],
                opacity_mask: [mesh.opacity, mask, 0.0, 0.0],
            };
        }
        let output = self.plugin_canvas(width, height);
        let output_view = output.create_view(&Default::default());
        let drawn: Vec<_> = frame
            .meshes
            .iter()
            .filter(|mesh| mesh.visible && mesh.opacity > 0.0 && !ranges[mesh.index].is_empty())
            .collect();
        if !indices.is_empty() {
            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Celesta mesh vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Celesta mesh indices"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            let uniform_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Celesta mesh uniforms"),
                        contents: bytemuck::cast_slice(&uniforms),
                        usage: wgpu::BufferUsages::STORAGE,
                    });
            let meshes_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Celesta mesh uniforms"),
                layout: &pipelines.meshes_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            // One mask canvas per distinct set of mask meshes.
            let mut mask_sets: Vec<&[usize]> = Vec::new();
            let mut mask_of = HashMap::new();
            for mesh in &drawn {
                let Some(mask) = &infos[mesh.index].mask else {
                    continue;
                };
                let masks = mask.meshes.as_slice();
                let set = match mask_sets.iter().position(|set| *set == masks) {
                    Some(set) => set,
                    None => {
                        mask_sets.push(masks);
                        mask_sets.len() - 1
                    }
                };
                mask_of.insert(mesh.index, set);
            }
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Celesta mesh layer"),
                });
            let mut mask_groups = Vec::new();
            for set in &mask_sets {
                let mask = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Celesta mesh mask"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: MASK_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = mask.create_view(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Celesta mesh mask"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_bind_group(1, &pipelines.no_mask, &[]);
                    pass.set_bind_group(2, &meshes_group, &[]);
                    // A mask's own visibility and opacity do not matter:
                    // a hidden mesh still clips the meshes it masks.
                    for &mask_index in *set {
                        let range = ranges[mask_index].clone();
                        if range.is_empty() {
                            continue;
                        }
                        let info = &infos[mask_index];
                        pass.set_pipeline(&pipelines.mask[usize::from(!info.double_sided)]);
                        pass.set_bind_group(0, &loaded.textures[info.texture], &[]);
                        pass.draw_indexed(range, 0, 0..1);
                    }
                }
                mask_groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Celesta mesh mask"),
                    layout: &pipelines.mask_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    }],
                }));
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Celesta mesh draw"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_bind_group(2, &meshes_group, &[]);
                for mesh in &drawn {
                    let info = &infos[mesh.index];
                    pass.set_pipeline(&pipelines.draw[&(info.blend, !info.double_sided)]);
                    pass.set_bind_group(0, &loaded.textures[info.texture], &[]);
                    let mask = mask_of
                        .get(&mesh.index)
                        .map_or(&pipelines.no_mask, |set| &mask_groups[*set]);
                    pass.set_bind_group(1, mask, &[]);
                    pass.draw_indexed(ranges[mesh.index].clone(), 0, 0..1);
                }
            }
            self.queue.submit([encoder.finish()]);
        }
        Ok((self.plugin_layer_texture(output, width, height), canvas))
    }

    /// A `width` by `height` canvas plugin content is drawn onto: transparent
    /// until something draws on it.
    pub(crate) fn plugin_canvas(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Celesta mesh canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUTPUT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// A drawn `plugin_canvas` as the layer's texture, premultiplied.
    pub(crate) fn plugin_layer_texture(
        &self,
        output: wgpu::Texture,
        width: u32,
        height: u32,
    ) -> LayerTexture {
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Celesta mesh layer texture"),
            layout: &self.texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &output.create_view(&Default::default()),
                ),
            }],
        });
        LayerTexture::from_data(TextureData {
            _texture: output,
            bind_group,
            width,
            height,
            baseline_anchor: 0.0,
            anchor_origin: (0.0, 0.0),
            anchor_span: (1.0, 1.0),
            text_box: None,
            raster_scale: 1.0,
            premultiplied: true,
        })
    }
}

fn pipelines_ref(state: &MeshState) -> &Pipelines {
    state.pipelines.as_ref().expect("created before drawing")
}
