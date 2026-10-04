use crate::bounds::{CanvasRegion, PixelBounds};
use crate::draw::RectShape;
use crate::effect::EffectSpec;
use crate::path::ShadedPath;
use crate::texture::LayerTexture;
use crate::transform::{Affine, LayerState};
use celesta_composition::{BlendMode, Point};

pub(crate) enum PreparedContent {
    Texture(LayerTexture),
    Rect(RectShape),
    Path(ShadedPath),
    /// A whole scene-sized canvas: a finished isolated group, or the root
    /// canvas being copied onto the target.
    Canvas {
        /// Whether the canvas holds premultiplied alpha (an isolated group)
        /// that the shader must unpremultiply.
        premultiplied: bool,
    },
}

/// What `prepare_layer` produces, in painter's order.
pub(crate) enum PreparedItem {
    Layer(PreparedLayer),
    /// The items up to the matching `EndGroup` draw onto a transparent canvas
    /// of their own.
    BeginGroup,
    /// Draws the finished group's canvas onto its parent.
    EndGroup(PreparedLayer),
    EndEffect(PreparedLayer, EffectSpec),
    /// A text layer `resolve_pending_texts` replaces with a `Layer` before
    /// anything else reads the items.
    PendingText,
}

pub(crate) struct PreparedLayer {
    pub(crate) content: PreparedContent,
    pub(crate) anchor: Point,
    pub(crate) state: LayerState,
    pub(crate) blend_mode: BlendMode,
    /// Texels per layer unit: the scale text was rasterized at, 1 for
    /// everything else. The quad keeps the layer's size in layer units.
    pub(crate) raster_scale: f32,
}

impl PreparedLayer {
    pub(crate) fn new(
        texture: LayerTexture,
        anchor: Point,
        state: LayerState,
        blend_mode: BlendMode,
    ) -> Self {
        let raster_scale = texture.raster_scale;
        Self {
            content: PreparedContent::Texture(texture),
            anchor,
            state,
            blend_mode,
            raster_scale,
        }
    }

    pub(crate) const fn canvas(
        state: LayerState,
        blend_mode: BlendMode,
        premultiplied: bool,
    ) -> Self {
        Self {
            content: PreparedContent::Canvas { premultiplied },
            anchor: Point { x: 0.0, y: 0.0 },
            state,
            blend_mode,
            raster_scale: 1.0,
        }
    }

    /// Appends this layer's `LayerInstance` (`LAYER_INSTANCE_SIZE` bytes),
    /// drawn onto the canvas covering `target`. A `Canvas` layer draws the
    /// canvas covering `canvas`; other layers ignore it.
    pub(crate) fn write_instance(
        &self,
        target: CanvasRegion,
        canvas: CanvasRegion,
        output: &mut Vec<u8>,
    ) {
        let (texel_width, texel_height, kind) = match &self.content {
            PreparedContent::Texture(texture) => (texture.width, texture.height, 0.0),
            PreparedContent::Rect(rect) => (rect.pixel_width, rect.pixel_height, 1.0),
            PreparedContent::Path(path) => (path.width, path.height, 2.0),
            PreparedContent::Canvas { .. } => (canvas.width, canvas.height, 0.0),
        };
        let premultiplied = matches!(
            self.content,
            PreparedContent::Canvas {
                premultiplied: true
            }
        );
        let (rect, fill, stroke) = match &self.content {
            PreparedContent::Texture(_) | PreparedContent::Canvas { .. } => {
                ([0.0; 4], [0.0; 4], [0.0; 4])
            }
            PreparedContent::Rect(rect) => (
                [
                    rect.half_width,
                    rect.half_height,
                    rect.radius,
                    rect.stroke_width,
                ],
                rect.fill,
                rect.stroke,
            ),
            PreparedContent::Path(path) => ([path.base, 0.0, 0.0, 0.0], path.fill, path.stroke),
        };
        let mut transform = self.state.transform;
        if let PreparedContent::Canvas { .. } = self.content {
            // A canvas is drawn untransformed, where it sits in the scene.
            transform.tx += canvas.x as f32;
            transform.ty += canvas.y as f32;
        }
        // A layer whose texels land one to one on canvas pixels is copied
        // exactly; anything scaled or rotated is filtered.
        let exact = transform.is_uniform_scale(self.raster_scale);
        let (tx, ty) = if exact {
            self.pixel_aligned_translation(transform, texel_width, texel_height)
        } else {
            (transform.tx, transform.ty)
        };
        let level_of_detail = if exact {
            0.0
        } else {
            transform
                .texels_per_pixel(self.raster_scale)
                .max(1.0)
                .log2()
        };
        let values = [
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            tx,
            ty,
            texel_width as f32 / self.raster_scale,
            texel_height as f32 / self.raster_scale,
            self.anchor.x as f32,
            self.anchor.y as f32,
            self.state.opacity,
            kind,
            target.width as f32,
            target.height as f32,
            blend_mode_index(self.blend_mode),
            f32::from(u8::from(premultiplied)),
        ]
        .into_iter()
        .chain(rect)
        .chain(fill)
        .chain(stroke)
        // The innermost clip the layer is drawn through (-1 without one),
        // whether to filter, the mip level to filter at, and texels per
        // layer unit.
        .chain([
            self.state.clip.map_or(-1.0, |index| index as f32),
            f32::from(u8::from(!exact)),
            level_of_detail,
            self.raster_scale,
        ])
        .chain([target.x as f32, target.y as f32, 0.0, 0.0]);
        output.extend(values.flat_map(f32::to_ne_bytes));
    }

