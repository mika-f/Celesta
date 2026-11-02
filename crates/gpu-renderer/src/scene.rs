//! `LayerContent::Plugin` scene content: a provider's 3D scene, uploaded
//! once when it is opened and checked against each of its frames. A scene
//! draws onto the same kind of canvas as 2D meshes (see `mesh.rs`).

use crate::error::GpuRenderError;
use crate::mesh::texture_image;
use crate::renderer::GpuRenderer;
use crate::texture::LayerTexture;
use celesta_plugin::{
    AlphaMode, Camera, ColorSpace, Filter, MaterialKind, ModelId, Projection, SceneFrame,
    SceneModel, Wrap,
};
use wgpu::util::DeviceExt;

/// A scene model and its copy on the GPU.
pub(crate) struct LoadedScene {
    pub(crate) id: ModelId,
    pub(crate) model: SceneModel,
    /// One per `model.geometries` entry.
    #[expect(dead_code, reason = "read once scenes are drawn")]
    pub(crate) geometries: Vec<GeometryBuffers>,
    /// One per `model.textures` entry.
    #[expect(dead_code, reason = "read once scenes are drawn")]
    pub(crate) textures: Vec<SceneTextureView>,
}

/// A geometry's vertex streams and triangles, laid out to be read as
/// storage buffers (each is also usable as a vertex buffer). Every stream
/// has one element per vertex, in vertex order; an empty one still holds a
/// single zeroed 16-byte element, so that it can be bound.
#[expect(dead_code, reason = "read once scenes are drawn")]
pub(crate) struct GeometryBuffers {
    pub(crate) vertices: u32,
    pub(crate) targets: u32,
    /// `vec4<f32>`: the position, `w` 1.
    pub(crate) positions: wgpu::Buffer,
    /// `vec4<f32>`: the normal, `w` 0.
    pub(crate) normals: wgpu::Buffer,
    /// `vec2<f32>`, `v` counting down from the texture's top row.
    pub(crate) uvs: wgpu::Buffer,
    /// `vec4<f32>`, the bitangent's sign in `w`.
    pub(crate) tangents: Option<wgpu::Buffer>,
    /// `vec4<u32>`, widened from the model's `u16`s; with `weights`
    /// (`vec4<f32>`) when the geometry is skinned.
    pub(crate) joints: Option<wgpu::Buffer>,
    pub(crate) weights: Option<wgpu::Buffer>,
    /// `vec4<f32>` position offsets, `w` 0, target by target: target `t`'s
    /// offset for vertex `v` is element `t × vertices + v`.
    pub(crate) morph_positions: Option<wgpu::Buffer>,
    /// Normal offsets laid out as `morph_positions`, zero for a target that
    /// leaves the normals as they are; absent when every target does.
    pub(crate) morph_normals: Option<wgpu::Buffer>,
    /// `u32`, three per triangle.
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
}

/// A scene texture and the sampler its `Sampler` describes.
#[expect(dead_code, reason = "read once scenes are drawn")]
pub(crate) struct SceneTextureView {
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
}

