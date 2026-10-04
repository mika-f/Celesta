use crate::clip::{ClipNode, clip_coverage};
use crate::composite::blend_with_mode;
use crate::error::RenderError;
use crate::types::{Color, RgbaFrame};
use celesta_composition::{BlendMode, Paint};
use std::sync::Arc;

/// A [`Paint`] with its colors parsed, ready to be sampled per pixel.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedPaint {
    Solid(Color),
    Linear {
        start: (f64, f64),
        end: (f64, f64),
        stops: Vec<GradientStop>,
    },
    Radial {
        center: (f64, f64),
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

/// A parsed gradient stop with premultiplied `[r, g, b, a]` channels in 0..=255.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub(crate) offset: f64,
    pub(crate) premultiplied: [f64; 4],
}

impl GradientStop {
    /// Where the stop sits along the gradient.
    pub const fn offset(&self) -> f64 {
        self.offset
    }

    /// The stop's color, premultiplied, with channels in 0..=255.
    pub const fn premultiplied(&self) -> [f64; 4] {
        self.premultiplied
    }
}

impl ResolvedPaint {
    pub fn from_paint(paint: &Paint) -> Result<Self, RenderError> {
        Ok(match paint {
            Paint::Solid { color } => Self::Solid(Color::from_hex(color)?),
            Paint::Linear { start, end, stops } => Self::Linear {
                start: (start.x, start.y),
                end: (end.x, end.y),
                stops: resolve_stops(stops)?,
            },
            Paint::Radial {
                center,
                radius,
                stops,
            } => Self::Radial {
                center: (center.x, center.y),
                radius: *radius,
                stops: resolve_stops(stops)?,
            },
        })
    }

    /// The flat color, or `None` for a gradient.
    pub const fn solid(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            _ => None,
        }
    }

    /// Scales gradient geometry, for text rasterized at a device scale.
    pub(crate) fn scaled(mut self, scale: f64) -> Self {
        match &mut self {
            Self::Solid(_) => {}
            Self::Linear { start, end, .. } => {
                *start = (start.0 * scale, start.1 * scale);
                *end = (end.0 * scale, end.1 * scale);
            }
            Self::Radial { center, radius, .. } => {
                *center = (center.0 * scale, center.1 * scale);
                *radius *= scale;
            }
        }
        self
    }

    /// The color at a point in the painted layer's local pixels.
    pub fn color_at(&self, x: f64, y: f64) -> Color {
        match self {
            Self::Solid(color) => *color,
            Self::Linear { start, end, stops } => {
                let (dx, dy) = (end.0 - start.0, end.1 - start.1);
                let length_squared = dx * dx + dy * dy;
                let t = if length_squared > 0.0 {
                    ((x - start.0) * dx + (y - start.1) * dy) / length_squared
                } else {
                    0.0
                };
                sample_stops(stops, t)
            }
            Self::Radial {
                center,
                radius,
                stops,
            } => {
                let distance = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
                sample_stops(
                    stops,
                    if *radius > 0.0 {
                        distance / radius
                    } else {
                        0.0
                    },
                )
            }
        }
    }
}

pub(crate) fn resolve_stops(
    stops: &[celesta_composition::GradientStop],
) -> Result<Vec<GradientStop>, RenderError> {
    let mut resolved = stops
        .iter()
        .map(|stop| {
            let color = Color::from_hex(&stop.color)?;
            let alpha = f64::from(color.alpha) / 255.0;
            Ok(GradientStop {
                offset: stop.offset,
                premultiplied: [
                    f64::from(color.red) * alpha,
                    f64::from(color.green) * alpha,
                    f64::from(color.blue) * alpha,
                    f64::from(color.alpha),
                ],
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    // Stable, so stops sharing an offset keep their order (a hard edge).
    resolved.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    Ok(resolved)
}

/// Interpolates premultiplied, like CSS, so fading to `#00000000` doesn't
/// darken the color on the way.
pub(crate) fn sample_stops(stops: &[GradientStop], t: f64) -> Color {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return Color::TRANSPARENT;
    };
    let value = if t <= first.offset {
        first.premultiplied
    } else if t >= last.offset {
        last.premultiplied
    } else {
        let next = stops.iter().position(|stop| stop.offset > t).unwrap_or(0);
        let (from, to) = (stops[next - 1], stops[next]);
        let span = to.offset - from.offset;
        let f = if span > 0.0 {
            (t - from.offset) / span
        } else {
            1.0
        };
        std::array::from_fn(|i| {
            from.premultiplied[i] + (to.premultiplied[i] - from.premultiplied[i]) * f
        })
    };
    let alpha = value[3];
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let unpremultiply = |channel: f64| (channel * 255.0 / alpha).round().clamp(0.0, 255.0) as u8;
    Color::rgba(
        unpremultiply(value[0]),
        unpremultiply(value[1]),
        unpremultiply(value[2]),
        alpha.round().clamp(0.0, 255.0) as u8,
    )
}

pub(crate) fn resolve_paint(paint: Option<&Paint>) -> Result<Option<ResolvedPaint>, RenderError> {
    paint.map(ResolvedPaint::from_paint).transpose()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_rect(
    frame: &mut RgbaFrame,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    color: Color,
    opacity: f64,
    blend_mode: BlendMode,
    clip: &Option<Arc<ClipNode>>,
) {
    let right = (left + width).clamp(0, frame.width as i32);
    let bottom = (top + height).clamp(0, frame.height as i32);
    let left = left.clamp(0, frame.width as i32);
    let top = top.clamp(0, frame.height as i32);
    for y in top..bottom {
        for x in left..right {
            let coverage = clip_coverage(clip, x, y);
            if coverage == 0.0 {
                continue;
            }
            let offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
            blend_with_mode(
                &mut frame.pixels[offset..offset + 4],
                color,
                opacity * coverage,
                blend_mode,
            );
        }
    }
}
