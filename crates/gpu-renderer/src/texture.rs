use crate::BYTES_PER_PIXEL;
use crate::error::GpuRenderError;
use crate::layer::PreparedItem;
use crate::renderer::GpuRenderer;
use crate::text::{PendingText, rasterize_text, text_layer};
use celesta_composition::ResolvedAsset;
use celesta_remote::resolve_asset_path;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

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

    /// Rasterizes the text `prepare_layer` found missing from the cache and
    /// puts its layers in place of their `PendingText` items. A frame whose
    /// labels or scale change each frame misses on dozens of texts, and each
    /// rasterization is independent, so they spread over rayon's threads,
    /// each with its own fork of the text rasterizer.
    pub(crate) fn resolve_pending_texts(
        &mut self,
        items: &mut [PreparedItem],
    ) -> Result<(), GpuRenderError> {
        let pending = std::mem::take(&mut self.pending_texts);
        if pending.is_empty() {
            return Ok(());
        }
        let mut seen = HashSet::new();
        let jobs: Vec<&PendingText> = pending
            .iter()
            .filter(|text| seen.insert(text.key.as_str()))
            .collect();
        let limit = self.max_texture_dimension;
        let images: Vec<_> = if jobs.len() == 1 {
            vec![rasterize_text(&mut self.text_rasterizer, jobs[0], limit)]
        } else {
            let workers = jobs
                .len()
                .min(rayon::current_num_threads())
                .min(MAX_TEXT_WORKERS);
            while self.text_workers.len() < workers {
                self.text_workers.push(self.text_rasterizer.fork());
            }
            let chunk = jobs.len().div_ceil(workers);
            // One result list per chunk, in chunk order, so the flattened
            // images line up with `jobs`.
            let chunks: Vec<Vec<_>> = self
                .text_workers
                .par_iter_mut()
                .zip(jobs.par_chunks(chunk))
                .map(|(rasterizer, jobs)| {
                    jobs.iter()
                        .map(|job| rasterize_text(rasterizer, job, limit))
                        .collect()
                })
                .collect();
            chunks.into_iter().flatten().collect()
        };
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
            items[text.item] = PreparedItem::Layer(text_layer(
                texture,
                text.anchor,
                text.baseline_anchor,
                text.state,
                text.blend_mode,
            ));
        }
        Ok(())
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
    let mut levels = vec![(image.width, image.height, Arc::clone(&image.pixels))];
    while mipmaps {
        let (width, height, pixels) = levels.last().expect("level 0 exists");
        if *width == 1 && *height == 1 {
            break;
        }
        let next = downsample(*width, *height, pixels);
        levels.push((next.0, next.1, Arc::new(next.2)));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta layer texture"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (width, height, pixels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
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
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta layer texture bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    LayerTexture {
        _texture: texture,
        bind_group,
        width: image.width,
        height: image.height,
        baseline_anchor: image.baseline_anchor,
        anchor_origin: image.anchor_origin,
        anchor_span: image.anchor_span,
        raster_scale: image.raster_scale,
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
    let mut next = vec![0; (next_width * next_height * BYTES_PER_PIXEL) as usize];
    let span = |index: u32, size: u32, next_size: u32| {
        let start = index * size / next_size;
        let end = ((index + 1) * size / next_size).max(start + 1);
        start..end
    };
    for y in 0..next_height {
        for x in 0..next_width {
            let mut color = [0_u32; 3];
            let mut alpha = 0_u32;
            let mut count = 0_u32;
            for source_y in span(y, height, next_height) {
                for source_x in span(x, width, next_width) {
                    let offset = ((source_y * width + source_x) * BYTES_PER_PIXEL) as usize;
                    let texel = &pixels[offset..offset + 4];
                    let weight = u32::from(texel[3]);
                    for channel in 0..3 {
                        color[channel] += u32::from(texel[channel]) * weight;
                    }
                    alpha += weight;
                    count += 1;
                }
            }
            let offset = ((y * next_width + x) * BYTES_PER_PIXEL) as usize;
            // A fully transparent block keeps a transparent black texel.
            for channel in 0..3 {
                if let Some(average) = (color[channel] + alpha / 2).checked_div(alpha) {
                    next[offset + channel] = average as u8;
                }
            }
            next[offset + 3] = ((alpha + count / 2) / count) as u8;
        }
    }
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
            raster_scale: 1.0,
        })
    }
}

/// An uploaded layer texture. Cloning it clones the `wgpu` handles, not the
/// pixels, so a cached texture can back draws in several in-flight frames.
#[derive(Clone)]
pub(crate) struct LayerTexture {
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
    /// See `DecodedImage::raster_scale`.
    pub(crate) raster_scale: f32,
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

pub(crate) fn canvas_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
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
        format: wgpu::TextureFormat::Rgba8Unorm,
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
