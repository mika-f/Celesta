use crate::PathTransform;
use crate::clip::ParentState;
use crate::error::RenderError;
use crate::images::render_tinted_image_pixels;
use crate::layer::render_output_pixels;
use crate::paint::ResolvedPaint;
use crate::rect::{RectPaint, rasterize_rect_transformed};
use crate::renderer::CpuRenderer;
use crate::types::{Color, RgbaFrame, parse_color};
use celesta_composition::{InstanceShape, Layer, LayerContent, Point, instance_size};

const CENTRE: Point = Point { x: 0.5, y: 0.5 };

impl CpuRenderer {
    /// Draws an `Instances` layer particle by particle, each as the `Rect`
    /// (or image) centred on it would draw, in `state`'s frame.
    pub(crate) fn render_instances(
        &mut self,
        frame: &mut RgbaFrame,
        layer: &Layer,
        state: &ParentState,
    ) -> Result<(), RenderError> {
        let LayerContent::Instances {
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
        } = &layer.content
        else {
            unreachable!("render_instances draws Instances layers");
        };
        let shared_color = match color {
            Some(color) => parse_color(color)?,
            None => Color::rgba(255, 255, 255, 255),
        };
        // Particle `i`'s own or the shared size, before an image fills in
        // a missing side.
        let given = |i: usize| {
            (
                widths
                    .as_ref()
                    .map_or(*width, |widths| Some(f64::from(widths[i]))),
                heights
                    .as_ref()
                    .map_or(*height, |heights| Some(f64::from(heights[i]))),
            )
        };
        let image = match shape.as_ref() {
            InstanceShape::Image { asset } => {
                let path = self.local_asset_path(asset)?;
                let decode_error = |source| RenderError::ImageDecode {
                    asset: asset.id.clone(),
                    source,
                };
                let natural = self
                    .image_sources
                    .natural_size(&asset.id, &path)
                    .map_err(decode_error)?;
                // An SVG is rasterized once, for the particle drawn largest
                // relative to its natural size, as an `Image` layer is for
                // its own size.
                let largest = (0..*count as usize)
                    .map(|i| {
                        let (w, h) = given(i);
                        let (w, h) = instance_size(w, h, Some(natural));
                        (w / natural.0).max(h / natural.1)
                    })
                    .filter(|scale| scale.is_finite())
                    .fold(1.0, f64::max);
                let density = state.scale.x.abs().max(state.scale.y.abs()) * largest;
                let display = self
                    .image_sources
                    .render(&asset.id, &path, None, None, None, None, density)
                    .map_err(decode_error)?;
                Some((display, natural))
            }
            InstanceShape::Rect { .. } | InstanceShape::Circle => None,
        };
        for i in 0..*count as usize {
            let (w, h) = given(i);
            let (particle_width, particle_height) =
                instance_size(w, h, image.as_ref().map(|(_, natural)| *natural));
            let degrees = rotation
                .as_ref()
                .map_or(0.0, |rotation| f64::from(rotation[i]));
            let (particle_x, particle_y) = (f64::from(x[i]), f64::from(y[i]));
            if ![
                particle_x,
                particle_y,
                particle_width,
                particle_height,
                degrees,
            ]
            .iter()
            .all(|value| value.is_finite())
            {
                continue;
            }
            if degrees != 0.0 {
                return Err(RenderError::UnsupportedRotation {
                    layer: layer.id.clone(),
                    degrees,
                });
            }
            let tint = colors.as_ref().map_or(
                [
                    shared_color.red,
                    shared_color.green,
                    shared_color.blue,
                    shared_color.alpha,
                ],
                |colors| {
                    [
                        colors[4 * i],
                        colors[4 * i + 1],
                        colors[4 * i + 2],
                        colors[4 * i + 3],
                    ]
                },
            );
            let particle = ParentState {
                position: Point {
                    x: state.position.x + particle_x * state.scale.x,
                    y: state.position.y + particle_y * state.scale.y,
                },
                ..state.clone()
            };
            if let Some((image, _)) = &image {
                let pixels = &image.pixels;
                let mut particle = particle;
                particle.scale.x *= particle_width / f64::from(pixels.width());
                particle.scale.y *= particle_height / f64::from(pixels.height());
                if particle.scale.x == 0.0 || particle.scale.y == 0.0 {
                    continue;
                }
                render_tinted_image_pixels(
                    frame,
                    pixels.width(),
                    pixels.height(),
                    pixels.as_raw(),
                    CENTRE,
                    &particle,
                    tint,
                );
                continue;
            }
            let corner_radius = match shape.as_ref() {
                InstanceShape::Rect { corner_radius } => *corner_radius,
                _ => particle_width.min(particle_height) / 2.0,
            };
            let paint = RectPaint {
                fill: Some(ResolvedPaint::Solid(Color::rgba(
                    tint[0], tint[1], tint[2], tint[3],
                ))),
                stroke: None,
            };
            let transform = PathTransform::scale_translate(
                particle.scale.x,
                particle.scale.y,
                particle.position.x - CENTRE.x * particle_width * particle.scale.x,
                particle.position.y - CENTRE.y * particle_height * particle.scale.y,
            );
            if let Some(rect) = rasterize_rect_transformed(
                particle_width,
                particle_height,
                corner_radius,
                &paint,
                transform,
                frame.width,
                frame.height,
            ) {
                render_output_pixels(frame, &rect, &particle);
            }
        }
        Ok(())
    }
}
