use crate::assets::local_asset_path;
use crate::clip::{ClipNode, ClipRegion, ParentState, clip_coverage};
use crate::composite::{blend, blend_with_mode, mask_value};
use crate::effects::{blur_pixels, premultiply_pixels, render_effect_shadow, unpremultiply_color};
use crate::error::RenderError;
use crate::images::{DecodedImage, render_image, render_image_pixels, render_placeholder};
use crate::rect::{rasterize_rect_transformed, resolve_rect_paint};
use crate::renderer::CpuRenderer;
use crate::types::{Color, RgbaFrame};
use crate::{PathShape, PathTransform, RasterizedPath, rasterize_path};
use celesta_composition::{
    BlendMode, GroupMask, Layer, LayerContent, MediaTiming, Point, ResolvedAsset, TextStyle,
};
use std::path::PathBuf;
use std::sync::Arc;

impl CpuRenderer {
    pub(crate) fn render_layer(
        &mut self,
        frame: &mut RgbaFrame,
        layer: &Layer,
        parent: ParentState,
    ) -> Result<(), RenderError> {
        if layer.transform.rotation != 0.0 {
            return Err(RenderError::UnsupportedRotation {
                layer: layer.id.clone(),
                degrees: layer.transform.rotation,
            });
        }
        let state = ParentState {
            position: Point {
                x: parent.position.x + layer.transform.position.x * parent.scale.x,
                y: parent.position.y + layer.transform.position.y * parent.scale.y,
            },
            scale: Point {
                x: parent.scale.x * layer.transform.scale.x,
                y: parent.scale.y * layer.transform.scale.y,
            },
            opacity: (parent.opacity * layer.opacity).clamp(0.0, 1.0),
            clip: parent.clip.clone(),
            blend_mode: layer.blend_mode,
        };
        if state.opacity == 0.0 || state.scale.x == 0.0 || state.scale.y == 0.0 {
            return Ok(());
        }

        if !layer.effects.is_empty() {
            return self.render_effect_layer(frame, layer, parent, state);
        }

        match &layer.content {
            LayerContent::Text {
                text,
                style,
                max_width,
                baseline_anchor,
            } => self.render_text(
                frame,
                text,
                style,
                *max_width,
                layer.transform.anchor,
                *baseline_anchor,
                &state,
            )?,
            LayerContent::Group { layers, clip, mask } => {
                let mut child_state = state.clone();
                if let Some(clip) = clip {
                    if clip.is_empty() {
                        return Ok(());
                    }
                    child_state.clip = Some(Arc::new(ClipNode {
                        region: ClipRegion::new(clip, state.position, state.scale),
                        parent: state.clip.clone(),
                    }));
                }
                if let Some(mask) = mask {
                    return self.render_masked_group(frame, layers, mask, &state, child_state);
                }
                if !layer.blend_mode.is_normal() {
                    // Isolated: the children composite onto a transparent layer
                    // of their own, which then blends with the backdrop as one.
                    // The children carry the clip; the layer itself does not.
                    let mut isolated = RgbaFrame {
                        width: frame.width,
                        height: frame.height,
                        pixels: vec![0; frame.pixels.len()],
                    };
                    let inner = ParentState {
                        opacity: 1.0,
                        blend_mode: BlendMode::Normal,
                        ..child_state
                    };
                    for child in layers {
                        self.render_layer(&mut isolated, child, inner.clone())?;
                    }
                    for (destination, source) in frame
                        .pixels
                        .chunks_exact_mut(4)
                        .zip(isolated.pixels.chunks_exact(4))
                    {
                        let source = Color::rgba(source[0], source[1], source[2], source[3]);
                        blend_with_mode(destination, source, state.opacity, state.blend_mode);
                    }
                    return Ok(());
                }
                // Not isolated: each child composites straight onto the
                // backdrop, through its own blend mode.
                for child in layers {
                    self.render_layer(frame, child, child_state.clone())?;
                }
            }
            LayerContent::Video { asset, timing } => {
                if self.video_decoder.is_some() {
                    let image = self.decode_video_frame(&layer.id, asset, timing)?;
                    render_image(frame, &image, layer.transform.anchor, &state);
                } else {
                    render_placeholder(
                        frame,
                        layer.transform.anchor,
                        &state,
                        Color::rgba(54, 98, 176, 255),
                        320.0,
                        180.0,
                    );
                }
            }
            LayerContent::Image {
                asset,
                width,
                height,
                fit,
            } => {
                let path = self.local_asset_path(asset)?;
                let display = self
                    .image_sources
                    .render(
                        &asset.id,
                        &path,
                        *width,
                        *height,
                        *fit,
                        state.scale.x.abs().max(state.scale.y.abs()),
                    )
                    .map_err(|source| RenderError::ImageDecode {
                        asset: asset.id.clone(),
                        source,
                    })?;
                let mut state = state.clone();
                state.scale.x *= display.width / display.pixels.width() as f64;
                state.scale.y *= display.height / display.pixels.height() as f64;
                render_image_pixels(
                    frame,
                    display.pixels.width(),
                    display.pixels.height(),
                    display.pixels.as_raw(),
                    layer.transform.anchor,
                    &state,
                );
            }
            LayerContent::Psd {
                asset,
                visible_layers,
                enabled_layers,
                disabled_layers,
            } => {
                let path = self.local_asset_path(asset)?;
                let image = self.psd_sources.render(
                    &asset.id,
                    &path,
                    visible_layers,
                    enabled_layers,
                    disabled_layers,
                    state.scale.x.abs().max(state.scale.y.abs()),
                )?;
                let mut state = state.clone();
                state.scale.x *= f64::from(image.canvas_width) / f64::from(image.width);
                state.scale.y *= f64::from(image.canvas_height) / f64::from(image.height);
                render_image_pixels(
                    frame,
                    image.width,
                    image.height,
                    &image.pixels,
                    layer.transform.anchor,
                    &state,
                );
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                let paint = resolve_rect_paint(fill.as_ref(), stroke.as_ref())?;
                let anchor = layer.transform.anchor;
                let transform = PathTransform::scale_translate(
                    state.scale.x,
                    state.scale.y,
                    state.position.x - anchor.x * width * state.scale.x,
                    state.position.y - anchor.y * height * state.scale.y,
                );
                if let Some(rect) = rasterize_rect_transformed(
                    *width,
                    *height,
                    *corner_radius,
                    &paint,
                    transform,
                    frame.width,
                    frame.height,
                ) {
                    render_output_pixels(frame, &rect, &state);
                }
            }
            LayerContent::Path {
                commands,
                fill,
                stroke,
                line_cap,
                line_join,
                miter_limit,
            } => {
                let shape = PathShape {
                    commands,
                    fill: fill.as_ref(),
                    stroke: stroke.as_ref(),
                    line_cap: *line_cap,
                    line_join: *line_join,
                    miter_limit: *miter_limit,
                };
                let transform = PathTransform::scale_translate(
                    state.scale.x,
                    state.scale.y,
                    state.position.x,
                    state.position.y,
                );
                if let Some(path) = rasterize_path(&shape, transform, frame.width, frame.height)? {
                    render_output_pixels(frame, &path, &state);
                }
            }
            LayerContent::MissingComponent { .. } => render_placeholder(
                frame,
                layer.transform.anchor,
                &state,
                Color::rgba(220, 125, 42, 255),
                240.0,
                120.0,
            ),
        }
        Ok(())
    }

