use celesta_renderer::{RectPaint, ResolvedPaint};

/// Bytes of `LayerInstance` in `layer.wgsl`: nine `vec4<f32>`s.
pub(crate) const LAYER_INSTANCE_SIZE: u64 = 9 * 4 * 4;

pub(crate) const LAYER_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    0 => Float32x4,
    1 => Float32x4,
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    8 => Float32x4,
];

pub(crate) fn instance_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Celesta layer instances"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A storage buffer for the frame's clips or paints.
pub(crate) fn clip_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Celesta layer clips"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(crate) fn clip_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    clips: &wgpu::Buffer,
    paints: &wgpu::Buffer,
    paths: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Celesta layer clip bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: clips.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: paints.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: paths.as_entire_binding(),
            },
        ],
    })
}

/// A rect drawn by `layer.wgsl`'s `rect_color`, which shades every output
/// pixel it covers as `celesta_renderer::rasterize_rect_transformed` does.
pub(crate) struct RectShape {
    pub(crate) half_width: f32,
    pub(crate) half_height: f32,
    pub(crate) radius: f32,
    /// 0 without a stroke.
    pub(crate) stroke_width: f32,
    /// See `encode_paint`.
    pub(crate) fill: [f32; 4],
    pub(crate) stroke: [f32; 4],
}

impl RectShape {
    /// The shape of a rect painted with `paint`, whose gradients are
    /// appended to `paints`.
    pub(crate) fn new(
        width: f64,
        height: f64,
        corner_radius: f64,
        paint: RectPaint,
        paints: &mut Vec<[f32; 4]>,
    ) -> Self {
        let RectPaint { fill, stroke } = paint;
        let (stroke, stroke_width) = match stroke {
            Some((stroke, width)) if width > 0.0 => (Some(stroke), width),
            _ => (None, 0.0),
        };
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        Self {
            half_width: half_width as f32,
            half_height: half_height as f32,
            radius: corner_radius.max(0.0).min(half_width.min(half_height)) as f32,
            stroke_width: stroke_width as f32,
            fill: encode_paint(fill.as_ref(), paints),
            stroke: encode_paint(stroke.as_ref(), paints),
        }
    }
}

/// A rect's fill or stroke as `layer.wgsl`'s `paint_color` reads it: a flat
/// color is its straight-alpha RGBA in 0-255 code values (transparent
/// without a paint); a gradient is `(index, 0, 0, -1)`, where `index` is
/// the first of its entries in `paints`:
///
/// - kind (1 linear, 2 radial), stop count, unused, unused
/// - linear: start x, start y, end x, end y; radial: center x, center y,
///   radius, unused (the rect's local pixels)
/// - per stop: offset, unused, unused, unused; then its premultiplied RGBA
///   in 0-255
///
/// Shading a gradient instead of rasterizing it keeps a gradient whose
/// colors change every frame as cheap as a flat one.
pub(crate) fn encode_paint(paint: Option<&ResolvedPaint>, paints: &mut Vec<[f32; 4]>) -> [f32; 4] {
    let index = paints.len() as f32;
    let (kind, geometry, stops) = match paint {
        None => return [0.0; 4],
        Some(ResolvedPaint::Solid(color)) => {
            return [color.red, color.green, color.blue, color.alpha].map(f32::from);
        }
        Some(ResolvedPaint::Linear { start, end, stops }) => {
            (1.0, [start.0, start.1, end.0, end.1], stops)
        }
        Some(ResolvedPaint::Radial {
            center,
            radius,
            stops,
        }) => (2.0, [center.0, center.1, *radius, 0.0], stops),
    };
    paints.push([kind, stops.len() as f32, 0.0, 0.0]);
    paints.push(geometry.map(|value| value as f32));
    for stop in stops {
        paints.push([stop.offset() as f32, 0.0, 0.0, 0.0]);
        paints.push(stop.premultiplied().map(|value| value as f32));
    }
    [index, 0.0, 0.0, -1.0]
}
