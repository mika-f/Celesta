use crate::bounds::CanvasRegion;
use crate::compositor::BackdropTexture;
use crate::draw::{LAYER_INSTANCE_SIZE, RectShape, clip_bind_group, clip_buffer, instance_buffer};
use crate::effect::EffectSpec;
use crate::error::GpuRenderError;
use crate::hit::{HitLeaf, HitRegion};
use crate::instances::{InstanceColumns, PreparedInstanceShape, PreparedInstances};
use crate::layer::{InstanceBytes, PreparedContent, PreparedItem, PreparedLayer};
use crate::mask::MaskSpec;
use crate::path::{PathEntries, PendingPath, path_entry_limit};
use crate::plan::{CompositePlan, GpuDraw, GpuStep, GroupPlan, PreparedDraws, plan_groups};
use crate::plane::{corners_to_scene, hit_clips};
use crate::renderer::GpuRenderer;
use crate::text::{PendingText, TextPlacement, TextSize, Windowed, text_layer, text_raster_scale};
use crate::texture::{DecodedImage, LayerTexture, TEXT_TEXTURE_PREFIX, canvas_texture, psd_key};
use crate::texture::{rasterize_texts, text_jobs};
use crate::transform::{Affine, CLIP_ENTRY_SIZE, ClipEntry, LayerState, MAX_CLIP_DEPTH, Walk};
use crate::types::RenderQuality;
use celesta_composition::{
    BackfaceVisibility, BlendMode, Clip, ImageAnimation, ImageFit, InstanceShape, Layer,
    LayerContent, Matrix3, Point, ResolvedAsset, Scene, faces_away, multiply3, multiply3_flat,
};
use celesta_renderer::image_source::{display_size, fit_within, resize_rgba};
use celesta_renderer::psd_source::PsdSize;
use celesta_renderer::{
    Color as CpuColor, FlattenedPath, flatten_path, parse_color, rasterize_path, resolve_rect_paint,
};
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
        self.shaders.begin_scene(&self.device, scene)?;
        let fonts_revision = self.text_rasterizer.fonts_revision();
        if fonts_revision != self.text_fonts_revision {
            self.textures
                .retain(|key, _| !key.starts_with(TEXT_TEXTURE_PREFIX));
            self.text_sizes.clear();
            self.text_workers.clear();
            self.text_fonts_revision = fonts_revision;
        }
        self.pending_texts.clear();
        self.pending_paths.clear();
        self.texture_generation += 1;
        self.clip_entries.clear();
        self.paint_entries.clear();
        self.path_entries.clear();
        self.scene_size = (scene.width, scene.height);
        self.scene_area = u64::from(scene.width) * u64::from(scene.height);
        self.font_fallbacks.clear();
        self.missing_glyphs.clear();
        self.hit_leaves.clear();
        self.hit_regions.clear();
        self.plane_entries.clear();
        self.plane_video_frames.clear();
        self.measuring_plane = false;
        self.walk = Walk::default();
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
        self.text_sizes
            .retain(|_, size| size.last_used == generation);
        prepared?;
        // The text the cache did not have and the paths, prepared on rayon's
        // threads at the same time.
        let texts = std::mem::take(&mut self.pending_texts);
        let paths = std::mem::take(&mut self.pending_paths);
        let jobs = text_jobs(&texts);
        let limit = self.max_texture_dimension;
        let entry_limit = path_entry_limit(&self.device.limits());
        let (rasterizer, workers) = (&mut self.text_rasterizer, &mut self.text_workers);
        let (images, outlines) = rayon::join(
            || rasterize_texts(rasterizer, workers, &jobs, limit),
            || outline_paths(&paths, entry_limit),
        );
        self.place_texts(&texts, &jobs, images, &mut items)?;
        self.place_paths(&paths, outlines?, &mut items)?;
        if self.record_hit_regions {
            self.resolve_hit_regions(&items);
        }
        // The paths that cover no pixel.
        if !paths.is_empty() {
            items.retain(|item| !matches!(item, PreparedItem::PendingPath));
        }

        // A frame with an isolated group, an effect, a mask, a motion blur,
        // or a layer that blends anything but source-over composites through
        // canvases, so
        // its layers need one more instance: the one that copies the
        // finished root canvas onto the target.
        let composited = items.iter().any(|item| match item {
            PreparedItem::Layer(layer) => !layer.blend_mode.is_normal(),
            // A blended `Instances` layer is wrapped in an isolated group.
            PreparedItem::Instances(_) => false,
            _ => true,
        });
        let instance_count = items
            .iter()
            .map(|item| match item {
                PreparedItem::BeginGroup
                | PreparedItem::BeginMask
                | PreparedItem::MaskContent
                | PreparedItem::BeginAverage
                | PreparedItem::NextSample
                | PreparedItem::BeginPlane { .. } => 0,
                PreparedItem::EndEffect(..) => 2,
                PreparedItem::Instances(instances) => instances.count() as usize,
                _ => 1,
            })
            .sum::<usize>()
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
        if size > self.instances.size() {
            // In-flight frames keep the old buffer alive until they finish.
            self.instances = instance_buffer(&self.device, size.next_power_of_two());
        }
        // Safe while earlier frames that read this buffer are still in
        // flight: queued writes land after previously submitted work.
        let mut staging = std::num::NonZeroU64::new(size).map(|size| {
            self.queue
                .write_buffer_with(&self.instances, 0, size)
                .expect("the instance buffer holds the frame's instances")
        });
        let mut instances = InstanceBytes::new(
            staging
                .as_mut()
                .map(|staging| staging.slice(..))
                .unwrap_or_default(),
        );
        let mut steps: Vec<GpuStep> = Vec::new();
        let mut index = 0_u32;
        // The canvas each open group draws onto, the root canvas first.
        let mut open = vec![GroupPlan {
            canvas: root,
            content: None,
            shape: None,
            drawn: true,
            lands: Some(root.bounds()),
        }];
        for item in items {
            let target = open.last().expect("the root canvas is always open").canvas;
            let layer = match item {
                PreparedItem::Layer(layer) => layer,
                PreparedItem::Instances(particles) => {
                    particles.write_instances(target, &mut instances);
                    push_draw(
                        &mut steps,
                        particles.texture().unwrap_or(&self.placeholder_texture),
                        0..6,
                        index..index + particles.count(),
                    );
                    index += particles.count();
                    continue;
                }
                PreparedItem::PendingText | PreparedItem::PendingPath => {
                    unreachable!("pending text and paths are resolved")
                }
                // A plane draws onto a canvas of its own as a group does.
                PreparedItem::BeginGroup | PreparedItem::BeginPlane { .. } => {
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
                PreparedItem::BeginAverage => {
                    let group = groups.next().expect("plan_groups plans every average");
                    steps.push(GpuStep::BeginAverage {
                        canvas: group.canvas,
                    });
                    open.push(group);
                    continue;
                }
                // Each sample draws onto a canvas covering the group's region.
                PreparedItem::NextSample => {
                    steps.push(GpuStep::NextSample);
                    continue;
                }
                PreparedItem::EndAverage(layer) => {
                    let group = open.pop().expect("every average was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndAverage {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        area: group.area(target),
                    });
                    index += 1;
                    continue;
                }
                PreparedItem::EndMask(layer, mask) => {
                    let group = open.pop().expect("every mask was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndMask {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        mask: *mask,
                        area: group.area(target),
                    });
                    index += 1;
                    continue;
                }
                PreparedItem::EndGroup(layer) | PreparedItem::EndPlane(layer) => {
                    let group = open.pop().expect("every group was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndGroup {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        area: group.area(target),
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
                        effects: *effects,
                        content: group
                            .content
                            .filter(|_| group.drawn)
                            .map(|content| group.canvas.local(content)),
                        shape: group.shape,
                        area: group.lands.and_then(|lands| target.local_area(lands)),
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
            let texture = match &layer.content {
                PreparedContent::Texture(texture) => texture,
                PreparedContent::Rect(_) | PreparedContent::Path(_) => &self.placeholder_texture,
                PreparedContent::Canvas { .. } => unreachable!("only a group's end draws a canvas"),
            };
            if !blend_mode.is_normal() {
                steps.push(GpuStep::Blend {
                    draw: GpuDraw {
                        texture: texture.clone(),
                        vertices,
                        instances: index..index + 1,
                    },
                    blend_mode,
                    area,
                });
            } else {
                push_draw(&mut steps, texture, vertices, index..index + 1);
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
        debug_assert_eq!(
            instances.len() as u64,
            size,
            "every counted instance is written"
        );
        drop(staging);

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
    /// Makes sure the root canvas exists at `width`x`height`, and the
    /// backdrop at `backdrop` (width, height) or larger.
    pub(crate) fn prepare_canvases(&mut self, width: u32, height: u32, backdrop: (u32, u32)) {
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
                self.canvas_format,
                width,
                height,
            ));
        }
        // A backdrop grown for a plane larger than the scene is kept while
        // planes need it, so one that grows and shrinks does not reallocate
        // it every frame, and is scene-sized again once none does.
        let current = self
            .backdrop
            .as_ref()
            .map(|backdrop| backdrop.texture.size());
        if current.is_none_or(|current| {
            let current = (current.width, current.height);
            let too_small = current.0 < backdrop.0 || current.1 < backdrop.1;
            too_small || (backdrop == (width, height) && current != backdrop)
        }) {
            let size = wgpu::Extent3d {
                width: backdrop.0,
                height: backdrop.1,
                depth_or_array_layers: 1,
            };
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Celesta backdrop"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.canvas_format,
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
        let previous = self.walk;
        let linear = layer.transform.linear();
        let orientation = if layer.transform.is_flat() {
            multiply3_flat(&previous.orientation, &linear)
        } else {
            multiply3(&previous.orientation, &linear)
        };
        if layer.transform.backface_visibility == BackfaceVisibility::Hidden
            && faces_away(&orientation)
        {
            return Ok(());
        }
        self.walk.orientation = orientation;
        let prepared = self.prepare_visible_layer(layer, parent, &linear, output);
        self.walk = previous;
        prepared
    }

    /// `prepare_layer` once the layer faces the viewer (or shows its back):
    /// `self.walk` describes the layer, and `linear` is its 3D linear part.
    fn prepare_visible_layer(
        &mut self,
        layer: &Layer,
        parent: LayerState,
        linear: &Matrix3,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        let state = parent.then_affine(
            Affine::from_linear(linear, layer.transform.translation(linear)),
            layer.opacity,
        );
        if state.opacity == 0.0 || state.transform.is_degenerate() {
            return Ok(());
        }
        let blend_mode = layer.blend_mode;
        if !layer.effects.is_empty() {
            let effects = EffectSpec::parse(&layer.effects)?;
            let mut effects = if self.measuring_plane {
                effects.scaled_uncapped(self.walk.effect_scale)
            } else {
                effects.scaled(self.walk.effect_scale)
            };
            effects.shaders =
                self.shaders
                    .use_filters(&self.device, &layer.id, &layer.effects.filters)?;
            output.push(PreparedItem::BeginGroup);
            // The layer again onto the effect's canvas, at full opacity,
            // blending normally, and without effects, but placed and turned
            // as it is, so `self.walk` stays the layer's. At full opacity
            // and normal blending, only a mask isolates a group, and
            // `prepare_content` gives one a canvas before asking.
            self.prepare_drawn(
                layer,
                LayerState {
                    opacity: 1.0,
                    ..parent
                },
                LayerState {
                    opacity: 1.0,
                    ..state
                },
                (1.0, BlendMode::Normal),
                false,
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
                Box::new(effects),
            ));
            return Ok(());
        }
        self.prepare_drawn(
            layer,
            parent,
            state,
            (layer.opacity, blend_mode),
            layer.isolates_group(),
            output,
        )
    }

    /// `layer` without its effects, placed by `parent` and then its own
    /// transform (`state`), drawn onto its parent at `opacity` with
    /// `blend_mode`, which an effect sets to full and `Normal` for the copy
    /// it draws onto its canvas: through a canvas in its own plane when it
    /// is projective (the effects then apply to that canvas once
    /// projected), else as its content, a group isolated when `isolated`.
    fn prepare_drawn(
        &mut self,
        layer: &Layer,
        parent: LayerState,
        state: LayerState,
        (opacity, blend_mode): (f64, BlendMode),
        isolated: bool,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        if layer.transform.perspective_distance().is_some() && layer.transform.is_projective() {
            return self.prepare_plane(layer, parent, (opacity, blend_mode), output);
        }
        self.prepare_content(layer, state, blend_mode, isolated, output)
    }

    /// `layer`'s content, placed by `state` and drawn onto its parent with
    /// `blend_mode`; a group's children draw onto a canvas of their own when
    /// `isolated` (see `Layer::isolates_group`). The rest of
    /// `prepare_visible_layer`, which a plane also prepares its content with,
    /// as the layer again without its transform, opacity, blend mode, or
    /// effects.
    pub(crate) fn prepare_content(
        &mut self,
        layer: &Layer,
        state: LayerState,
        blend_mode: BlendMode,
        isolated: bool,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        let item = output.len();
        match &layer.content {
            LayerContent::Group {
                layers,
                clip,
                mask,
                motion_blur,
            } => {
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
                    // The mask is never seen, so it has no hit regions.
                    let leaves = self.hit_leaves.len();
                    for layer in &mask.layers {
                        self.prepare_layer(layer, matte, output)?;
                    }
                    self.hit_leaves.truncate(leaves);
                    output.push(PreparedItem::MaskContent);
                    let inner = LayerState {
                        opacity: 1.0,
                        ..child_state
                    };
                    if *motion_blur {
                        // Averaged onto a canvas of their own, which then
                        // draws onto the children's.
                        self.prepare_average(layers, inner, output)?;
                        output.push(PreparedItem::EndAverage(PreparedLayer::canvas(
                            LayerState {
                                transform: Affine::IDENTITY,
                                opacity: 1.0,
                                clip: None,
                            },
                            BlendMode::Normal,
                            true,
                        )));
                    } else {
                        for child in layers {
                            self.prepare_layer(child, inner, output)?;
                        }
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
                        Box::new(MaskSpec::new(mask)),
                    ));
                    return Ok(());
                }
                if *motion_blur {
                    // Always isolated: the samples' average draws onto the
                    // parent as one layer, as an isolated group's children
                    // do. The samples carry the clip.
                    self.prepare_average(
                        layers,
                        LayerState {
                            opacity: 1.0,
                            ..child_state
                        },
                        output,
                    )?;
                    output.push(PreparedItem::EndAverage(PreparedLayer::canvas(
                        LayerState {
                            transform: Affine::IDENTITY,
                            opacity: state.opacity,
                            clip: None,
                        },
                        blend_mode,
                        true,
                    )));
                    return Ok(());
                }
                if !isolated {
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
                animation,
            } => {
                let (texture, display_width, display_height) = self.image_texture(
                    asset,
                    *width,
                    *height,
                    *fit,
                    *animation,
                    largest_stretch(state.transform),
                    false,
                )?;
                let mut state = state;
                state.transform.a *= (display_width / f64::from(texture.width)) as f32;
                state.transform.b *= (display_width / f64::from(texture.width)) as f32;
                state.transform.c *= (display_height / f64::from(texture.height)) as f32;
                state.transform.d *= (display_height / f64::from(texture.height)) as f32;
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
                let (texture, size) = if self.measuring_plane {
                    // Only its size counts there, which the parsed PSD gives
                    // without compositing or uploading anything.
                    let size = self
                        .psd_sources
                        .size_within(&asset.id, &path, density, self.max_texture_dimension)
                        .map_err(GpuRenderError::Psd)?;
                    (self.stand_in_texture(size.width, size.height), size)
                } else {
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
                    let texture = self.cached_texture(
                        // Keyed like the composite: a composite shrunk to the
                        // limit can match another level's size with other
                        // pixels.
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
                    (texture, PsdSize::from(&image))
                };
                let mut state = state;
                let x = (f64::from(size.canvas_width) / f64::from(size.width)) as f32;
                let y = (f64::from(size.canvas_height) / f64::from(size.height)) as f32;
                state.transform.a *= x;
                state.transform.b *= x;
                state.transform.c *= y;
                state.transform.d *= y;
                output.push(PreparedItem::Layer(PreparedLayer::new(
                    texture,
                    layer.transform.anchor,
                    state,
                    blend_mode,
                )));
            }
            LayerContent::Video {
                asset,
                timing,
                width: display_width,
                height: display_height,
            } => {
                let path = self.local_asset_path(asset)?;
                let decoder = self
                    .video_decoder
                    .as_mut()
                    .ok_or_else(|| GpuRenderError::MissingVideoDecoder(layer.id.clone()))?;
                let source_time_seconds = if timing.r#loop {
                    decoder.loop_source_time(&path, timing.source_time_seconds)?
                } else {
                    timing.source_time_seconds
                };
                // A plane's content is prepared once to measure it and once
                // to draw it: the frame decoded for the first serves both.
                let measured = self.plane_video_frames.iter().find(|(id, time, _)| {
                    *id == layer.id && time.to_bits() == source_time_seconds.to_bits()
                });
                let frame = match measured {
                    Some((_, _, frame)) => frame.clone(),
                    None => {
                        let frame =
                            decoder.decode_frame_for(&layer.id, &path, source_time_seconds)?;
                        if self.measuring_plane {
                            self.plane_video_frames.push((
                                layer.id.clone(),
                                source_time_seconds,
                                frame.clone(),
                            ));
                        }
                        frame
                    }
                };
                let (width, height) =
                    fit_within(frame.width, frame.height, self.max_texture_dimension);
                let (display_width, display_height) = display_size(
                    f64::from(frame.width),
                    f64::from(frame.height),
                    *display_width,
                    *display_height,
                );
                let mut state = state;
                let x = (display_width / f64::from(width)) as f32;
                let y = (display_height / f64::from(height)) as f32;
                state.transform.a *= x;
                state.transform.b *= x;
                state.transform.c *= y;
                state.transform.d *= y;
                let texture = if self.measuring_plane {
                    // Only its size counts there: the frame is not uploaded.
                    self.stand_in_texture(width, height)
                } else {
                    // Checked against its size before anything reads it.
                    let mut image = DecodedImage::shared(frame.width, frame.height, frame.pixels)?;
                    if (width, height) != (frame.width, frame.height) {
                        let pixels =
                            resize_rgba(frame.width, frame.height, &image.pixels, width, height);
                        image = DecodedImage::new(width, height, pixels.into_raw())?;
                    }
                    // Every frame brings new pixels, so video is never
                    // cached, and its mipmaps would be rebuilt every frame:
                    // only a final render of a frame shrunk to half size or
                    // less pays for them.
                    let mipmaps = self.render_quality == RenderQuality::Final
                        && state.transform.texels_per_pixel(1.0) >= 2.0;
                    self.upload_texture(&image, mipmaps)
                };
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
                for fallback in self.text_rasterizer.font_fallback(&layer.id, style) {
                    if !self.font_fallbacks.iter().any(|reported| {
                        reported.families == fallback.families && reported.weight == fallback.weight
                    }) {
                        self.font_fallbacks.push(fallback);
                    }
                }
                // A family with no face is left out here: it is a fallback.
                for missing in self.text_rasterizer.missing_glyphs(&layer.id, text, style) {
                    if !self.missing_glyphs.iter().any(|reported| {
                        reported.families == missing.families
                            && reported.weight == missing.weight
                            && reported.characters == missing.characters
                    }) {
                        self.missing_glyphs.push(missing);
                    }
                }
                let raster_scale = match self.render_quality {
                    RenderQuality::Draft => 1.0,
                    RenderQuality::Final => text_raster_scale(state.transform),
                };
                // `{:?}` spells out every style field and prints floats
                // exactly, so equal keys always mean equal rasterizer input.
                let layout = format!("{text}\0{style:?}\0{max_width:?}");
                let mut key = format!("{TEXT_TEXTURE_PREFIX}{layout}\0{raster_scale:?}");
                let (mut window, mut shape) = (None, None);
                // A band of a plane draws only the text its canvas shows.
                let in_band = self.walk.plane.is_some() && !self.measuring_plane;
                // A text whose whole raster is cached fits a texture; any
                // other is measured (once) to see whether it does.
                let measured = (in_band || !self.textures.contains_key(&key))
                    .then(|| self.text_size(layout, text, style, *max_width));
                if let Some(size) = &measured {
                    let placement = TextPlacement::new(
                        size,
                        style,
                        *max_width,
                        layer.transform.anchor,
                        *baseline_anchor,
                    );
                    let bounds = placement.bounds(state.transform);
                    let (width, height) = self.scene_size;
                    let [left, top, right, bottom] = bounds.0;
                    if in_band
                        && (right <= 0.0
                            || bottom <= 0.0
                            || left >= width as f32
                            || top >= height as f32)
                    {
                        return Ok(());
                    }
                    if !self.measuring_plane {
                        match placement.windowed(
                            size.lines > 1,
                            state.transform,
                            raster_scale,
                            self.scene_size,
                            self.max_texture_dimension,
                        ) {
                            Windowed::Whole => {}
                            Windowed::Hidden => return Ok(()),
                            Windowed::Window(part) => {
                                key = format!("{key}\0{part:?}");
                                window = Some(part);
                                shape = Some(bounds);
                            }
                        }
                    }
                }
                let generation = self.texture_generation;
                if let Some(cached) = self.textures.get_mut(&key) {
                    cached.last_used = generation;
                    let mut prepared = text_layer(
                        cached.texture.clone(),
                        layer.transform.anchor,
                        *baseline_anchor,
                        state,
                        blend_mode,
                    );
                    prepared.shape = shape;
                    output.push(PreparedItem::Layer(prepared));
                } else {
                    // Rasterized with the frame's other new text once every
                    // layer is prepared; see `rasterize_texts`.
                    let size = measured.expect("a text whose raster is not cached is measured");
                    self.pending_texts.push(PendingText {
                        item: output.len(),
                        key,
                        text: text.clone(),
                        style: style.as_ref().clone(),
                        max_width: *max_width,
                        raster_scale,
                        anchor: layer.transform.anchor,
                        baseline_anchor: *baseline_anchor,
                        state,
                        blend_mode,
                        window,
                        shape,
                        size,
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
                    shape: None,
                }));
            }
            LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                miter_limit,
                line_dash,
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
                    line_dash: line_dash.clone(),
                    state,
                    blend_mode,
                    space: self.scene_size,
                });
                output.push(PreparedItem::PendingPath);
            }
            LayerContent::MissingComponent { .. } => {
                return Err(GpuRenderError::UnsupportedContent {
                    layer: layer.id.clone(),
                    content: "missing component",
                });
            }
            // The React bridge expands every one before handing a scene on.
            LayerContent::StaticRef { .. } => {
                return Err(GpuRenderError::UnsupportedContent {
                    layer: layer.id.clone(),
                    content: "static reference",
                });
            }
            LayerContent::Instances {
                count,
                shape,
                width,
                height,
                color,
                x,
                y,
                widths,
                heights,
                rotation,
                colors,
            } => {
                let color = match color {
                    Some(color) => parse_color(color).map_err(GpuRenderError::Text)?,
                    None => CpuColor::rgba(255, 255, 255, 255),
                };
                let mut columns = InstanceColumns {
                    x: x.clone(),
                    y: y.clone(),
                    widths: widths.clone(),
                    heights: heights.clone(),
                    rotation: rotation.clone(),
                    colors: colors.clone(),
                    width: *width,
                    height: *height,
                    natural: None,
                    color: [color.red, color.green, color.blue, color.alpha].map(f32::from),
                };
                let shape = match shape.as_ref() {
                    InstanceShape::Rect { corner_radius } => PreparedInstanceShape::Rect {
                        corner_radius: *corner_radius,
                    },
                    InstanceShape::Circle => PreparedInstanceShape::Circle,
                    InstanceShape::Image { asset } => {
                        let path = self.local_asset_path(asset)?;
                        let natural = self.image_sources.natural_size(&asset.id, &path).map_err(
                            |source| GpuRenderError::ImageDecode {
                                asset: asset.id.clone(),
                                source,
                            },
                        )?;
                        columns.natural = Some(natural);
                        // An SVG is rasterized once, for the particle drawn
                        // largest relative to its natural size, as an `Image`
                        // layer is for its own size; smaller particles sample
                        // its mipmaps.
                        let largest = (0..*count as usize)
                            .map(|i| {
                                let (width, height) = columns.size(i);
                                (width / natural.0).max(height / natural.1)
                            })
                            .filter(|scale| scale.is_finite())
                            .fold(1.0, f64::max);
                        let density = largest_stretch(state.transform) * largest;
                        let (texture, _, _) =
                            self.image_texture(asset, None, None, None, None, density, true)?;
                        PreparedInstanceShape::Image(texture)
                    }
                };
                // Isolated like a group when blended: the particles composite
                // with each other source-over at full opacity, then the canvas
                // blends onto the parent as one layer.
                let inner = if blend_mode.is_normal() {
                    state
                } else {
                    LayerState {
                        opacity: 1.0,
                        ..state
                    }
                };
                let Some(particles) = PreparedInstances::new(inner, shape, *count, columns) else {
                    return Ok(());
                };
                let particles = PreparedItem::Instances(Box::new(particles));
                if blend_mode.is_normal() {
                    output.push(particles);
                } else {
                    output.push(PreparedItem::BeginGroup);
                    output.push(particles);
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
            }
        }
        if self.record_hit_regions && !matches!(layer.content, LayerContent::Group { .. }) {
            // A blended `Instances` layer's particles follow the
            // `BeginGroup` that isolates them.
            let item = match output[item] {
                PreparedItem::BeginGroup => item + 1,
                _ => item,
            };
            self.hit_leaves.push(HitLeaf {
                item,
                layer_id: layer.id.clone(),
                clip: state.clip,
                plane: self.walk.plane,
            });
        }
        Ok(())
    }

    /// The layout of `text`, measured the first time a frame needs it and
    /// kept while frames use it; `layout` is its text, style, and width.
    fn text_size(
        &mut self,
        layout: String,
        text: &str,
        style: &celesta_composition::TextStyle,
        max_width: Option<f64>,
    ) -> TextSize {
        let generation = self.texture_generation;
        let rasterizer = &mut self.text_rasterizer;
        let size = self.text_sizes.entry(layout).or_insert_with(|| {
            TextSize::of(&rasterizer.measure(text, style, max_width), generation)
        });
        size.last_used = generation;
        *size
    }

    /// Begins a motion-blurred group's canvas and prepares its `samples`,
    /// each drawn through `inner`, with a `NextSample` between them. The
    /// caller ends the canvas with an `EndAverage`.
    fn prepare_average(
        &mut self,
        samples: &[Layer],
        inner: LayerState,
        output: &mut Vec<PreparedItem>,
    ) -> Result<(), GpuRenderError> {
        output.push(PreparedItem::BeginAverage);
        for (index, sample) in samples.iter().enumerate() {
            if index > 0 {
                output.push(PreparedItem::NextSample);
            }
            self.prepare_layer(sample, inner, output)?;
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
        let outlines = outline_paths(&paths, entry_limit)?;
        self.place_paths(&paths, outlines, items)?;
        items.retain(|item| !matches!(item, PreparedItem::PendingPath));
        Ok(())
    }

    /// Puts the layers of `pending`, outlined by `outline_paths`, in place
    /// of their `PendingPath` items; the caller drops the items of paths
    /// that cover no pixel. Only where each path's entries go in the frame's
    /// buffer is chosen in order; they are copied there in parallel.
    pub(crate) fn place_paths(
        &mut self,
        pending: &[PendingPath],
        outlines: Vec<PathOutline>,
        items: &mut [PreparedItem],
    ) -> Result<(), GpuRenderError> {
        if pending.is_empty() {
            return Ok(());
        }
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
                    let (width, height) = path.space;
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
                shape: path.shape_box(),
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
        Ok(())
    }

    /// Turns the frame's `hit_leaves` into `hit_regions`, from `items` with
    /// every text and path placed but before the paths that cover no pixel
    /// (which record nothing) are dropped. A layer whose quad has no area,
    /// such as a rect 0 wide, records nothing either. The leaves were met
    /// in painter's order.
    fn resolve_hit_regions(&mut self, items: &[PreparedItem]) {
        let (clips, planes) = (&self.clip_entries, &self.plane_entries);
        self.hit_regions = self
            .hit_leaves
            .drain(..)
            .filter_map(|leaf| {
                let (corners, hole, anchor_box, text_box) = match &items[leaf.item] {
                    PreparedItem::Layer(layer) => (
                        layer.corners(),
                        layer.hole_corners(),
                        layer.anchor_box_corners(),
                        layer.text_box_corners(),
                    ),
                    PreparedItem::Instances(particles) => {
                        (particles.shape().corners(), None, None, None)
                    }
                    _ => return None,
                };
                // A leaf in a plane is in the plane's canvas pixels.
                let corners = corners_to_scene(planes, leaf.plane, corners)?;
                let to_scene = |quad: Option<[Point; 4]>| {
                    quad.map_or(Some(None), |quad| {
                        corners_to_scene(planes, leaf.plane, quad).map(Some)
                    })
                };
                let region = HitRegion {
                    layer_id: leaf.layer_id,
                    corners,
                    hole: to_scene(hole)?,
                    // A box that doesn't map leaves the layer hittable,
                    // without an anchor box (or a text box).
                    anchor_box: anchor_box
                        .and_then(|quad| corners_to_scene(planes, leaf.plane, quad)),
                    text_box: text_box.and_then(|quad| corners_to_scene(planes, leaf.plane, quad)),
                    clips: hit_clips(clips, planes, leaf.plane, leaf.clip)?,
                };
                (region.area() > 0.0).then_some(region)
            })
            .collect();
    }
}

