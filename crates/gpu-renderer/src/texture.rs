use crate::BYTES_PER_PIXEL;
use crate::error::GpuRenderError;
use crate::layer::PreparedItem;
use crate::renderer::GpuRenderer;
use crate::text::{PendingText, rasterize_text, text_layer};
use celesta_composition::ResolvedAsset;
use celesta_remote::resolve_asset_path;
use celesta_renderer::TextRasterizer;
use celesta_renderer::resample::{Coverage, box_downscale};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

impl GpuRenderer {
    pub(crate) fn local_asset_path(
        &self,
        asset: &ResolvedAsset,
    ) -> Result<PathBuf, GpuRenderError> {
        resolve_asset_path(&self.asset_root, &asset.location).map_err(|source| {
            GpuRenderError::RemoteAsset {
                asset: asset.id.clone(),
                source,
            }
        })
    }

    /// Returns the texture cached under `key`, marking it used by the frame
    /// being prepared, or produces and uploads it on a miss. Cached textures
    /// are reused for many frames, so they get mipmaps when `mipmaps` is set.
    pub(crate) fn cached_texture(
        &mut self,
        key: String,
        mipmaps: bool,
        produce: impl FnOnce(&mut Self) -> Result<DecodedImage, GpuRenderError>,
    ) -> Result<LayerTexture, GpuRenderError> {
        let generation = self.texture_generation;
        if let Some(cached) = self.textures.get_mut(&key) {
            cached.last_used = generation;
            return Ok(cached.texture.clone());
        }
        let image = produce(self)?;
        let texture = self.upload_texture(&image, mipmaps);
        self.textures.insert(
            key,
            CachedTexture {
                texture: texture.clone(),
                last_used: generation,
            },
        );
        Ok(texture)
    }

    /// Caches the textures of the text `prepare_layer` found missing from
    /// the cache, rasterized by `rasterize_texts`, and puts its layers in
    /// place of their `PendingText` items.
    pub(crate) fn place_texts(
        &mut self,
        pending: &[PendingText],
        jobs: &[&PendingText],
        images: Vec<Result<DecodedImage, GpuRenderError>>,
        items: &mut [PreparedItem],
    ) -> Result<(), GpuRenderError> {
        let generation = self.texture_generation;
        for (job, image) in jobs.iter().zip(images) {
            let texture = self.upload_texture(&image?, false);
            self.textures.insert(
                job.key.clone(),
                CachedTexture {
                    texture,
                    last_used: generation,
                },
            );
        }
        for text in pending {
            let texture = self.textures[&text.key].texture.clone();
            let mut layer = text_layer(
                texture,
                text.anchor,
                text.baseline_anchor,
                text.state,
                text.blend_mode,
            );
            layer.shape = text.shape;
            items[text.item] = PreparedItem::Layer(layer);
        }
        Ok(())
    }

    /// A texture `width`x`height` texels in size that holds none: it shares
    /// the placeholder's handles, for content whose size alone counts, as
    /// while a plane is measured. Nothing is uploaded.
    pub(crate) fn stand_in_texture(&self, width: u32, height: u32) -> LayerTexture {
        let placeholder = &self.placeholder_texture;
        LayerTexture(Arc::new(TextureData {
            _texture: placeholder._texture.clone(),
            bind_group: placeholder.bind_group.clone(),
            width,
            height,
            baseline_anchor: 0.0,
            anchor_origin: (0.0, 0.0),
            anchor_span: (1.0, 1.0),
            text_box: None,
            raster_scale: 1.0,
            premultiplied: false,
        }))
    }

    pub(crate) fn upload_texture(&self, image: &DecodedImage, mipmaps: bool) -> LayerTexture {
        upload_texture(
            &self.device,
            &self.queue,
            &self.texture_bind_group_layout,
            image,
            mipmaps,
        )
    }
}

pub(crate) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    image: &DecodedImage,
    mipmaps: bool,
) -> LayerTexture {
    let levels = mip_levels(image, mipmaps);
    let texture = layer_texture(device, layout, image, levels.len() as u32);
    write_levels(queue, &texture._texture, &levels);
    texture
}

/// `image`'s pixels, and with `mipmaps` each smaller level down to 1x1.
type Levels = Vec<(u32, u32, Arc<Vec<u8>>)>;

fn mip_levels(image: &DecodedImage, mipmaps: bool) -> Levels {
    let mut levels = vec![(image.width, image.height, Arc::clone(&image.pixels))];
    if mipmaps {
        loop {
            let (width, height, pixels) = levels.last().expect("level 0 exists");
            if *width == 1 && *height == 1 {
                break;
            }
            let next = downsample(*width, *height, pixels);
            levels.push((next.0, next.1, Arc::new(next.2)));
        }
    }
    levels
}

