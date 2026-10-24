use crate::bounds::{CanvasRegion, PixelBounds};
use crate::draw::RectShape;
use crate::effect::EffectSpec;
use crate::instances::PreparedInstances;
use crate::mask::MaskSpec;
use crate::path::ShadedPath;
use crate::texture::LayerTexture;
use crate::transform::{Affine, LayerState};
use celesta_composition::{BlendMode, Matrix3, Point};

pub(crate) enum PreparedContent {
    Texture(LayerTexture),
    Rect(RectShape),
    Path(ShadedPath),
    /// A whole canvas: a finished isolated group, a masked group's children
    /// shown through its mask, a plane, or the root canvas being copied onto
    /// the target.
    Canvas {
        /// Whether the canvas holds premultiplied alpha (an isolated group)
        /// that the shader must unpremultiply.
        premultiplied: bool,
        /// The bottom row of the map from the canvas's pixels to its target,
        /// whose top two rows are the layer's `state.transform`: `AFFINE`
        /// for every canvas but a plane's. Kept here, the only content that
        /// can be projective, so prepared items stay small.
        projection: [f32; 3],
        /// The pixels of its target a plane's band draws, `(u, v, e_far,
        /// e_near)` in target-space units: those where `s = u·x + v·y`
        /// has `s + e_far > 0` and `s + e_near ≤ 0`. `WHOLE_BAND` for every
        /// other canvas.
        band: [f32; 4],
    },
}

/// What `prepare_layer` produces, in painter's order.
pub(crate) enum PreparedItem {
    Layer(PreparedLayer),
    /// An `Instances` layer's particles, drawn source-over in order.
    Instances(Box<PreparedInstances>),
    /// The items up to the matching `EndGroup` draw onto a transparent canvas
    /// of their own.
    BeginGroup,
    /// Draws the finished group's canvas onto its parent.
    EndGroup(PreparedLayer),
    EndEffect(PreparedLayer, Box<EffectSpec>),
    /// The items up to the matching `MaskContent` draw a group's mask onto a
    /// transparent canvas, those from there to the matching `EndMask` its
    /// children onto another covering the same region.
    BeginMask,
    MaskContent,
    /// Draws the children shown through the mask onto the group's parent.
    EndMask(PreparedLayer, Box<MaskSpec>),
    /// The items up to the matching `EndAverage` are a motion-blurred
    /// group's samples, separated by `NextSample`s: each draws onto a
    /// transparent canvas of its own, and the group's canvas holds their
    /// average.
    BeginAverage,
    NextSample,
    /// Draws the averaged samples' canvas onto the group's parent.
    EndAverage(PreparedLayer),
    /// The items up to the matching `EndPlane` are a projective layer's
    /// content, drawn onto a transparent `width`x`height` canvas in its own
    /// plane, whose pixels they are placed in.
    BeginPlane {
        width: u32,
        height: u32,
    },
    /// Draws the finished plane's canvas onto its parent through the
    /// layer's homography (`PreparedLayer::projection`).
    EndPlane(PreparedLayer),
    /// A text layer `place_texts` replaces with a `Layer` before anything
    /// else reads the items.
    PendingText,
    /// A path layer `place_paths` replaces with a `Layer` before anything
    /// else reads the items; its callers drop the ones still pending, which
    /// cover no pixel.
    PendingPath,
}

pub(crate) struct PreparedLayer {
    pub(crate) content: PreparedContent,
    pub(crate) anchor: Point,
    pub(crate) state: LayerState,
    pub(crate) blend_mode: BlendMode,
    /// Texels per layer unit: the scale text was rasterized at, 1 for
    /// everything else. The quad keeps the layer's size in layer units.
    pub(crate) raster_scale: f32,
    /// The box of the layer's shape when it is not its quad's: a path's
    /// (`PendingPath::shape_box`), whose quad covers its outline's pixels.
    pub(crate) shape: Option<PixelBounds>,
}

/// `PreparedLayer::projection` of an affine layer.
pub(crate) const AFFINE: [f32; 3] = [0.0, 0.0, 1.0];

