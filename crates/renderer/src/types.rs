use crate::error::RenderError;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub use celesta_composition::Color;

/// `#RRGGBB` or `#RRGGBBAA`, through [`Color::from_hex`].
pub fn parse_color(value: &str) -> Result<Color, RenderError> {
    Color::from_hex(value).ok_or_else(|| RenderError::InvalidColor(value.to_owned()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    pub background: Color,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            background: Color::rgba(20, 22, 28, 255),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

impl RgbaFrame {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        let mut output = Vec::new();
        self.write_png_to(&mut output)?;
        Ok(output)
    }

    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), RenderError> {
        let file = File::create(path).map_err(RenderError::Io)?;
        let mut writer = BufWriter::new(file);
        self.write_png_to(&mut writer)?;
        writer.flush().map_err(RenderError::Io)
    }

    pub(crate) fn write_png_to(&self, output: impl Write) -> Result<(), RenderError> {
        let mut encoder = png::Encoder::new(output, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(RenderError::Png)?;
        writer
            .write_image_data(&self.pixels)
            .map_err(RenderError::Png)
    }
}