/// A texture of `image`'s size with `mip_levels` levels, not yet written,
/// as a layer texture carrying the rest of `image`'s description.
fn layer_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    image: &DecodedImage,
    mip_levels: u32,
) -> LayerTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta layer texture"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta layer texture bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    LayerTexture(Arc::new(TextureData {
        _texture: texture,
        bind_group,
        width: image.width,
        height: image.height,
        baseline_anchor: image.baseline_anchor,
        anchor_origin: image.anchor_origin,
        anchor_span: image.anchor_span,
        text_box: image.text_box,
        raster_scale: image.raster_scale,
        premultiplied: false,
    }))
}

/// Writes `levels` into `texture` through `Queue::write_texture`.
fn write_levels(queue: &wgpu::Queue, texture: &wgpu::Texture, levels: &Levels) {
    for (level, (width, height, pixels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * BYTES_PER_PIXEL),
                rows_per_image: Some(*height),
            },
            wgpu::Extent3d {
                width: *width,
                height: *height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Video frames' textures, and the staging buffers their pixels are copied
/// through, kept from render to render. A video brings new pixels every
/// frame, and making a texture and going through `Queue::write_texture`,
/// which makes a staging buffer of its own each time, costs a 4K frame
/// several times the copy itself: both are fresh memory the system maps and
/// clears as it is first touched.
///
/// A frame is written into a kept texture of its size that nothing else
/// holds (`LayerTexture::unshared`), so two video layers of one size in a
/// render each have their own, and a texture something still draws with is
/// never written; its pixels are copied into a kept staging buffer of the
/// same size the GPU has finished reading (mapped again), then copied to the
/// texture. Every texel of the texture is written. A render lets go of what
/// the render before it did not use.
#[derive(Default)]
pub(crate) struct VideoUploads {
    textures: Vec<KeptTexture>,
    staging: Vec<Staging>,
    /// The textures and staging buffers created, ever.
    pub(crate) allocations: u64,
}

struct KeptTexture {
    /// The size and mip level count.
    key: (u32, u32, u32),
    texture: LayerTexture,
    /// Whether the render being prepared, or the one before it, used it.
    used: bool,
}

struct Staging {
    buffer: wgpu::Buffer,
    /// Set once the buffer is mapped for writing: when it is made, and when
    /// the GPU has finished copying out of it.
    mapped: Arc<AtomicBool>,
    used: bool,
}

impl VideoUploads {
    /// Starts a render: what the last render did not use goes.
    pub(crate) fn begin_render(&mut self) {
        self.textures
            .retain_mut(|texture| std::mem::take(&mut texture.used));
        self.staging
            .retain_mut(|staging| std::mem::take(&mut staging.used));
    }

    /// How many textures and staging buffers are kept.
    #[cfg(test)]
    pub(crate) fn kept(&self) -> (usize, usize) {
        (self.textures.len(), self.staging.len())
    }

    /// `image`, with `mipmaps` its smaller levels too, as a layer texture.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        image: &DecodedImage,
        mipmaps: bool,
    ) -> LayerTexture {
        let levels = mip_levels(image, mipmaps);
        let key = (image.width, image.height, levels.len() as u32);
        let texture = match self
            .textures
            .iter_mut()
            .find(|kept| kept.key == key && kept.texture.unshared())
        {
            Some(kept) => {
                kept.used = true;
                kept.texture.clone()
            }
            None => {
                self.allocations += 1;
                let texture = layer_texture(device, layout, image, key.2);
                self.textures.push(KeptTexture {
                    key,
                    texture: texture.clone(),
                    used: true,
                });
                texture
            }
        };

        // Each level at an offset and with rows padded as a copy from a
        // buffer to a texture needs.
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let mut offsets = Vec::with_capacity(levels.len());
        let mut size = 0u64;
        for (width, height, _) in &levels {
            size = wgpu::util::align_to(size, u64::from(alignment));
            offsets.push(size);
            let row = wgpu::util::align_to(width * BYTES_PER_PIXEL, alignment);
            size += u64::from(row) * u64::from(*height);
        }
        let index = match self.staging.iter().position(|staging| {
            staging.buffer.size() == size && staging.mapped.load(Ordering::Acquire)
        }) {
            Some(index) => index,
            None => {
                self.allocations += 1;
                self.staging.push(Staging {
                    buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Celesta video staging buffer"),
                        size,
                        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: true,
                    }),
                    mapped: Arc::new(AtomicBool::new(true)),
                    used: false,
                });
                self.staging.len() - 1
            }
        };
        let staging = &mut self.staging[index];
        staging.used = true;
        let Ok(mut mapped) = staging.buffer.get_mapped_range_mut(..) else {
            // Not mapped after all: this frame goes the plain way, and the
            // buffer is not offered again.
            self.staging.swap_remove(index);
            write_levels(queue, &texture._texture, &levels);
            return texture;
        };
        for ((width, _, pixels), offset) in levels.iter().zip(&offsets) {
            let offset = *offset as usize;
            let row = (width * BYTES_PER_PIXEL) as usize;
            let padded = wgpu::util::align_to(width * BYTES_PER_PIXEL, alignment) as usize;
            if row == padded {
                mapped
                    .slice(offset..offset + pixels.len())
                    .copy_from_slice(pixels);
            } else {
                for (index, from) in pixels.chunks_exact(row).enumerate() {
                    let at = offset + index * padded;
                    mapped.slice(at..at + row).copy_from_slice(from);
                }
            }
        }
        drop(mapped);
        staging.buffer.unmap();
        staging.mapped.store(false, Ordering::Release);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Celesta video upload"),
        });
        for (level, ((width, height, _), offset)) in levels.iter().zip(&offsets).enumerate() {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: *offset,
                        bytes_per_row: Some(wgpu::util::align_to(
                            width * BYTES_PER_PIXEL,
                            alignment,
                        )),
                        rows_per_image: Some(*height),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture._texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: *width,
                    height: *height,
                    depth_or_array_layers: 1,
                },
            );
        }
        // Submitted before the draws that sample the texture, which are
        // submitted after this layer is prepared.
        queue.submit([encoder.finish()]);
        let mapped = Arc::clone(&staging.mapped);
        staging
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Write, move |result| {
                if result.is_ok() {
                    mapped.store(true, Ordering::Release);
                }
            });
        texture
    }
}

