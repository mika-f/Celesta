use crate::error::RenderError;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const WHITE: Self = Self::rgba(255, 255, 255, 255);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn from_hex(value: &str) -> Result<Self, RenderError> {
        let hex = value
            .strip_prefix('#')
            .ok_or_else(|| RenderError::InvalidColor(value.to_owned()))?;
        if !matches!(hex.len(), 6 | 8) {
            return Err(RenderError::InvalidColor(value.to_owned()));
        }
        let byte = |offset| {
            u8::from_str_radix(&hex[offset..offset + 2], 16)
                .map_err(|_| RenderError::InvalidColor(value.to_owned()))
        };
        Ok(Self::rgba(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            if hex.len() == 8 { byte(6)? } else { 255 },
        ))
    }
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
