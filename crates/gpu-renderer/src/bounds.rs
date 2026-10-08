/// An axis-aligned rectangle in canvas pixels: `[left, top, right, bottom]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PixelBounds(pub(crate) [f32; 4]);

impl PixelBounds {
    pub(crate) fn union(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(Self(a)), Some(Self(b))) => Some(Self([
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ])),
            (a, None) => a,
            (None, b) => b,
        }
    }

    /// The part both cover; `None` when either is `None` or they do not
    /// overlap.
    pub(crate) fn intersection(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        let (Self(a), Self(b)) = (a?, b?);
        let bounds = [
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ];
        (bounds[2] > bounds[0] && bounds[3] > bounds[1]).then_some(Self(bounds))
    }

    pub(crate) fn expand(self, x: f32, y: f32) -> Self {
        let [left, top, right, bottom] = self.0;
        Self([left - x, top - y, right + x, bottom + y])
    }

    pub(crate) fn offset(self, [x, y]: [f32; 2]) -> Self {
        let [left, top, right, bottom] = self.0;
        Self([left + x, top + y, right + x, bottom + y])
    }

    /// The whole pixels the rectangle touches, clipped to `size`, as a
    /// scissor rect; `None` when nothing of it is on the canvas.
    pub(crate) fn scissor(self, size: wgpu::Extent3d) -> Option<[u32; 4]> {
        let [left, top, right, bottom] = self.0;
        let clamp = |value: f32, limit: u32| value.clamp(0.0, limit as f32);
        let left = clamp(left.floor(), size.width) as u32;
        let top = clamp(top.floor(), size.height) as u32;
        let right = clamp(right.ceil(), size.width) as u32;
        let bottom = clamp(bottom.ceil(), size.height) as u32;
        (right > left && bottom > top).then(|| [left, top, right - left, bottom - top])
    }
}

/// The part of the scene a canvas covers, in whole scene pixels: its
/// top-left corner and the size of its texture.
///
/// The root canvas covers the whole scene. An isolated group or an effect
/// draws onto a canvas that only covers what its layers can touch, so a
/// small glow composites through a small texture rather than several
/// scene-sized ones (each render pass loads and stores its whole target).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CanvasRegion {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl CanvasRegion {
    /// Group canvases are sized in steps of this many pixels, so a region
    /// that moves or grows a little from frame to frame reuses its texture.
    pub(crate) const STEP: u32 = 128;

    pub(crate) const fn scene(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// A canvas inside the `scene` that covers `bounds` (clipped to the
    /// scene), or `None` when nothing of `bounds` is on it. The canvas never
    /// reaches past the scene, so what is drawn onto it is exactly what a
    /// scene-sized canvas would hold there.
    pub(crate) fn covering(bounds: Option<PixelBounds>, scene: Self) -> Option<Self> {
        let [left, top, width, height] = bounds?.scissor(wgpu::Extent3d {
            width: scene.width,
            height: scene.height,
            depth_or_array_layers: 1,
        })?;
        let step = |extent: u32, limit: u32| (extent.div_ceil(Self::STEP) * Self::STEP).min(limit);
        let width = step(width, scene.width);
        let height = step(height, scene.height);
        Some(Self {
            x: left.min(scene.width - width),
            y: top.min(scene.height - height),
            width,
            height,
        })
    }

    /// `bounds`, in scene pixels, in this canvas's own pixels.
    pub(crate) fn local(self, bounds: PixelBounds) -> PixelBounds {
        bounds.offset([-(self.x as f32), -(self.y as f32)])
    }

    /// `bounds`, in scene pixels, in this canvas's pixels and clipped to
    /// it, as `[x, y, width, height]`; `None` when it misses the canvas.
    pub(crate) fn local_area(self, bounds: PixelBounds) -> Option<[u32; 4]> {
        self.local(bounds).scissor(self.extent())
    }

    pub(crate) const fn extent(self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }

    pub(crate) fn bounds(self) -> PixelBounds {
        PixelBounds([
            self.x as f32,
            self.y as f32,
            (self.x + self.width) as f32,
            (self.y + self.height) as f32,
        ])
    }
}

/// How far, in pixels, a blur of `radius` (the Gaussian's sigma in
/// `effect.wgsl`) can carry a pixel: its kernel's extent, plus one pixel for
/// the bilinear tap a fractional shadow offset reads.
pub(crate) fn blur_reach(radius: f32) -> f32 {
    let sigma = radius.clamp(0.0, 64.0);
    (sigma * 3.0).ceil() + 1.0
}