fn finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Why `model` cannot be drawn: a canvas without area, vertex streams of
/// different lengths, triangles outside their vertices, or a reference to
/// something it lacks. A draw's skin must have every joint its geometry's
/// vertices name; both are fixed, so that is checked here, once, rather
/// than in every frame.
pub(crate) fn invalid_scene(model: &SceneModel) -> Option<String> {
    if !model
        .canvas
        .iter()
        .all(|size| size.is_finite() && *size > 0.0)
    {
        return Some(format!("the canvas is {:?}", model.canvas));
    }
    for (index, geometry) in model.geometries.iter().enumerate() {
        let vertices = geometry.positions.len();
        let streams = [
            ("normals", Some(geometry.normals.len())),
            ("uvs", Some(geometry.uvs.len())),
            ("tangents", geometry.tangents.as_ref().map(Vec::len)),
            ("joints", geometry.joints.as_ref().map(Vec::len)),
            ("weights", geometry.weights.as_ref().map(Vec::len)),
        ];
        for (name, length) in streams {
            if let Some(length) = length
                && length != vertices
            {
                return Some(format!(
                    "geometry {index} has {length} {name} for {vertices} vertices"
                ));
            }
        }
        if geometry.joints.is_some() != geometry.weights.is_some() {
            return Some(format!(
                "geometry {index} needs both joints and weights, or neither"
            ));
        }
        if geometry.indices.len() % 3 != 0
            || geometry
                .indices
                .iter()
                .any(|vertex| *vertex as usize >= vertices)
        {
            return Some(format!(
                "geometry {index} has triangles outside its {vertices} vertices"
            ));
        }
        for (target, offsets) in geometry.targets.iter().enumerate() {
            if offsets.positions.len() != vertices
                || !(offsets.normals.is_empty() || offsets.normals.len() == vertices)
            {
                return Some(format!(
                    "geometry {index}'s target {target} has {} positions and {} normals for {vertices} vertices",
                    offsets.positions.len(),
                    offsets.normals.len()
                ));
            }
        }
    }
    for (index, material) in model.materials.iter().enumerate() {
        if let AlphaMode::Mask { cutoff } = material.alpha_mode
            && !cutoff.is_finite()
        {
            return Some(format!("material {index}'s alphaCutoff is {cutoff}"));
        }
        let MaterialKind::Unlit(unlit) = &material.kind;
        if let Some(texture) = &unlit.base_color_texture
            && texture.texture >= model.textures.len()
        {
            return Some(format!(
                "material {index} uses texture {}, of {}",
                texture.texture,
                model.textures.len()
            ));
        }
    }
    // The highest joint each geometry's vertices name.
    let highest_joints: Vec<Option<u16>> = model
        .geometries
        .iter()
        .map(|geometry| {
            geometry
                .joints
                .as_ref()
                .and_then(|joints| joints.as_flattened().iter().max().copied())
        })
        .collect();
    for (index, draw) in model.draws.iter().enumerate() {
        let Some(geometry) = model.geometries.get(draw.geometry) else {
            return Some(format!(
                "draw {index} uses geometry {}, of {}",
                draw.geometry,
                model.geometries.len()
            ));
        };
        if draw.material >= model.materials.len() {
            return Some(format!(
                "draw {index} uses material {}, of {}",
                draw.material,
                model.materials.len()
            ));
        }
        match (draw.skin, geometry.joints.is_some()) {
            (None, false) => {}
            (None, true) => {
                return Some(format!(
                    "draw {index} has no skin for skinned geometry {}",
                    draw.geometry
                ));
            }
            (Some(skin), false) => {
                return Some(format!(
                    "draw {index} has skin {skin}, but geometry {} is not skinned",
                    draw.geometry
                ));
            }
            (Some(skin), true) => {
                let Some(joints) = model.skins.get(skin).map(|skin| skin.joints) else {
                    return Some(format!(
                        "draw {index} uses skin {skin}, of {}",
                        model.skins.len()
                    ));
                };
                if let Some(highest) = highest_joints[draw.geometry]
                    && u32::from(highest) >= joints
                {
                    return Some(format!(
                        "draw {index}'s geometry {} uses joint {highest}, of skin {skin}'s {joints}",
                        draw.geometry
                    ));
                }
            }
        }
    }
    None
}

/// Why `camera` cannot see anything: a lens without depth or angle, or a
/// view that cannot be undone.
fn invalid_camera(camera: &Camera) -> Option<String> {
    if !finite(&camera.view) {
        return Some(format!("the camera's view {:?} is not finite", camera.view));
    }
    let (near, far) = match camera.projection {
        Projection::Perspective { yfov, near, far } => {
            if !(yfov > 0.0 && yfov < std::f32::consts::PI) {
                return Some(format!("the camera's yfov is {yfov}, not between 0 and π"));
            }
            (near, far)
        }
        Projection::Orthographic { height, near, far } => {
            if !(height.is_finite() && height > 0.0) {
                return Some(format!("the camera's height is {height}"));
            }
            (near, far)
        }
    };
    if !(near > 0.0 && near < far && far.is_finite()) {
        return Some(format!(
            "the camera's near is {near} and far {far}, not 0 < near < far"
        ));
    }
    // A view is normally rigid, its determinant ±1; one of about zero has
    // no inverse worth the name.
    if determinant(&camera.view).abs() <= f64::EPSILON {
        return Some("the camera's view is not invertible".to_owned());
    }
    None
}

/// The determinant of a 4×4 matrix, by the 2×2 minors of its first two
/// rows and its last two.
fn determinant(m: &[f32; 16]) -> f64 {
    // Column-major: row `r`, column `c` is `m[c * 4 + r]`.
    let a = |r: usize, c: usize| f64::from(m[c * 4 + r]);
    let low = |r: usize, j: usize, k: usize| a(r, j) * a(r + 1, k) - a(r + 1, j) * a(r, k);
    low(0, 0, 1) * low(2, 2, 3) - low(0, 0, 2) * low(2, 1, 3)
        + low(0, 0, 3) * low(2, 1, 2)
        + low(0, 1, 2) * low(2, 0, 3)
        - low(0, 1, 3) * low(2, 0, 2)
        + low(0, 2, 3) * low(2, 0, 1)
}