    pub(crate) fn render_effect_layer(
        &mut self,
        frame: &mut RgbaFrame,
        layer: &Layer,
        parent: ParentState,
        state: ParentState,
    ) -> Result<(), RenderError> {
        // WGSL runs only on the GPU renderer.
        if layer.effects.shader.is_some() {
            return Err(RenderError::UnsupportedShader {
                layer: layer.id.clone(),
            });
        }
        let mut source = RgbaFrame {
            width: frame.width,
            height: frame.height,
            pixels: vec![0; frame.pixels.len()],
        };
        let mut inner = layer.clone();
        inner.opacity = 1.0;
        inner.blend_mode = BlendMode::Normal;
        inner.effects = Default::default();
        self.render_layer(
            &mut source,
            &inner,
            ParentState {
                opacity: 1.0,
                ..parent.clone()
            },
        )?;

        let mut result = RgbaFrame {
            width: frame.width,
            height: frame.height,
            pixels: vec![0; frame.pixels.len()],
        };
        if let Some(shadow) = &layer.effects.shadow {
            render_effect_shadow(
                &mut result,
                &source,
                &shadow.color,
                shadow.blur,
                shadow.offset_x,
                shadow.offset_y,
            )?;
        }
        if let Some(glow) = &layer.effects.glow {
            render_effect_shadow(&mut result, &source, &glow.color, glow.blur, 0.0, 0.0)?;
        }
        let source_pixels = if layer.effects.blur > 0.0 {
            blur_pixels(&source, layer.effects.blur)
        } else {
            premultiply_pixels(&source.pixels)
        };
        for (destination, source) in result
            .pixels
            .chunks_exact_mut(4)
            .zip(source_pixels.chunks_exact(4))
        {
            blend(destination, unpremultiply_color(source), 1.0);
        }
        for (index, (destination, source)) in frame
            .pixels
            .chunks_exact_mut(4)
            .zip(result.pixels.chunks_exact(4))
            .enumerate()
        {
            let x = (index as u32 % frame.width) as i32;
            let y = (index as u32 / frame.width) as i32;
            let coverage = clip_coverage(&parent.clip, x, y);
            blend_with_mode(
                destination,
                Color::rgba(source[0], source[1], source[2], source[3]),
                state.opacity * coverage,
                state.blend_mode,
            );
        }
        Ok(())
    }

