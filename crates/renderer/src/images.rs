use crate::clip::{ParentState, clip_coverage};
use crate::composite::blend_with_mode;
use crate::paint::fill_rect;
use crate::types::{Color, RgbaFrame};
use celesta_composition::Point;

pub(crate) fn render_placeholder(
    frame: &mut RgbaFrame,
    anchor: Point,
    state: &ParentState,
    color: Color,
    width: f64,
    height: f64,
) {
    let width = (width * state.scale.x.abs()).round() as i32;
    let height = (height * state.scale.y.abs()).round() as i32;
    let left = (state.position.x - f64::from(width) * anchor.x).round() as i32;
    let top = (state.position.y - f64::from(height) * anchor.y).round() as i32;
    let clip = &state.clip;
    let mode = state.blend_mode;
    fill_rect(
        frame,
        left,
        top,
        width,
        height,
        color,
        state.opacity,
        mode,
        clip,
    );

    let border = Color::rgba(255, 255, 255, 180);
    fill_rect(
        frame,
        left,
        top,
        width,
        2,
        border,
        state.opacity,
        mode,
        clip,
    );
    fill_rect(
        frame,
        left,
        top + height - 2,
        width,
        2,
        border,
        state.opacity,
        mode,
        clip,
    );
    fill_rect(
        frame,
        left,
        top,
        2,
        height,
        border,
        state.opacity,
        mode,
        clip,
    );
    fill_rect(
        frame,
        left + width - 2,
        top,
        2,
        height,
        border,
        state.opacity,
        mode,
        clip,
    );
}

#[derive(Clone, Debug)]
pub(crate) struct DecodedImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

pub(crate) fn render_image(
    frame: &mut RgbaFrame,
    image: &DecodedImage,
    anchor: Point,
    state: &ParentState,
) {
    render_image_pixels(
        frame,
        image.width,
        image.height,
        &image.pixels,
        anchor,
        state,
    );
}

pub(crate) fn render_image_pixels(
    frame: &mut RgbaFrame,
    image_width: u32,
    image_height: u32,
    pixels: &[u8],
    anchor: Point,
    state: &ParentState,
) {
    let width = (f64::from(image_width) * state.scale.x.abs())
        .round()
        .max(1.0) as u32;
    let height = (f64::from(image_height) * state.scale.y.abs())
        .round()
        .max(1.0) as u32;
    let left = (state.position.x - f64::from(width) * anchor.x).round() as i32;
    let top = (state.position.y - f64::from(height) * anchor.y).round() as i32;
    for destination_y in 0..height {
        for destination_x in 0..width {
            let source_x = destination_x * image_width / width;
            let source_y = destination_y * image_height / height;
            let source_offset = ((source_y * image_width + source_x) * 4) as usize;
            let x = left + destination_x as i32;
            let y = top + destination_y as i32;
            if x < 0 || y < 0 || x >= frame.width as i32 || y >= frame.height as i32 {
                continue;
            }
            let coverage = clip_coverage(&state.clip, x, y);
            if coverage == 0.0 {
                continue;
            }
            let destination_offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
            blend_with_mode(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                Color::rgba(
                    pixels[source_offset],
                    pixels[source_offset + 1],
                    pixels[source_offset + 2],
                    pixels[source_offset + 3],
                ),
                state.opacity * coverage,
                state.blend_mode,
            );
        }
    }
}
