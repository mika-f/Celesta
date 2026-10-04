use crate::clip::ParentState;
use crate::error::RenderError;
use crate::fonts::TextRasterizer;
use crate::types::{RenderOptions, RgbaFrame};
use crate::{image_source, psd_source};
use celesta_composition::Scene;
use celesta_media::VideoFrameDecoder;
use std::path::PathBuf;

pub struct CpuRenderer {
    pub(crate) options: RenderOptions,
    pub(crate) asset_root: PathBuf,
    pub(crate) text_rasterizer: TextRasterizer,
    pub(crate) psd_sources: psd_source::PsdSources,
    pub(crate) image_sources: image_source::ImageSources,
    pub(crate) video_decoder: Option<Box<dyn VideoFrameDecoder>>,
}

impl CpuRenderer {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            asset_root: PathBuf::from("."),
            text_rasterizer: TextRasterizer::new(),
            psd_sources: Default::default(),
            image_sources: Default::default(),
            video_decoder: None,
        }
    }

    pub fn with_asset_root(mut self, asset_root: impl Into<PathBuf>) -> Self {
        self.asset_root = asset_root.into();
        self
    }

    pub fn with_video_decoder(mut self, decoder: impl VideoFrameDecoder + 'static) -> Self {
        self.video_decoder = Some(Box::new(decoder));
        self
    }

    pub const fn options(&self) -> RenderOptions {
        self.options
    }

    pub fn render(&mut self, scene: &Scene) -> Result<RgbaFrame, RenderError> {
        self.text_rasterizer
            .load_fonts(&scene.fonts, &self.asset_root)?;
        let pixel_count = u64::from(scene.width)
            .checked_mul(u64::from(scene.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(RenderError::SurfaceTooLarge {
                width: scene.width,
                height: scene.height,
            })?;
        let mut frame = RgbaFrame {
            width: scene.width,
            height: scene.height,
            pixels: vec![0; pixel_count],
        };
        for pixel in frame.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[
                self.options.background.red,
                self.options.background.green,
                self.options.background.blue,
                self.options.background.alpha,
            ]);
        }

        for layer in &scene.layers {
            self.render_layer(&mut frame, layer, ParentState::default())?;
        }
        Ok(frame)
    }
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new(RenderOptions::default())
    }
}