    /// Draws a group with a mask: the children and the mask each onto a
    /// transparent frame of their own, the children then shown through the
    /// mask onto `frame` with the group's opacity and blend mode. The
    /// children carry the clips; the mask draws without any.
    fn render_masked_group(
        &mut self,
        frame: &mut RgbaFrame,
        layers: &[Layer],
        mask: &GroupMask,
        state: &ParentState,
        child_state: ParentState,
    ) -> Result<(), RenderError> {
        let blank = || RgbaFrame {
            width: frame.width,
            height: frame.height,
            pixels: vec![0; frame.pixels.len()],
        };
        let mut children = blank();
        let inner = ParentState {
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            ..child_state
        };
        for child in layers {
            self.render_layer(&mut children, child, inner.clone())?;
        }
        let mut matte = blank();
        let matte_state = ParentState {
            opacity: 1.0,
            clip: None,
            blend_mode: BlendMode::Normal,
            ..state.clone()
        };
        for layer in &mask.layers {
            self.render_layer(&mut matte, layer, matte_state.clone())?;
        }
        for ((destination, source), matte) in frame
            .pixels
            .chunks_exact_mut(4)
            .zip(children.pixels.chunks_exact(4))
            .zip(matte.pixels.chunks_exact(4))
        {
            let shown = mask_value(mask.mode, mask.invert, matte);
            if shown == 0.0 {
                continue;
            }
            let source = Color::rgba(source[0], source[1], source[2], source[3]);
            blend_with_mode(destination, source, state.opacity * shown, state.blend_mode);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_text(
        &mut self,
        frame: &mut RgbaFrame,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        mut anchor: Point,
        baseline_anchor: bool,
        state: &ParentState,
    ) -> Result<(), RenderError> {
        if (state.scale.x.abs() - state.scale.y.abs()).abs() > f64::EPSILON {
            return Err(RenderError::UnsupportedNonUniformTextScale {
                x: state.scale.x,
                y: state.scale.y,
            });
        }
        let scale = state.scale.y.abs() as f32;
        let text = self
            .text_rasterizer
            .rasterize(text, style, max_width, scale)?;
        let (anchor_x, anchor_y) = text.anchor_in_image(anchor.x, anchor.y);
        anchor.x = anchor_x;
        anchor.y = if baseline_anchor {
            text.baseline_anchor()
        } else {
            anchor_y
        };
        let image = DecodedImage {
            width: text.width,
            height: text.height,
            pixels: text.pixels,
        };
        render_image(
            frame,
            &image,
            anchor,
            &ParentState {
                scale: Point { x: 1.0, y: 1.0 },
                ..state.clone()
            },
        );
        Ok(())
    }

    pub(crate) fn decode_video_frame(
        &mut self,
        request_id: &str,
        asset: &ResolvedAsset,
        timing: &MediaTiming,
    ) -> Result<DecodedImage, RenderError> {
        let path = self.local_asset_path(asset)?;
        let frame = self
            .video_decoder
            .as_mut()
            .expect("video decoder presence was checked")
            .decode_frame_for(request_id, &path, timing.source_time_seconds)?;
        Ok(DecodedImage {
            width: frame.width,
            height: frame.height,
            pixels: Arc::unwrap_or_clone(frame.pixels),
        })
    }

    pub(crate) fn local_asset_path(&self, asset: &ResolvedAsset) -> Result<PathBuf, RenderError> {
        local_asset_path(&self.asset_root, asset)
    }
}

/// Draws an image already in output pixels (a rasterized rect or path)
/// unscaled at its corner.
fn render_output_pixels(frame: &mut RgbaFrame, image: &RasterizedPath, state: &ParentState) {
    let state = ParentState {
        position: Point {
            x: f64::from(image.left),
            y: f64::from(image.top),
        },
        scale: Point { x: 1.0, y: 1.0 },
        ..state.clone()
    };
    render_image_pixels(
        frame,
        image.image.width,
        image.image.height,
        &image.image.pixels,
        Point { x: 0.0, y: 0.0 },
        &state,
    );
}