/// The next mip level of straight-alpha RGBA pixels: each texel averages the
/// 2x2 block above it (the last row or column of an odd size folds into its
/// neighbour), weighting color by alpha so transparent texels do not darken
/// the edges of what they surround. Sizes halve rounding down, as wgpu
/// expects of mip levels.
pub(crate) fn downsample(width: u32, height: u32, pixels: &[u8]) -> (u32, u32, Vec<u8>) {
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    let next = box_downscale(
        width,
        height,
        pixels,
        next_width,
        next_height,
        Coverage::Alpha,
    );
    (next_width, next_height, next)
}

/// Prefix of every cached text texture's key, so a newly loaded font can drop
/// just those.
pub(crate) const TEXT_TEXTURE_PREFIX: &str = "text\0";

/// Most forks of the text rasterizer kept for parallel text. Each holds its
/// own font database and glyph cache, so more cores should not mean more of
/// them; a frame's slowest text bounds the time anyway.
pub(crate) const MAX_TEXT_WORKERS: usize = 8;

pub(crate) fn psd_key(
    asset: &ResolvedAsset,
    visible_layers: &[String],
    enabled_layers: &[String],
    disabled_layers: &[String],
) -> String {
    format!(
        "psd\0{}\0{}\0{}\0{}",
        asset.id,
        visible_layers.join("\0"),
        enabled_layers.join("\0"),
        disabled_layers.join("\0")
    )
}

#[derive(Clone)]
pub(crate) struct DecodedImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// `Arc<Vec<u8>>` rather than `Arc<[u8]>`: the latter cannot take ownership
    /// of an existing `Vec` and copies it instead, which costs a full 8MB
    /// memcpy for every 1080p video frame the preview decodes — several
    /// milliseconds per frame for nothing, since the buffer is already owned.
    pub(crate) pixels: Arc<Vec<u8>>,
    /// Normalized anchor `y` of the first text baseline; 0 for non-text images.
    pub(crate) baseline_anchor: f64,
    /// Maps a layer anchor onto the image: `origin + anchor * span`, both
    /// normalized. Text can be larger than the box its anchor refers to (a
    /// stroke reaches past it); everything else is `(0, 0)` and `(1, 1)`.
    pub(crate) anchor_origin: (f64, f64),
    pub(crate) anchor_span: (f64, f64),
    /// A text's layout box (see `HitRegion::text_box`) as `[left, top,
    /// right, bottom]`, in layout units from the image's top-left: kept with
    /// the image, as a cached text is not measured again. Its width is the
    /// layout's own, where the anchor box's is rounded up to whole texels.
    /// `None` for non-text images.
    pub(crate) text_box: Option<[f64; 4]>,
    /// Texels per layer unit: the scale text was rasterized at, else 1.
    pub(crate) raster_scale: f32,
}

impl DecodedImage {
    pub(crate) fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, GpuRenderError> {
        Self::shared(width, height, Arc::new(pixels))
    }