    /// Scene pixels the layer's quad can touch: the corners of the quad
    /// `vs_main` draws (a filtered layer's reaches one texel past its edge),
    /// grown by two pixels for the exact-copy rounding.
    pub(crate) fn bounds(&self) -> PixelBounds {
        let (texel_width, texel_height) = match &self.content {
            PreparedContent::Texture(texture) => (texture.width, texture.height),
            PreparedContent::Rect(rect) => (rect.pixel_width, rect.pixel_height),
            PreparedContent::Path(path) => (path.width, path.height),
            PreparedContent::Canvas { .. } => unreachable!("only a group's end draws a canvas"),
        };
        let width = texel_width as f32 / self.raster_scale;
        let height = texel_height as f32 / self.raster_scale;
        let transform = self.state.transform;
        let anchor = [self.anchor.x as f32, self.anchor.y as f32];
        let (margin_u, margin_v) = if transform.is_uniform_scale(self.raster_scale) {
            (0.0, 0.0)
        } else {
            (1.0 / texel_width as f32, 1.0 / texel_height as f32)
        };
        let (low_u, high_u) = (-margin_u, 1.0 + margin_u);
        let (low_v, high_v) = (-margin_v, 1.0 + margin_v);
        let corners = [
            [low_u, low_v],
            [high_u, low_v],
            [low_u, high_v],
            [high_u, high_v],
        ]
        .map(|[u, v]| {
            let x = (u - anchor[0]) * width;
            let y = (v - anchor[1]) * height;
            (
                transform.a * x + transform.c * y + transform.tx,
                transform.b * x + transform.d * y + transform.ty,
            )
        });
        let xs = corners.map(|(x, _)| x);
        let ys = corners.map(|(_, y)| y);
        let min = |values: [f32; 4]| values.into_iter().fold(f32::INFINITY, f32::min);
        let max = |values: [f32; 4]| values.into_iter().fold(f32::NEG_INFINITY, f32::max);
        PixelBounds([min(xs), min(ys), max(xs), max(ys)]).expand(2.0, 2.0)
    }

    /// The translation of a layer drawn texel for texel, moved so its
    /// top-left corner lands on a whole canvas pixel, as
    /// `celesta_renderer::render_image` places it. On a half pixel every pixel
    /// centre would sit exactly on a texel boundary, and f32 rounding would
    /// pick the left or right texel per column, notching glyph stems.
    pub(crate) fn pixel_aligned_translation(
        &self,
        transform: Affine,
        texel_width: u32,
        texel_height: u32,
    ) -> (f32, f32) {
        let align = |translation: f32, texels: u32, anchor: f64| {
            let offset = f64::from(texels) * anchor;
            ((f64::from(translation) - offset).round() + offset) as f32
        };
        (
            align(transform.tx, texel_width, self.anchor.x),
            align(transform.ty, texel_height, self.anchor.y),
        )
    }
}

/// `fs_blend`'s index for `mode` in `layer.wgsl`.
pub(crate) fn blend_mode_index(mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => 0.0,
        BlendMode::Multiply => 1.0,
        BlendMode::Screen => 2.0,
        BlendMode::Overlay => 3.0,
        BlendMode::Add => 4.0,
        BlendMode::Difference => 5.0,
    }
}