impl GpuRenderer {
    /// The texture `asset` draws from, rasterized (an SVG) for `density`
    /// output pixels per layer unit, and its display size in layer units.
    /// A raster image always has mipmaps; an SVG only with `svg_mipmaps`.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the Image layer's fields, which `ImageSources::render` takes the same way"
    )]
    pub(crate) fn image_texture(
        &mut self,
        asset: &ResolvedAsset,
        width: Option<f64>,
        height: Option<f64>,
        fit: Option<ImageFit>,
        animation: Option<ImageAnimation>,
        density: f64,
        svg_mipmaps: bool,
    ) -> Result<(LayerTexture, f64, f64), GpuRenderError> {
        let path = self.local_asset_path(asset)?;
        let decode_error = |source| GpuRenderError::ImageDecode {
            asset: asset.id.clone(),
            source,
        };
        if self.measuring_plane {
            // Only its size counts there: the texture it would draw from,
            // stood in for without rasterizing or uploading it.
            let layout = self
                .image_sources
                .layout(&asset.id, &path, width, height, fit, animation, density)
                .map_err(decode_error)?;
            let (texture_width, texture_height) = fit_within(
                layout.pixel_width,
                layout.pixel_height,
                self.max_texture_dimension,
            );
            let texture = self.stand_in_texture(texture_width, texture_height);
            return Ok((texture, layout.width, layout.height));
        }
        let display = self
            .image_sources
            .render(&asset.id, &path, width, height, fit, animation, density)
            .map_err(decode_error)?;
        let (texture_width, texture_height) = fit_within(
            display.pixels.width(),
            display.pixels.height(),
            self.max_texture_dimension,
        );
        // An `Image` layer's SVG is rasterized for its own draw, and mipmaps
        // would blur its downscaled edges relative to the CPU renderer. One
        // SVG serves image particles of many sizes, so the smaller ones need
        // them.
        let mipmaps = svg_mipmaps || !display.is_svg;
        let texture = self.cached_texture(
            // Both sizes: SVG rasterized at different densities can shrink to
            // the same texture size from different pixels.
            format!(
                "image\0{}\0{:?}\0{:?}\0{:?}\0{}\0{}x{}\0{}x{}\0{}",
                asset.id,
                width,
                height,
                fit,
                display.frame,
                display.pixels.width(),
                display.pixels.height(),
                texture_width,
                texture_height,
                mipmaps,
            ),
            mipmaps,
            |_| {
                let pixels = if (texture_width, texture_height) == display.pixels.dimensions() {
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
        Ok((texture, display.width, display.height))
    }
}

/// The most `transform` stretches a length by (its largest singular
/// value), which also covers shear from rotated, non-uniformly scaled
/// parent groups: the output pixels per layer unit an image is rasterized
/// for.
fn largest_stretch(m: Affine) -> f64 {
    let x = (m.a as f64).powi(2) + (m.b as f64).powi(2);
    let y = (m.c as f64).powi(2) + (m.d as f64).powi(2);
    let dot = m.a as f64 * m.c as f64 + m.b as f64 * m.d as f64;
    ((x + y + ((x - y).powi(2) + 4.0 * dot * dot).sqrt()) / 2.0).sqrt()
}

/// Appends a source-over `draw`, joining it to the previous draw when both
/// sample the same texture with one quad per instance: a dense scene has
/// thousands of layers, and consecutive rects all share the placeholder.
/// The texture is cloned only for a new draw.
fn push_draw(
    steps: &mut Vec<GpuStep>,
    texture: &LayerTexture,
    vertices: std::ops::Range<u32>,
    instances: std::ops::Range<u32>,
) {
    match steps.last_mut() {
        Some(GpuStep::Draw(last))
            if last.texture.same(texture)
                && last.vertices == (0..6)
                && vertices == (0..6)
                && last.instances.end == instances.start =>
        {
            last.instances.end = instances.end;
        }
        _ => steps.push(GpuStep::Draw(GpuDraw {
            texture: texture.clone(),
            vertices,
            instances,
        })),
    }
}

/// A pending path flattened, and its entries for `paths` unless it needs
/// CPU rasterization; `None` when it covers no pixel.
pub(crate) type PathOutline = Option<(FlattenedPath, Option<PathEntries>)>;

/// Outlines, flattens and bins `pending` into tiles, each cut to its space,
/// building the entries of those that fit `entry_limit`. Each path is
/// independent of the others, and a frame of animated paths has dozens, so
/// they spread over rayon's threads.
pub(crate) fn outline_paths(
    pending: &[PendingPath],
    entry_limit: usize,
) -> Result<Vec<PathOutline>, GpuRenderError> {
    pending
        .par_iter()
        .map(|path| {
            let (width, height) = path.space;
            let flattened = flatten_path(&path.shape(), path.state.transform.into(), width, height)
                .map_err(GpuRenderError::Text)?;
            Ok(flattened.map(|flattened| {
                let entries = PathEntries::build(&flattened, entry_limit);
                (flattened, entries)
            }))
        })
        .collect()
}