/// Why `frame` does not fit `model` (which `invalid_scene` passed): a draw
/// it lacks or lists twice, morph weights or skinning matrices that do not
/// match, a number that is not finite, or a camera that sees nothing.
pub(crate) fn invalid_scene_frame(model: &SceneModel, frame: &SceneFrame) -> Option<String> {
    if let Some(message) = invalid_camera(&frame.camera) {
        return Some(message);
    }
    if !frame.time.is_finite() {
        return Some(format!("the frame's time is {}", frame.time));
    }
    if !finite(frame.lights.ambient.as_flattened()) {
        return Some("the ambient light is not finite".to_owned());
    }
    for (index, light) in frame.lights.directional.iter().enumerate() {
        if !(finite(&light.direction) && finite(&light.color) && light.intensity.is_finite()) {
            return Some(format!("directional light {index} is not finite"));
        }
    }
    if frame.skins.len() != model.skins.len() {
        return Some(format!(
            "the frame has {} skins for the model's {}",
            frame.skins.len(),
            model.skins.len()
        ));
    }
    for (index, (matrices, skin)) in frame.skins.iter().zip(&model.skins).enumerate() {
        if matrices.len() != skin.joints as usize {
            return Some(format!(
                "skin {index} has {} matrices for {} joints",
                matrices.len(),
                skin.joints
            ));
        }
        if !matrices.iter().all(|matrix| finite(matrix)) {
            return Some(format!("skin {index} has a matrix that is not finite"));
        }
    }
    let mut listed = vec![false; model.draws.len()];
    for state in &frame.draws {
        let index = state.index;
        let Some(info) = model.draws.get(index) else {
            return Some(format!(
                "the frame lists draw {index}, of {}",
                model.draws.len()
            ));
        };
        if std::mem::replace(&mut listed[index], true) {
            return Some(format!("the frame lists draw {index} twice"));
        }
        let targets = model.geometries[info.geometry].targets.len();
        if state.morph_weights.len() != targets {
            return Some(format!(
                "draw {index} has {} morph weights for {targets} targets",
                state.morph_weights.len()
            ));
        }
        let overrides = &state.overrides;
        let numbers = [
            ("world", Some(&state.world[..])),
            ("morph weights", Some(&state.morph_weights[..])),
            ("center", state.center.as_ref().map(|center| &center[..])),
            (
                "base color override",
                overrides.base_color.as_ref().map(|color| &color[..]),
            ),
            (
                "texture offset override",
                overrides.texture_offset.as_ref().map(|offset| &offset[..]),
            ),
            (
                "texture scale override",
                overrides.texture_scale.as_ref().map(|scale| &scale[..]),
            ),
        ];
        for (name, values) in numbers {
            if let Some(values) = values
                && !finite(values)
            {
                return Some(format!("draw {index}'s {name} is not finite"));
            }
        }
    }
    None
}