/// The `band` of a canvas that draws every pixel it lands on.
pub(crate) const WHOLE_BAND: [f32; 4] = [0.0, 0.0, 1.0, -1.0];

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
            shape: None,
        }
    }

    pub(crate) const fn canvas(
        state: LayerState,
        blend_mode: BlendMode,
        premultiplied: bool,
    ) -> Self {
        Self::plane_or_canvas(state, blend_mode, premultiplied, AFFINE, WHOLE_BAND)
    }

    /// A plane's premultiplied canvas, drawn through the map whose top two
    /// rows are `state.transform` and whose bottom row is `projection`, onto
    /// the pixels of `band` (see `PreparedContent::Canvas`).
    pub(crate) const fn plane(
        state: LayerState,
        blend_mode: BlendMode,
        projection: [f32; 3],
        band: [f32; 4],
    ) -> Self {
        Self::plane_or_canvas(state, blend_mode, true, projection, band)
    }

    const fn plane_or_canvas(
        state: LayerState,
        blend_mode: BlendMode,
        premultiplied: bool,
        projection: [f32; 3],
        band: [f32; 4],
    ) -> Self {
        Self {
            content: PreparedContent::Canvas {
                premultiplied,
                projection,
                band,
            },
            anchor: Point { x: 0.0, y: 0.0 },
            state,
            blend_mode,
            raster_scale: 1.0,
            shape: None,
        }
    }

    /// The bottom row of the map from the layer's quad units to its target,
    /// whose top two rows are `state.transform`: `AFFINE` for every layer
    /// but a plane's canvas.
    pub(crate) const fn projection(&self) -> [f32; 3] {
        match self.content {
            PreparedContent::Canvas { projection, .. } => projection,
            _ => AFFINE,
        }
    }

    /// Whether the layer is drawn through a homography rather than an
    /// affine map: a plane's canvas.
    pub(crate) fn is_projective(&self) -> bool {
        let [g, h, _] = self.projection();
        g != 0.0 || h != 0.0
    }

    /// The map from the layer's quad units to its target: the rows of
    /// `state.transform`, then `projection`.
    pub(crate) fn map(&self) -> Matrix3 {
        let t = self.state.transform;
        [[t.a, t.c, t.tx], [t.b, t.d, t.ty], self.projection()].map(|row| row.map(f64::from))
    }

    /// Appends this layer's `LayerInstance` (`LAYER_INSTANCE_SIZE` bytes),
    /// drawn onto the canvas covering `target`. A `Canvas` layer draws the
    /// canvas covering `canvas`; other layers ignore it.
    pub(crate) fn write_instance(
        &self,
        target: CanvasRegion,
        canvas: CanvasRegion,
        output: &mut InstanceBytes,
    ) {
        let (texel_width, texel_height, kind) = match &self.content {
            PreparedContent::Texture(texture) => (texture.width, texture.height, 0.0),
            // Unused: a rect is shaded at its exact size, `size` below.
            PreparedContent::Rect(_) => (1, 1, 1.0),
            PreparedContent::Path(path) => (path.width, path.height, 2.0),
            PreparedContent::Canvas { .. } => (canvas.width, canvas.height, 0.0),
        };
        let premultiplied = matches!(
            self.content,
            PreparedContent::Canvas {
                premultiplied: true,
                ..
            }
        );
        let (rect, fill, stroke) = match &self.content {
            PreparedContent::Texture(_) => ([0.0; 4], [0.0; 4], [0.0; 4]),
            // A band's values are in its target's space, and the shader
            // tests the fragment's pixel in the target's canvas, which
            // starts at `target`'s origin. Bands meeting at a boundary
            // carry the same value for it and offset it alike here, so each
            // pixel passes exactly one.
            &PreparedContent::Canvas {
                band: [u, v, far, near],
                ..
            } => {
                let offset = u * target.x as f32 + v * target.y as f32;
                ([0.0; 4], [u, v, far + offset, near + offset], [0.0; 4])
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
        // exactly; anything scaled or rotated is filtered. A rect has no
        // texels: it is shaded wherever it lands, so it is never moved onto
        // a whole pixel. A plane's canvas is filtered over each pixel's
        // footprint at its own level (it has no mips).
        let projective = self.is_projective();
        let exact = !projective && transform.is_uniform_scale(self.raster_scale);
        let is_rect = matches!(self.content, PreparedContent::Rect(_));
        let level_of_detail = if projective {
            0.0
        } else {
            level_of_detail(transform, self.raster_scale, exact)
        };
        if exact && !is_rect {
            (transform.tx, transform.ty) =
                self.pixel_aligned_translation(transform, texel_width, texel_height);
        }
        let instance = layer_instance(&InstanceFields {
            transform,
            size: self.size(texel_width, texel_height),
            anchor: [self.anchor.x as f32, self.anchor.y as f32],
            opacity: self.state.opacity,
            kind,
            target,
            blend_mode: self.blend_mode,
            premultiplied,
            rect,
            fill,
            stroke,
            clip: self.state.clip,
            exact,
            level_of_detail,
            raster_scale: self.raster_scale,
            projection: self.projection(),
        });
        output.push(&instance);
    }

    /// Scene pixels the layer's quad can touch: the corners of the quad
    /// `vs_main` draws (a filtered layer's reaches one texel past its edge),
    /// grown by two pixels for the exact-copy rounding.
    pub(crate) fn bounds(&self) -> PixelBounds {
        let (texel_width, texel_height) = self.texels();
        // A rect's quad reaches one pixel past its edge, within the growth
        // below.
        let margins = if matches!(self.content, PreparedContent::Rect(_))
            || self.state.transform.is_uniform_scale(self.raster_scale)
        {
            [0.0, 0.0]
        } else {
            [1.0 / texel_width as f32, 1.0 / texel_height as f32]
        };
        self.quad(self.state.transform, margins).expand(2.0, 2.0)
    }

    /// The box of the layer's own shape in scene pixels: the corners of the
    /// quad `write_instance` draws, without the margins `bounds` adds, or
    /// `shape` when set. A layer copied texel for texel lands on whole
    /// pixels, so its box does too.
    pub(crate) fn shape(&self) -> PixelBounds {
        if let Some(shape) = self.shape {
            return shape;
        }
        self.quad(self.drawn_transform(), [0.0, 0.0])
    }

    /// The corners of the layer's own shape in scene pixels, in order
    /// around it (see `HitRegion::corners`): the quad `shape` boxes, or the
    /// corners of `shape` when set.
    pub(crate) fn corners(&self) -> [Point; 4] {
        if let Some(shape) = self.shape {
            return shape.corners();
        }
        self.quad_corners(self.drawn_transform(), [0.0, 0.0])
            .map(|(x, y)| Point {
                x: f64::from(x),
                y: f64::from(y),
            })
    }

    /// For a stroked rect with no fill (or a fully transparent one), the
    /// corners of the part inside its stroke in scene pixels, in the order of
    /// `corners`; `None` for any other layer and for a stroke that leaves no
    /// inside. `rect_color` in `layer.wgsl` draws the stroke inside the
    /// rect's edge, so the hole is the rect inset by the stroke's width.
    pub(crate) fn hole_corners(&self) -> Option<[Point; 4]> {
        let PreparedContent::Rect(rect) = &self.content else {
            return None;
        };
        // A flat color's alpha, 0 without a fill; a gradient's is -1.
        if self.shape.is_some() || rect.stroke_width <= 0.0 || rect.fill[3] != 0.0 {
            return None;
        }
        let (width, height) = (2.0 * rect.half_width, 2.0 * rect.half_height);
        let stroke = rect.stroke_width;
        if width <= 2.0 * stroke || height <= 2.0 * stroke {
            return None;
        }
        Some(
            self.quad_corners(self.drawn_transform(), [-stroke / width, -stroke / height])
                .map(|(x, y)| Point {
                    x: f64::from(x),
                    y: f64::from(y),
                }),
        )
    }

    /// For a text, the corners of its layout box (what its anchor is a
    /// fraction of) in scene pixels, in the order of `corners`: the part of
    /// its texture's quad between the anchor box's edges, which can lie
    /// outside a window's texture. `None` when the anchor box is the whole
    /// texture, and for every other layer.
    pub(crate) fn anchor_box_corners(&self) -> Option<[Point; 4]> {
        let PreparedContent::Texture(texture) = &self.content else {
            return None;
        };
        let ((left, top), (width, height)) = (texture.anchor_origin, texture.anchor_span);
        if (left, top, width, height) == (0.0, 0.0, 1.0, 1.0) {
            return None;
        }
        let low = [left as f32, top as f32];
        let high = [(left + width) as f32, (top + height) as f32];
        Some(
            self.quad_corners_between(self.drawn_transform(), low, high)
                .map(|(x, y)| Point {
                    x: f64::from(x),
                    y: f64::from(y),
                }),
        )
    }

    /// For a text, the corners of its layout box (`HitRegion::text_box`) in
    /// scene pixels, in the order of `corners`: placed like
    /// `anchor_box_corners`. `None` for every other layer.
    pub(crate) fn text_box_corners(&self) -> Option<[Point; 4]> {
        let PreparedContent::Texture(texture) = &self.content else {
            return None;
        };
        let [left, top, right, bottom] = texture.text_box?;
        let (width, height) = self.size(texture.width, texture.height);
        let (width, height) = (f64::from(width), f64::from(height));
        let low = [(left / width) as f32, (top / height) as f32];
        let high = [(right / width) as f32, (bottom / height) as f32];
        Some(
            self.quad_corners_between(self.drawn_transform(), low, high)
                .map(|(x, y)| Point {
                    x: f64::from(x),
                    y: f64::from(y),
                }),
        )
    }

    /// The transform `write_instance` places the layer's quad with: a layer
    /// copied texel for texel lands on whole pixels.
    fn drawn_transform(&self) -> Affine {
        let mut transform = self.state.transform;
        let is_rect = matches!(self.content, PreparedContent::Rect(_));
        if !is_rect && transform.is_uniform_scale(self.raster_scale) {
            let (texel_width, texel_height) = self.texels();
            (transform.tx, transform.ty) =
                self.pixel_aligned_translation(transform, texel_width, texel_height);
        }
        transform
    }

    fn texels(&self) -> (u32, u32) {
        match &self.content {
            PreparedContent::Texture(texture) => (texture.width, texture.height),
            PreparedContent::Rect(_) => (1, 1),
            PreparedContent::Path(path) => (path.width, path.height),
            PreparedContent::Canvas { .. } => unreachable!("only a group's end draws a canvas"),
        }
    }

    /// The box of the layer's quad placed by `transform`, reaching
    /// `margin_u` and `margin_v` of its size past each edge.
    fn quad(&self, transform: Affine, margins: [f32; 2]) -> PixelBounds {
        let corners = self.quad_corners(transform, margins);
        let xs = corners.map(|(x, _)| x);
        let ys = corners.map(|(_, y)| y);
        let min = |values: [f32; 4]| values.into_iter().fold(f32::INFINITY, f32::min);
        let max = |values: [f32; 4]| values.into_iter().fold(f32::NEG_INFINITY, f32::max);
        PixelBounds([min(xs), min(ys), max(xs), max(ys)])
    }

    /// The corners of the quad `quad` boxes, in order around it.
    fn quad_corners(&self, transform: Affine, [margin_u, margin_v]: [f32; 2]) -> [(f32, f32); 4] {
        self.quad_corners_between(
            transform,
            [-margin_u, -margin_v],
            [1.0 + margin_u, 1.0 + margin_v],
        )
    }

    /// The corners of the part of the quad from `low` to `high`, in
    /// fractions of it (`[u, v]`, 0 to 1 the whole quad), in order around it.
    fn quad_corners_between(
        &self,
        transform: Affine,
        [low_u, low_v]: [f32; 2],
        [high_u, high_v]: [f32; 2],
    ) -> [(f32, f32); 4] {
        let (texel_width, texel_height) = self.texels();
        let (width, height) = self.size(texel_width, texel_height);
        let anchor = [self.anchor.x as f32, self.anchor.y as f32];
        [
            [low_u, low_v],
            [high_u, low_v],
            [high_u, high_v],
            [low_u, high_v],
        ]
        .map(|[u, v]| {
            let x = (u - anchor[0]) * width;
            let y = (v - anchor[1]) * height;
            (
                transform.a * x + transform.c * y + transform.tx,
                transform.b * x + transform.d * y + transform.ty,
            )
        })
    }

    /// The layer's size in layer units: a rect's own, or `texel_width` x
    /// `texel_height` texels at `raster_scale`.
    fn size(&self, texel_width: u32, texel_height: u32) -> (f32, f32) {
        match &self.content {
            PreparedContent::Rect(rect) => (2.0 * rect.half_width, 2.0 * rect.half_height),
            _ => (
                texel_width as f32 / self.raster_scale,
                texel_height as f32 / self.raster_scale,
            ),
        }
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
        pixel_aligned_translation(transform, texel_width, texel_height, self.anchor)
    }
}

/// See `PreparedLayer::pixel_aligned_translation`, for a layer anchored at
/// `anchor`.
pub(crate) fn pixel_aligned_translation(
    transform: Affine,
    texel_width: u32,
    texel_height: u32,
    anchor: Point,
) -> (f32, f32) {
    let align = |translation: f32, texels: u32, anchor: f64| {
        let offset = f64::from(texels) * anchor;
        ((f64::from(translation) - offset).round() + offset) as f32
    };
    (
        align(transform.tx, texel_width, anchor.x),
        align(transform.ty, texel_height, anchor.y),
    )
}

/// Floats in one `LayerInstance` of `layer.wgsl`: nine `vec4<f32>`s.
pub(crate) const LAYER_INSTANCE_FLOATS: usize = 36;

/// A frame's `LayerInstance`s, written in order straight into the memory
/// the queue uploads them from, so a frame of 100,000 layers is not first
/// gathered into a buffer of its own and then copied.
pub(crate) struct InstanceBytes<'a> {
    bytes: wgpu::WriteOnly<'a, [u8]>,
    len: usize,
}

