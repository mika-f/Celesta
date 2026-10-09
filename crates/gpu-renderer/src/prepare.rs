use crate::bounds::CanvasRegion;
use crate::compositor::BackdropTexture;
use crate::draw::{LAYER_INSTANCE_SIZE, RectShape, clip_bind_group, clip_buffer, instance_buffer};
use crate::effect::EffectSpec;
use crate::error::GpuRenderError;
use crate::layer::{PreparedContent, PreparedItem, PreparedLayer};
use crate::mask::MaskSpec;
use crate::path::{PathEntries, PendingPath, path_entry_limit};
use crate::plan::{CompositePlan, GpuDraw, GpuStep, GroupPlan, PreparedDraws, plan_groups};
use crate::renderer::GpuRenderer;
use crate::text::{PendingText, text_layer, text_raster_scale};
use crate::texture::{DecodedImage, TEXT_TEXTURE_PREFIX, canvas_texture, psd_key};
use crate::texture::{rasterize_texts, text_jobs};
use crate::transform::{Affine, CLIP_ENTRY_SIZE, ClipEntry, LayerState, MAX_CLIP_DEPTH};
use crate::types::RenderQuality;
use celesta_composition::{BlendMode, Clip, Layer, LayerContent, LayerEffects, Point, Scene};
use celesta_renderer::image_source::{fit_within, resize_rgba};
use celesta_renderer::{FlattenedPath, flatten_path, rasterize_path, resolve_rect_paint};
use rayon::prelude::*;