/// Straight-alpha RGBA mip levels from the image down to 1x1, each texel
/// the box average of the 2x2 above it. sRGB colors average in linear light;
/// alpha, and linear data, as they are.
fn straight_mips(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    color_space: ColorSpace,
) -> Vec<(u32, u32, Vec<u8>)> {
    let decode: [f32; 256] = std::array::from_fn(|value| {
        let value = value as f32 / 255.0;
        match color_space {
            ColorSpace::Srgb if value <= 0.04045 => value / 12.92,
            ColorSpace::Srgb => ((value + 0.055) / 1.055).powf(2.4),
            ColorSpace::Linear => value,
        }
    });
    let encode = |value: f32| {
        let value = match color_space {
            ColorSpace::Srgb if value <= 0.003_130_8 => value * 12.92,
            ColorSpace::Srgb => 1.055 * value.powf(1.0 / 2.4) - 0.055,
            ColorSpace::Linear => value,
        };
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let mut levels = vec![(width, height, pixels)];
    loop {
        let (width, height, level) = levels.last().expect("starts with the image");
        let (width, height) = (*width, *height);
        if width == 1 && height == 1 {
            return levels;
        }
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let mut next = Vec::with_capacity((next_width * next_height * 4) as usize);
        for y in 0..next_height {
            for x in 0..next_width {
                let mut sum = [0.0_f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(width - 1);
                    let sy = (y * 2 + dy).min(height - 1);
                    let texel = &level[((sy * width + sx) * 4) as usize..][..4];
                    for channel in 0..3 {
                        sum[channel] += decode[usize::from(texel[channel])];
                    }
                    sum[3] += f32::from(texel[3]);
                }
                next.extend([
                    encode(sum[0] / 4.0),
                    encode(sum[1] / 4.0),
                    encode(sum[2] / 4.0),
                    (sum[3] / 4.0).round() as u8,
                ]);
            }
        }
        levels.push((next_width, next_height, next));
    }
}

fn address_mode(wrap: Wrap) -> wgpu::AddressMode {
    match wrap {
        Wrap::Repeat => wgpu::AddressMode::Repeat,
        Wrap::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        Wrap::ClampToEdge => wgpu::AddressMode::ClampToEdge,
    }
}

fn filter_mode(filter: Filter) -> wgpu::FilterMode {
    match filter {
        Filter::Nearest => wgpu::FilterMode::Nearest,
        Filter::Linear => wgpu::FilterMode::Linear,
    }
}

impl LoadedScene {
    /// Checks `model` and uploads its geometries and textures.
    pub(crate) fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        max_texture_dimension: u32,
        id: ModelId,
        model: SceneModel,
        layer: &str,
    ) -> Result<Self, GpuRenderError> {
        let invalid = |message| GpuRenderError::InvalidPluginContent {
            layer: layer.to_owned(),
            message,
        };
        if let Some(message) = invalid_scene(&model) {
            return Err(invalid(message));
        }
        let limits = device.limits();
        let largest_buffer = limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size);
        let mut geometries = Vec::new();
        for (index, geometry) in model.geometries.iter().enumerate() {
            let buffer = |name: &str, contents: &[u8], usage| {
                if contents.len() as u64 > largest_buffer {
                    return Err(invalid(format!(
                        "geometry {index}'s {name} take {} bytes, more than this GPU's {largest_buffer}",
                        contents.len()
                    )));
                }
                Ok(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Celesta scene geometry"),
                        contents: if contents.is_empty() {
                            &[0; 16]
                        } else {
                            contents
                        },
                        usage,
                    }),
                )
            };
            let stream = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX;
            let vec4 = |values: &[[f32; 3]], w: f32| -> Vec<f32> {
                values
                    .iter()
                    .flat_map(|value| [value[0], value[1], value[2], w])
                    .collect()
            };
            let vertices = geometry.positions.len();
            let morph_positions: Vec<f32> = geometry
                .targets
                .iter()
                .flat_map(|target| vec4(&target.positions, 0.0))
                .collect();
            let morph_normals: Option<Vec<f32>> = geometry
                .targets
                .iter()
                .any(|target| !target.normals.is_empty())
                .then(|| {
                    geometry
                        .targets
                        .iter()
                        .flat_map(|target| {
                            if target.normals.is_empty() {
                                vec![0.0; vertices * 4]
                            } else {
                                vec4(&target.normals, 0.0)
                            }
                        })
                        .collect()
                });
            let joints: Option<Vec<u32>> = geometry.joints.as_ref().map(|joints| {
                joints
                    .as_flattened()
                    .iter()
                    .map(|joint| u32::from(*joint))
                    .collect()
            });
            geometries.push(GeometryBuffers {
                vertices: vertices as u32,
                targets: geometry.targets.len() as u32,
                positions: buffer(
                    "positions",
                    bytemuck::cast_slice(&vec4(&geometry.positions, 1.0)),
                    stream,
                )?,
                normals: buffer(
                    "normals",
                    bytemuck::cast_slice(&vec4(&geometry.normals, 0.0)),
                    stream,
                )?,
                uvs: buffer(
                    "uvs",
                    bytemuck::cast_slice(geometry.uvs.as_flattened()),
                    stream,
                )?,
                tangents: geometry
                    .tangents
                    .as_ref()
                    .map(|tangents| {
                        buffer(
                            "tangents",
                            bytemuck::cast_slice(tangents.as_flattened()),
                            stream,
                        )
                    })
                    .transpose()?,
                joints: joints
                    .map(|joints| buffer("joints", bytemuck::cast_slice(&joints), stream))
                    .transpose()?,
                weights: geometry
                    .weights
                    .as_ref()
                    .map(|weights| {
                        buffer(
                            "weights",
                            bytemuck::cast_slice(weights.as_flattened()),
                            stream,
                        )
                    })
                    .transpose()?,
                morph_positions: (!geometry.targets.is_empty())
                    .then(|| {
                        buffer(
                            "morph targets",
                            bytemuck::cast_slice(&morph_positions),
                            stream,
                        )
                    })
                    .transpose()?,
                morph_normals: morph_normals
                    .map(|normals| {
                        buffer(
                            "morph target normals",
                            bytemuck::cast_slice(&normals),
                            stream,
                        )
                    })
                    .transpose()?,
                indices: buffer(
                    "indices",
                    bytemuck::cast_slice(&geometry.indices),
                    wgpu::BufferUsages::INDEX | wgpu::BufferUsages::STORAGE,
                )?,
                index_count: geometry.indices.len() as u32,
            });
        }

        let mut textures = Vec::new();
        for (index, texture) in model.textures.iter().enumerate() {
            let image = texture_image(&texture.source, layer)?;
            let (width, height) = image.dimensions();
            if width == 0 || height == 0 || width.max(height) > max_texture_dimension {
                return Err(invalid(format!(
                    "texture {index} is {width}x{height}, outside 1 to {max_texture_dimension} texels a side"
                )));
            }
            let sampler = &texture.sampler;
            let levels = if sampler.mipmaps {
                straight_mips(width, height, image.into_raw(), texture.color_space)
            } else {
                vec![(width, height, image.into_raw())]
            };
            let data: Vec<u8> = levels
                .iter()
                .flat_map(|(_, _, pixels)| pixels.iter().copied())
                .collect();
            let uploaded = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("Celesta scene texture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: levels.len() as u32,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: match texture.color_space {
                        ColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
                        ColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
                    },
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &data,
            );
            textures.push(SceneTextureView {
                view: uploaded.create_view(&Default::default()),
                sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("Celesta scene sampler"),
                    address_mode_u: address_mode(sampler.wrap_s),
                    address_mode_v: address_mode(sampler.wrap_t),
                    mag_filter: filter_mode(sampler.mag_filter),
                    min_filter: filter_mode(sampler.min_filter),
                    mipmap_filter: if sampler.mipmaps {
                        wgpu::MipmapFilterMode::Linear
                    } else {
                        wgpu::MipmapFilterMode::Nearest
                    },
                    ..Default::default()
                }),
            });
        }
        Ok(Self {
            id,
            model,
            geometries,
            textures,
        })
    }
}