impl<'a> InstanceBytes<'a> {
    pub(crate) const fn new(bytes: wgpu::WriteOnly<'a, [u8]>) -> Self {
        Self { bytes, len: 0 }
    }

    pub(crate) fn push(&mut self, instance: &[f32; LAYER_INSTANCE_FLOATS]) {
        let end = self.len + LAYER_INSTANCE_FLOATS * 4;
        self.bytes
            .slice(self.len..end)
            .copy_from_slice(bytemuck::bytes_of(instance));
        self.len = end;
    }

    /// Bytes written so far.
    pub(crate) const fn len(&self) -> usize {
        self.len
    }
}

/// Everything one `LayerInstance` holds, before it is laid out.
pub(crate) struct InstanceFields {
    /// Places the layer's local units; its translation is final (already
    /// moved onto a whole pixel where that applies).
    pub(crate) transform: Affine,
    /// The layer's size in layer units.
    pub(crate) size: (f32, f32),
    pub(crate) anchor: [f32; 2],
    pub(crate) opacity: f32,
    /// 0 samples the texture, 1 shades a rect, 2 shades a path.
    pub(crate) kind: f32,
    /// The canvas the instance draws onto.
    pub(crate) target: CanvasRegion,
    pub(crate) blend_mode: BlendMode,
    /// Whether the texture holds premultiplied alpha (an isolated group).
    pub(crate) premultiplied: bool,
    pub(crate) rect: [f32; 4],
    pub(crate) fill: [f32; 4],
    pub(crate) stroke: [f32; 4],
    /// The innermost clip the layer is drawn through.
    pub(crate) clip: Option<u32>,
    /// Whether texels land one to one on canvas pixels (no filtering).
    pub(crate) exact: bool,
    /// The mip level to filter at.
    pub(crate) level_of_detail: f32,
    /// Texels per layer unit.
    pub(crate) raster_scale: f32,
    /// The bottom row of the map `transform` is the top two rows of
    /// (`PreparedLayer::projection`); a projective one is filtered over each
    /// pixel's footprint, whatever `exact` says, and carries it in the
    /// `rect` slot, which a canvas leaves unused.
    pub(crate) projection: [f32; 3],
}