impl GpuRenderer {
    pub(crate) fn prepare_draws(&mut self, scene: &Scene) -> Result<PreparedDraws, GpuRenderError> {
        if scene.width == 0 || scene.height == 0 {
            return Err(GpuRenderError::InvalidSurfaceSize {
                width: scene.width,
                height: scene.height,
            });
        }
        self.text_rasterizer
            .load_fonts(&scene.fonts, &self.asset_root)
            .map_err(GpuRenderError::Text)?;
        let font_count = self.text_rasterizer.loaded_font_count();
        if font_count != self.text_font_count {
            self.textures
                .retain(|key, _| !key.starts_with(TEXT_TEXTURE_PREFIX));
            self.text_workers.clear();
            self.text_font_count = font_count;
        }
        self.pending_texts.clear();
        self.pending_paths.clear();
        self.texture_generation += 1;
        self.clip_entries.clear();
        self.paint_entries.clear();
        self.path_entries.clear();
        self.scene_size = (scene.width, scene.height);
        self.font_fallbacks.clear();
        self.missing_glyphs.clear();
        let mut items = Vec::new();
        let prepared = scene
            .layers
            .iter()
            .try_for_each(|layer| self.prepare_layer(layer, LayerState::default(), &mut items));
        // Evict what this frame did not use, also when it failed part way
        // (the entries it did reach are still marked as used).
        let generation = self.texture_generation;
        self.textures
            .retain(|_, cached| cached.last_used == generation);
        prepared?;
        // The text the cache did not have and the paths, prepared on rayon's
        // threads at the same time.
        let texts = std::mem::take(&mut self.pending_texts);
        let paths = std::mem::take(&mut self.pending_paths);
        let jobs = text_jobs(&texts);
        let (limit, size) = (self.max_texture_dimension, self.scene_size);
        let entry_limit = path_entry_limit(&self.device.limits());
        let (rasterizer, workers) = (&mut self.text_rasterizer, &mut self.text_workers);
        let (images, outlines) = rayon::join(
            || rasterize_texts(rasterizer, workers, &jobs, limit),
            || outline_paths(&paths, size, entry_limit),
        );
        self.place_texts(&texts, &jobs, images, &mut items)?;
        self.place_paths(&paths, outlines?, &mut items)?;

        // A frame with an isolated group, an effect, a mask or a layer that
        // blends anything but source-over composites through canvases, so
        // its layers need one more instance: the one that copies the
        // finished root canvas onto the target.
        let composited = items.iter().any(
            |item| !matches!(item, PreparedItem::Layer(layer) if layer.blend_mode.is_normal()),
        );
        let instance_count = items
            .iter()
            .filter(|item| {
                !matches!(
                    item,
                    PreparedItem::BeginGroup | PreparedItem::BeginMask | PreparedItem::MaskContent
                )
            })
            .count()
            + items
                .iter()
                .filter(|item| matches!(item, PreparedItem::EndEffect(..)))
                .count()
            + usize::from(composited);

        // One buffer write for every layer, and one instanced draw for each
        // run of layers that sample the same texture, rather than a buffer,
        // a bind group, and a draw per layer: a dense scene has thousands of
        // layers, and consecutive rects all share the placeholder texture.
        let size = instance_count as u64 * LAYER_INSTANCE_SIZE;
        if size > self.device.limits().max_buffer_size || u32::try_from(instance_count).is_err() {
            return Err(GpuRenderError::TooManyLayers(instance_count));
        }
        let root = CanvasRegion::scene(scene.width, scene.height);
        let mut groups = plan_groups(&items, root).into_iter();
        let mut instances = Vec::with_capacity(size as usize);
        let mut steps: Vec<GpuStep> = Vec::new();
        let mut index = 0_u32;
        // The canvas each open group draws onto, the root canvas first.
        let mut open = vec![GroupPlan {
            canvas: root,
            content: None,
            drawn: true,
        }];
        for item in items {
            let target = open.last().expect("the root canvas is always open").canvas;
            let layer = match item {
                PreparedItem::Layer(layer) => layer,
                PreparedItem::PendingText | PreparedItem::PendingPath => {
                    unreachable!("pending text and paths are resolved")
                }
                PreparedItem::BeginGroup => {
                    let group = groups.next().expect("plan_groups plans every group");
                    steps.push(GpuStep::BeginGroup {
                        canvas: group.canvas,
                    });
                    open.push(group);
                    continue;
                }
                PreparedItem::BeginMask => {
                    let group = groups.next().expect("plan_groups plans every mask");
                    steps.push(GpuStep::BeginMask {
                        canvas: group.canvas,
                    });
                    open.push(group);
                    continue;
                }
                // The children draw onto a canvas covering the mask's region.
                PreparedItem::MaskContent => {
                    steps.push(GpuStep::MaskContent);
                    continue;
                }
                PreparedItem::EndMask(layer, mask) => {
                    let group = open.pop().expect("every mask was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndMask {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        mask,
                        area: group
                            .drawn
                            .then(|| target.local_area(group.canvas.bounds()))
                            .flatten(),
                    });
                    index += 1;
                    continue;
                }
                PreparedItem::EndGroup(layer) => {
                    let group = open.pop().expect("every group was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndGroup {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        area: group
                            .drawn
                            .then(|| target.local_area(group.canvas.bounds()))
                            .flatten(),
                    });
                    index += 1;
                    continue;
                }
                PreparedItem::EndEffect(layer, effects) => {
                    let group = open.pop().expect("every group was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    PreparedLayer::canvas(LayerState::default(), BlendMode::Normal, true)
                        .write_instance(group.canvas, group.canvas, &mut instances);
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndEffect {
                        inner_instance: index,
                        final_instance: index + 1,
                        blend_mode: layer.blend_mode,
                        effects,
                        content: group
                            .content
                            .filter(|_| group.drawn)
                            .map(|content| group.canvas.local(content)),
                        area: target.local_area(group.canvas.bounds()),
                    });
                    index += 2;
                    continue;
                }
            };
            layer.write_instance(target, target, &mut instances);
            let blend_mode = layer.blend_mode;
            let area = (!blend_mode.is_normal())
                .then(|| target.local_area(layer.bounds()))
                .flatten();
            let vertices = match &layer.content {
                PreparedContent::Path(path) => 0..path.tiles * 6,
                _ => 0..6,
            };
            let texture = match layer.content {
                PreparedContent::Texture(texture) => texture,
                PreparedContent::Rect(_) | PreparedContent::Path(_) => {
                    self.placeholder_texture.clone()
                }
                PreparedContent::Canvas { .. } => unreachable!("only a group's end draws a canvas"),
            };
            let draw = GpuDraw {
                texture,
                vertices,
                instances: index..index + 1,
            };
            if !blend_mode.is_normal() {
                steps.push(GpuStep::Blend {
                    draw,
                    blend_mode,
                    area,
                });
            } else {
                match steps.last_mut() {
                    Some(GpuStep::Draw(last))
                        if last.texture.bind_group == draw.texture.bind_group
                            && last.vertices == (0..6)
                            && draw.vertices == (0..6) =>
                    {
                        last.instances.end = index + 1;
                    }
                    _ => steps.push(GpuStep::Draw(draw)),
                }
            }
            index += 1;
        }
        let composite =
            composited.then(|| {
                PreparedLayer::canvas(LayerState::default(), BlendMode::Normal, false)
                    .write_instance(root, root, &mut instances);
                CompositePlan {
                    blit_instance: index,
                }
            });
        if size > self.instances.size() {
            // In-flight frames keep the old buffer alive until they finish.
            self.instances = instance_buffer(&self.device, size.next_power_of_two());
        }
        if !instances.is_empty() {
            // Safe while earlier frames that read this buffer are still in
            // flight: queued writes land after previously submitted work.
            self.queue.write_buffer(&self.instances, 0, &instances);
        }

        // The clips and paints the layers above point into, uploaded like
        // the instances.
        let clip_size = self.clip_entries.len() as u64 * CLIP_ENTRY_SIZE;
        let paint_size = self.paint_entries.len() as u64 * 16;
        let path_size = self.path_entries.len() as u64 * 16;
        // The largest path buffer the device can create and bind, in whole
        // entries.
        let limits = self.device.limits();
        let path_limit = path_entry_limit(&limits) as u64 * 16;
        if path_size > path_limit {
            return Err(GpuRenderError::PathsTooComplex(self.path_entries.len()));
        }
        if clip_size > self.clips.size()
            || paint_size > self.paints.size()
            || path_size > self.paths.size()
        {
            if clip_size > self.clips.size() {
                self.clips = clip_buffer(&self.device, clip_size.next_power_of_two());
            }
            if paint_size > self.paints.size() {
                self.paints = clip_buffer(&self.device, paint_size.next_power_of_two());
            }
            if path_size > self.paths.size() {
                self.paths =
                    clip_buffer(&self.device, path_size.next_power_of_two().min(path_limit));
            }
            self.clip_bind_group = clip_bind_group(
                &self.device,
                &self.clip_bind_group_layout,
                &self.clips,
                &self.paints,
                &self.paths,
            );
        }
        if !self.path_entries.is_empty() {
            self.queue
                .write_buffer(&self.paths, 0, bytemuck::cast_slice(&self.path_entries));
        }
        if !self.paint_entries.is_empty() {
            let bytes: Vec<u8> = self
                .paint_entries
                .iter()
                .flatten()
                .flat_map(|value| value.to_ne_bytes())
                .collect();
            self.queue.write_buffer(&self.paints, 0, &bytes);
        }
        if !self.clip_entries.is_empty() {
            let bytes: Vec<u8> = self
                .clip_entries
                .iter()
                .flat_map(ClipEntry::floats)
                .flat_map(f32::to_ne_bytes)
                .collect();
            self.queue.write_buffer(&self.clips, 0, &bytes);
        }
        Ok(PreparedDraws {
            instances: self.instances.clone(),
            clips: self.clip_bind_group.clone(),
            steps,
            composite,
        })
    }
}