impl GpuRenderer {
    /// `frame` of `scene` on a `width` by `height` canvas, once the frame is
    /// checked against the model. Drawing is not implemented yet: the
    /// canvas stays transparent.
    pub(crate) fn scene_texture(
        &self,
        layer: &str,
        scene: &LoadedScene,
        frame: &SceneFrame,
        width: u32,
        height: u32,
    ) -> Result<LayerTexture, GpuRenderError> {
        if let Some(message) = invalid_scene_frame(&scene.model, frame) {
            return Err(GpuRenderError::InvalidPluginContent {
                layer: layer.to_owned(),
                message,
            });
        }
        let output = self.plugin_canvas(width, height);
        Ok(self.plugin_layer_texture(output, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinant_of_scaled_and_singular_matrices() {
        let mut m = [0.0_f32; 16];
        for (i, scale) in [2.0, 3.0, 4.0, 1.0].into_iter().enumerate() {
            m[i * 4 + i] = scale;
        }
        m[12] = 5.0;
        assert_eq!(determinant(&m), 24.0);
        // Swapping two rows negates it.
        let mut swapped = m;
        for c in 0..4 {
            swapped.swap(c * 4, c * 4 + 1);
        }
        assert_eq!(determinant(&swapped), -24.0);
        m[0] = 0.0;
        assert_eq!(determinant(&m), 0.0);
    }

    #[test]
    fn srgb_mips_average_in_linear_light() {
        // Black and white average to linear 0.5, which sRGB encodes as 188.
        let pixels = [0, 0, 0, 255, 255, 255, 255, 255].to_vec();
        let levels = straight_mips(2, 1, pixels.clone(), ColorSpace::Srgb);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[1], (1, 1, vec![188, 188, 188, 255]));
        let levels = straight_mips(2, 1, pixels, ColorSpace::Linear);
        assert_eq!(levels[1], (1, 1, vec![128, 128, 128, 255]));
        // A uniform texel stays as it is.
        let levels = straight_mips(2, 2, [128, 64, 200, 77].repeat(4), ColorSpace::Srgb);
        assert_eq!(levels[1].2, vec![128, 64, 200, 77]);
    }
}