/// `fields` in `layer.wgsl`'s `LayerInstance` order.
pub(crate) fn layer_instance(fields: &InstanceFields) -> [f32; LAYER_INSTANCE_FLOATS] {
    let InstanceFields {
        transform,
        size: (width, height),
        anchor: [anchor_x, anchor_y],
        opacity,
        kind,
        target,
        blend_mode,
        premultiplied,
        rect,
        fill,
        stroke,
        clip,
        exact,
        level_of_detail,
        raster_scale,
        projection: [g, h, i],
    } = *fields;
    let projective = g != 0.0 || h != 0.0;
    debug_assert!(
        !projective || rect == [0.0; 4],
        "only a canvas is projective"
    );
    let [r0, r1, r2, r3] = if projective { [g, h, i, 0.0] } else { rect };
    let [f0, f1, f2, f3] = fill;
    let [s0, s1, s2, s3] = stroke;
    [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.tx,
        transform.ty,
        width,
        height,
        anchor_x,
        anchor_y,
        opacity,
        kind,
        target.width as f32,
        target.height as f32,
        blend_mode_index(blend_mode),
        f32::from(u8::from(premultiplied)),
        r0,
        r1,
        r2,
        r3,
        f0,
        f1,
        f2,
        f3,
        s0,
        s1,
        s2,
        s3,
        // The innermost clip the layer is drawn through (-1 without one),
        // how to filter (0 copies, 1 filters, 2 averages the footprint of a
        // projective layer), the mip level to filter at, and texels per
        // layer unit.
        clip.map_or(-1.0, |index| index as f32),
        if projective {
            2.0
        } else {
            f32::from(u8::from(!exact))
        },
        level_of_detail,
        raster_scale,
        target.x as f32,
        target.y as f32,
        0.0,
        0.0,
    ]
}