impl GpuRenderer {
    /// Makes sure the root canvas and the backdrop exist at `width`x`height`.
    pub(crate) fn prepare_canvases(&mut self, width: u32, height: u32) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        if self
            .canvas
            .as_ref()
            .is_none_or(|canvas| canvas.texture.size() != size)
        {
            self.canvas = Some(canvas_texture(
                &self.device,
                &self.texture_bind_group_layout,
                width,
                height,
            ));
        }
        if self
            .backdrop
            .as_ref()
            .is_none_or(|backdrop| backdrop.texture.size() != size)
        {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Celesta backdrop"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Celesta backdrop bind group"),
                layout: &self.backdrop_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            self.backdrop = Some(BackdropTexture {
                texture,
                bind_group,
            });
        }
    }
}

impl GpuRenderer {
    pub(crate) fn prepare_layer(
        &mut self,
        layer: &Layer,
        parent: LayerState,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        let state = parent.then(&layer.transform, layer.opacity);
        if state.opacity == 0.0 || state.transform.is_degenerate() {
            return Ok(());
        }
        let blend_mode = layer.blend_mode;
        if !layer.effects.is_empty() {
            let effects = EffectSpec::parse(&layer.effects)?;
            output.push(PreparedItem::BeginGroup);
            let mut inner = layer.clone();
            inner.opacity = 1.0;
            inner.blend_mode = BlendMode::Normal;
            inner.effects = LayerEffects::default();
            self.prepare_layer(
                &inner,
                LayerState {
                    opacity: 1.0,
                    ..parent
                },
                output,
            )?;
            output.push(PreparedItem::EndEffect(
                PreparedLayer::canvas(
                    LayerState {
                        transform: Affine::IDENTITY,
                        opacity: state.opacity,
                        clip: parent.clip,
                    },
                    blend_mode,
                    true,
                ),
                effects,
            ));
            return Ok(());
        }
        match &layer.content {
            LayerContent::Group { layers, clip, mask } => {
                let mut child_state = state;
                if let Some(clip) = clip {
                    if clip.is_empty() {
                        return Ok(());
                    }
                    child_state.clip = Some(self.push_clip(clip, state)?);
                }
                if let Some(mask) = mask {
                    // Always isolated: the mask, drawn without clips, then
                    // the children, which carry them, each onto a canvas of
                    // their own; the children shown through the mask then
                    // draw onto the parent as one layer.
                    output.push(PreparedItem::BeginMask);
                    let matte = LayerState {
                        opacity: 1.0,
                        clip: None,
                        ..state
                    };
                    for layer in &mask.layers {
                        self.prepare_layer(layer, matte, output)?;
                    }
                    output.push(PreparedItem::MaskContent);
                    let inner = LayerState {
                        opacity: 1.0,
                        ..child_state
                    };
                    for child in layers {
                        self.prepare_layer(child, inner, output)?;
                    }
                    output.push(PreparedItem::EndMask(
                        PreparedLayer::canvas(
                            LayerState {
                                transform: Affine::IDENTITY,
                                opacity: state.opacity,
                                clip: None,
                            },
                            blend_mode,
                            true,
                        ),
                        MaskSpec::new(mask),
                    ));
                    return Ok(());
                }
                if blend_mode.is_normal() {
                    for child in layers {
                        self.prepare_layer(child, child_state, output)?;
                    }
                    return Ok(());
                }
                // Isolated: the children draw onto a transparent canvas of
                // their own, which then blends onto its parent as one layer.
                // The children carry the clip; the canvas itself does not.
                output.push(PreparedItem::BeginGroup);
                let inner = LayerState {
                    opacity: 1.0,
                    ..child_state
                };
                for child in layers {
                    self.prepare_layer(child, inner, output)?;
                }
                output.push(PreparedItem::EndGroup(PreparedLayer::canvas(
                    LayerState {
                        transform: Affine::IDENTITY,
                        opacity: state.opacity,
                        clip: None,
                    },
                    blend_mode,
                    true,
                )));
            }
            LayerContent::Image {
                asset,
                width,
                height,
                fit,
            } => {
                // Largest singular value also covers shear from rotated,
                // non-uniformly scaled parent groups.
                let m = state.transform;
                let x = (m.a as f64).powi(2) + (m.b as f64).powi(2);
                let y = (m.c as f64).powi(2) + (m.d as f64).powi(2);
                let dot = m.a as f64 * m.c as f64 + m.b as f64 * m.d as f64;
                let density = ((x + y + ((x - y).powi(2) + 4.0 * dot * dot).sqrt()) / 2.0).sqrt();
                let path = self.local_asset_path(asset)?;
                let display = self
                    .image_sources
                    .render(&asset.id, &path, *width, *height, *fit, density)
                    .map_err(|source| GpuRenderError::ImageDecode {
                        asset: asset.id.clone(),
                        source,
                    })?;
                let (texture_width, texture_height) = fit_within(
                    display.pixels.width(),
                    display.pixels.height(),
                    self.max_texture_dimension,
                );
                let mut state = state;
                state.transform.a *= (display.width / f64::from(texture_width)) as f32;
                state.transform.b *= (display.width / f64::from(texture_width)) as f32;
                state.transform.c *= (display.height / f64::from(texture_height)) as f32;
                state.transform.d *= (display.height / f64::from(texture_height)) as f32;
                let texture = self.cached_texture(
                    // Both sizes: SVG rasterized at different densities can
                    // shrink to the same texture size from different pixels.
                    format!(
                        "image\0{}\0{:?}\0{:?}\0{:?}\0{}x{}\0{}x{}",
                        asset.id,
                        width,
                        height,
                        fit,
                        display.pixels.width(),
                        display.pixels.height(),
                        texture_width,
                        texture_height
                    ),
                    // SVG is already rasterized for this draw; mipmaps blur
                    // its downscaled edges relative to the CPU renderer.
                    !display.is_svg,
                    |_| {
                        let pixels =
                            if (texture_width, texture_height) == display.pixels.dimensions() {
                                display.pixels.as_ref().clone()
                            } else {
                                resize_rgba(
                                    display.pixels.width(),
                                    display.pixels.height(),
                                    display.pixels.as_raw(),
                                    texture_width,
                                    texture_height,
                                )
                            };
                        DecodedImage::new(texture_width, texture_height, pixels.into_raw())
                    },
                )?;
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Psd {
                asset,
                visible_layers,
                enabled_layers,
                disabled_layers,
            } => {
                let path = self.local_asset_path(asset)?;
                let density = f64::from(state.transform.stretch().0);
                let image = self
                    .psd_sources
                    .render_within(
                        &asset.id,
                        &path,
                        visible_layers,
                        enabled_layers,
                        disabled_layers,
                        density,
                        self.max_texture_dimension,
                    )
                    .map_err(GpuRenderError::Psd)?;
                let mut state = state;
                let x = (f64::from(image.canvas_width) / f64::from(image.width)) as f32;
                let y = (f64::from(image.canvas_height) / f64::from(image.height)) as f32;
                state.transform.a *= x;
                state.transform.b *= x;
                state.transform.c *= y;
                state.transform.d *= y;
                let texture = self.cached_texture(
                    // Keyed like the composite: a composite shrunk to the
                    // limit can match another level's size with other pixels.
                    format!(
                        "{}\0{}\0{}\0{}x{}",
                        psd_key(asset, visible_layers, enabled_layers, disabled_layers),
                        celesta_renderer::psd_source::level_for(density),
                        self.max_texture_dimension,
                        image.width,
                        image.height
                    ),
                    true,
                    |_| DecodedImage::shared(image.width, image.height, image.pixels.clone()),
                )?;
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Video { asset, timing } => {
                let path = self.local_asset_path(asset)?;
                let decoder = self
                    .video_decoder
                    .as_mut()
                    .ok_or_else(|| GpuRenderError::MissingVideoDecoder(layer.id.clone()))?;
                let frame =
                    decoder.decode_frame_for(&layer.id, &path, timing.source_time_seconds)?;
                let (width, height) =
                    fit_within(frame.width, frame.height, self.max_texture_dimension);
                // Checked against its size before anything reads it.
                let mut image = DecodedImage::shared(frame.width, frame.height, frame.pixels)?;
                if (width, height) != (frame.width, frame.height) {
                    let pixels =
                        resize_rgba(frame.width, frame.height, &image.pixels, width, height);
                    image = DecodedImage::new(width, height, pixels.into_raw())?;
                }
                let mut state = state;
                let x = (f64::from(frame.width) / f64::from(width)) as f32;
                let y = (f64::from(frame.height) / f64::from(height)) as f32;
                state.transform.a *= x;
                state.transform.b *= x;
                state.transform.c *= y;
                state.transform.d *= y;
                // Every frame brings new pixels, so video is never cached,
                // and its mipmaps would be rebuilt every frame: only a final
                // render of a frame shrunk to half size or less pays for them.
                let mipmaps = self.render_quality == RenderQuality::Final
                    && state.transform.texels_per_pixel(1.0) >= 2.0;
                let texture = self.upload_texture(&image, mipmaps);
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Text {
                text,
                style,
                max_width,
                baseline_anchor,
            } => {
                if let Some(fallback) = self.text_rasterizer.font_fallback(&layer.id, style) {
                    if !self.font_fallbacks.iter().any(|reported| {
                        reported.family == fallback.family && reported.weight == fallback.weight
                    }) {
                        self.font_fallbacks.push(fallback);
                    }
                } else if let Some(missing) =
                    self.text_rasterizer.missing_glyphs(&layer.id, text, style)
                    && !self.missing_glyphs.iter().any(|reported| {
                        reported.family == missing.family
                            && reported.weight == missing.weight
                            && reported.characters == missing.characters
                    })
                {
                    self.missing_glyphs.push(missing);
                }
                let raster_scale = match self.render_quality {
                    RenderQuality::Draft => 1.0,
                    RenderQuality::Final => text_raster_scale(state.transform),
                };
                // `{:?}` spells out every style field and prints floats
                // exactly, so equal keys always mean equal rasterizer input.
                let key = format!(
                    "{TEXT_TEXTURE_PREFIX}{text}\0{style:?}\0{max_width:?}\0{raster_scale:?}"
                );
                let generation = self.texture_generation;
                if let Some(cached) = self.textures.get_mut(&key) {
                    cached.last_used = generation;
                    output.push(PreparedItem::Layer(text_layer(
                        cached.texture.clone(),
                        layer.transform.anchor,
                        *baseline_anchor,
                        state,
                        blend_mode,
                    )));
                } else {
                    // Rasterized with the frame's other new text once every
                    // layer is prepared; see `rasterize_texts`.
                    self.pending_texts.push(PendingText {
                        item: output.len(),
                        key,
                        text: text.clone(),
                        style: style.clone(),
                        max_width: *max_width,
                        raster_scale,
                        anchor: layer.transform.anchor,
                        baseline_anchor: *baseline_anchor,
                        state,
                        blend_mode,
                    });
                    output.push(PreparedItem::PendingText);
                }
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                // Shaded on the GPU rather than rasterized into a texture:
                // animated rects change size (and gradients colors) every
                // frame, and a texture per rect per frame dominates the cost
                // of dense geometry and of full-screen gradients alike.
                let paint = resolve_rect_paint(fill.as_ref(), stroke.as_ref())
                    .map_err(GpuRenderError::Text)?;
                output.push(PreparedItem::Layer(PreparedLayer {
                    content: PreparedContent::Rect(RectShape::new(
                        *width,
                        *height,
                        *corner_radius,
                        paint,
                        &mut self.paint_entries,
                    )),
                    anchor: layer.transform.anchor,
                    state,
                    blend_mode,
                    raster_scale: 1.0,
                }));
            }
            LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                miter_limit,
            } => {
                // Outlined and flattened on the CPU, with the whole
                // transform applied to the geometry exactly as the CPU
                // renderer's rasterizer does; the coverage of every output
                // pixel is then shaded by `layer.wgsl`, so an animated path
                // costs neither a rasterization nor a texture upload per frame.
                // It is outlined with the frame's other paths once every layer
                // is prepared; see `outline_paths`.
                self.pending_paths.push(PendingPath {
                    item: output.len(),
                    commands: commands.clone(),
                    fill: fill.clone(),
                    stroke: stroke.clone(),
                    line_cap: *line_cap,
                    line_join: *line_join,
                    miter_limit: *miter_limit,
                    state,
                    blend_mode,
                });
                output.push(PreparedItem::PendingPath);
            }
            LayerContent::MissingComponent { .. } => {
                return Err(GpuRenderError::UnsupportedContent {
                    layer: layer.id.clone(),
                    content: "missing component",
                });
            }
        }
        Ok(())
    }

    /// Enters `clip`, defined in the frame `state` describes (the group's
    /// own), inside the clip `state` is already drawn through, and returns
    /// its index for the layers below it.
    pub(crate) fn push_clip(
        &mut self,
        clip: &Clip,
        state: LayerState,
    ) -> Result<u32, GpuRenderError> {
        let depth = state
            .clip
            .map_or(1, |parent| self.clip_entries[parent as usize].depth + 1);
        if depth > MAX_CLIP_DEPTH {
            return Err(GpuRenderError::ClipsNestedTooDeep(depth));
        }
        let index = u32::try_from(self.clip_entries.len())
            .map_err(|_| GpuRenderError::TooManyLayers(self.clip_entries.len()))?;
        self.clip_entries
            .push(ClipEntry::new(clip, state.transform, state.clip, depth));
        Ok(index)
    }
}

impl GpuRenderer {
    /// Outlines the paths `prepare_layer` deferred and puts their layers in
    /// place of their `PendingPath` items, as `prepare_draws` does.
    #[cfg(test)]
    pub(crate) fn resolve_pending_paths(
        &mut self,
        items: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        let paths = std::mem::take(&mut self.pending_paths);
        let entry_limit = path_entry_limit(&self.device.limits());
        let outlines = outline_paths(&paths, self.scene_size, entry_limit)?;
        self.place_paths(&paths, outlines, items)
    }

    /// Puts the layers of `pending`, outlined by `outline_paths`, in place
    /// of their `PendingPath` items, and drops the items of paths that cover
    /// no pixel. Only where each path's entries go in the frame's buffer is
    /// chosen in order; they are copied there in parallel.
    pub(crate) fn place_paths(
        &mut self,
        pending: &[PendingPath],
        outlines: Vec<PathOutline>,
        items: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        if pending.is_empty() {
            return Ok(());
        }
        let (width, height) = self.scene_size;
        let entry_limit = path_entry_limit(&self.device.limits());
        // The shaded paths' entries and where they go, in order.
        let mut placed = Vec::new();
        let mut end = self.path_entries.len();
        for (path, outline) in pending.iter().zip(&outlines) {
            let Some((flattened, entries)) = outline else {
                continue;
            };
            let mut transform = Affine {
                tx: flattened.left as f32,
                ty: flattened.top as f32,
                ..Affine::IDENTITY
            };
            let content = match entries {
                Some(entries) if entries.tiles() == 0 => continue,
                Some(entries) if entries.fits(end, entry_limit) => {
                    let shaded = entries.shaded(flattened, end, &mut self.paint_entries);
                    placed.push((entries, end));
                    end += entries.len();
                    PreparedContent::Path(shaded)
                }
                // Dense tiles or a full storage buffer use the CPU rasterizer.
                _ => {
                    let Some(rasterized) =
                        rasterize_path(&path.shape(), path.state.transform.into(), width, height)
                            .map_err(GpuRenderError::Text)?
                    else {
                        continue;
                    };
                    let (raster_width, raster_height) =
                        (rasterized.image.width(), rasterized.image.height());
                    let (texture_width, texture_height) =
                        fit_within(raster_width, raster_height, self.max_texture_dimension);
                    let pixels = if (texture_width, texture_height) == (raster_width, raster_height)
                    {
                        rasterized.image.into_pixels()
                    } else {
                        resize_rgba(
                            raster_width,
                            raster_height,
                            rasterized.image.pixels(),
                            texture_width,
                            texture_height,
                        )
                        .into_raw()
                    };
                    // The raster already contains the layer transform;
                    // compensate only for shrinking its output-space pixels.
                    transform.a = raster_width as f32 / texture_width as f32;
                    transform.d = raster_height as f32 / texture_height as f32;
                    let image = DecodedImage::new(texture_width, texture_height, pixels)?;
                    PreparedContent::Texture(self.upload_texture(&image, false))
                }
            };
            items[path.item] = PreparedItem::Layer(PreparedLayer {
                content,
                anchor: Point { x: 0.0, y: 0.0 },
                state: LayerState {
                    transform,
                    ..path.state
                },
                blend_mode: path.blend_mode,
                raster_scale: 1.0,
            });
        }
        // Each path's entries into its own part of the buffer.
        let start = self.path_entries.len();
        self.path_entries.resize(end, [0.0; 4]);
        let mut parts = Vec::with_capacity(placed.len());
        let mut rest = &mut self.path_entries[start..];
        for (entries, base) in placed {
            let (part, after) = std::mem::take(&mut rest).split_at_mut(entries.len());
            parts.push((entries, base, part));
            rest = after;
        }
        parts
            .into_par_iter()
            .for_each(|(entries, base, part)| entries.place(base, part));
        items.retain(|item| !matches!(item, PreparedItem::PendingPath));
        Ok(())
    }
}

/// A pending path flattened, and its entries for `paths` unless it needs
/// CPU rasterization; `None` when it covers no pixel.
pub(crate) type PathOutline = Option<(FlattenedPath, Option<PathEntries>)>;

/// Outlines, flattens and bins `pending` into tiles, building the entries of
/// those that fit `entry_limit`. Each path is independent of the others, and
/// a frame of animated paths has dozens, so they spread over rayon's threads.
pub(crate) fn outline_paths(
    pending: &[PendingPath],
    (width, height): (u32, u32),
    entry_limit: usize,
) -> Result<Vec<PathOutline>, GpuRenderError> {
    pending
        .par_iter()
        .map(|path| {
            let flattened = flatten_path(&path.shape(), path.state.transform.into(), width, height)
                .map_err(GpuRenderError::Text)?;
            Ok(flattened.map(|flattened| {
                let entries = PathEntries::build(&flattened, entry_limit);
                (flattened, entries)
            }))
        })
        .collect()
}