    pub(crate) fn shared(
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    ) -> Result<Self, GpuRenderError> {
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(u64::from(BYTES_PER_PIXEL)))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err(GpuRenderError::InvalidImageData {
                width,
                height,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
            baseline_anchor: 0.0,
            anchor_origin: (0.0, 0.0),
            anchor_span: (1.0, 1.0),
            text_box: None,
            raster_scale: 1.0,
        })
    }
}

/// An uploaded layer texture. Cloning it shares the `wgpu` handles, not the
/// pixels, so a cached texture can back draws in several in-flight frames;
/// it is one pointer, so a prepared layer that holds it stays small.
#[derive(Clone)]
pub(crate) struct LayerTexture(Arc<TextureData>);

impl LayerTexture {
    pub(crate) fn from_data(data: TextureData) -> Self {
        Self(Arc::new(data))
    }

    /// Whether draws with `self` and `other` sample the same texture.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.bind_group == other.bind_group
    }

    /// Whether this is the texture's only handle: no layer of the frame
    /// being prepared, nor anything else, draws with it.
    pub(crate) fn unshared(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

impl std::ops::Deref for LayerTexture {
    type Target = TextureData;

    fn deref(&self) -> &TextureData {
        &self.0
    }
}

pub(crate) struct TextureData {
    pub(crate) _texture: wgpu::Texture,
    /// The texture, bound as group 0.
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// See `DecodedImage::baseline_anchor`.
    pub(crate) baseline_anchor: f64,
    /// See `DecodedImage::anchor_origin`.
    pub(crate) anchor_origin: (f64, f64),
    pub(crate) anchor_span: (f64, f64),
    /// See `DecodedImage::text_box`.
    pub(crate) text_box: Option<[f64; 4]>,
    /// See `DecodedImage::raster_scale`.
    pub(crate) raster_scale: f32,
    /// Whether the texels hold premultiplied alpha (a plugin layer's mesh
    /// canvas) rather than straight alpha.
    pub(crate) premultiplied: bool,
}

pub(crate) struct CachedTexture {
    pub(crate) texture: LayerTexture,
    pub(crate) last_used: u64,
}

#[derive(Clone)]
pub(crate) struct CanvasTexture {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    /// Samples the canvas as a layer texture (group 0).
    pub(crate) bind_group: wgpu::BindGroup,
}

/// The format layers composite in: half floats where the adapter can render
/// and blend them, so what overlapping layers, gradients, and blurs mix is
/// kept finer than 8-bit code values until the frame is quantized once, with
/// a dither, for readback (see `Quantizer`). An 11-bit significand holds a
/// code value to within 1/16 of a code (1/8 where a driver truncates).
/// Without half floats, 8-bit canvases, which band as before.
pub(crate) fn canvas_format(adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
    let features = adapter.get_texture_format_features(FORMAT);
    let usages = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    let flags =
        wgpu::TextureFormatFeatureFlags::BLENDABLE | wgpu::TextureFormatFeatureFlags::FILTERABLE;
    if features.allowed_usages.contains(usages) && features.flags.contains(flags) {
        FORMAT
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

pub(crate) fn canvas_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> CanvasTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta canvas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        // Copied from when a blended draw takes its backdrop.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta canvas bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    CanvasTexture {
        texture,
        view,
        bind_group,
    }
}

/// The texts of `pending` to rasterize: the first of each key.
pub(crate) fn text_jobs(pending: &[PendingText]) -> Vec<&PendingText> {
    let mut seen = HashSet::new();
    pending
        .iter()
        .filter(|text| seen.insert(text.key.as_str()))
        .collect()
}

/// Rasterizes `jobs`, in order. A frame whose labels or scale change each
/// frame misses on dozens of texts, and each rasterization is independent,
/// so they spread over rayon's threads, each with its own fork of
/// `rasterizer` from `workers`.
pub(crate) fn rasterize_texts(
    rasterizer: &mut TextRasterizer,
    workers: &mut Vec<TextRasterizer>,
    jobs: &[&PendingText],
    limit: u32,
) -> Vec<Result<DecodedImage, GpuRenderError>> {
    if jobs.len() <= 1 {
        return jobs
            .iter()
            .map(|job| rasterize_text(rasterizer, job, limit))
            .collect();
    }
    let count = jobs
        .len()
        .min(rayon::current_num_threads())
        .min(MAX_TEXT_WORKERS);
    while workers.len() < count {
        workers.push(rasterizer.fork());
    }
    let chunk = jobs.len().div_ceil(count);
    // One result list per chunk, in chunk order, so the flattened images
    // line up with `jobs`.
    let chunks: Vec<Vec<_>> = workers
        .par_iter_mut()
        .zip(jobs.par_chunks(chunk))
        .map(|(rasterizer, jobs)| {
            jobs.iter()
                .map(|job| rasterize_text(rasterizer, job, limit))
                .collect()
        })
        .collect();
    chunks.into_iter().flatten().collect()
}