/// The mip level a layer placed by `transform` is filtered at: 0 when its
/// texels land one to one on pixels.
pub(crate) fn level_of_detail(transform: Affine, raster_scale: f32, exact: bool) -> f32 {
    if exact {
        0.0
    } else {
        transform.texels_per_pixel(raster_scale).max(1.0).log2()
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
        BlendMode::Darken => 6.0,
        BlendMode::Lighten => 7.0,
        BlendMode::ColorDodge => 8.0,
        BlendMode::ColorBurn => 9.0,
        BlendMode::HardLight => 10.0,
        BlendMode::SoftLight => 11.0,
        BlendMode::Exclusion => 12.0,
        BlendMode::Hue => 13.0,
        BlendMode::Saturation => 14.0,
        BlendMode::Color => 15.0,
        BlendMode::Luminosity => 16.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas_fields(projection: [f32; 3]) -> InstanceFields {
        InstanceFields {
            transform: Affine::IDENTITY,
            size: (10.0, 10.0),
            anchor: [0.0, 0.0],
            opacity: 1.0,
            kind: 0.0,
            target: CanvasRegion::scene(64, 64),
            blend_mode: BlendMode::Normal,
            premultiplied: true,
            rect: [0.0; 4],
            fill: [0.0; 4],
            stroke: [0.0; 4],
            clip: None,
            exact: false,
            level_of_detail: 0.0,
            raster_scale: 1.0,
            projection,
        }
    }

    #[test]
    fn a_plane_canvas_carries_its_projection_in_the_rect_slot() {
        assert_eq!(LAYER_INSTANCE_FLOATS, 36);
        let plane = layer_instance(&canvas_fields([0.25, -0.5, 2.0]));
        assert_eq!(plane[16..20], [0.25, -0.5, 2.0, 0.0]);
        assert_eq!(plane[29], 2.0, "averaged over each pixel's footprint");
        let flat = layer_instance(&canvas_fields(AFFINE));
        assert_eq!(flat[16..20], [0.0; 4]);
        assert_eq!(flat[29], 1.0);
    }
}
