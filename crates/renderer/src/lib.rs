//! Deterministic CPU reference renderer.
//!
//! This backend exists to fix scene semantics before the GPU renderer arrives.
//! Video and unresolved components remain diagnostic placeholders; images and text use
//! real decoders, font shaping, and glyph rasterization.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use cosmic_text::{
    Align, Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, SwashCache,
    Weight, Wrap,
};
use image::ImageReader;
use mikan_composition::{
    AssetLocation, Layer, LayerContent, MediaTiming, Paint, Point, ResolvedAsset, Scene, Stroke,
    TextAlign, TextStyle,
};
use mikan_media::{MediaError, VideoFrameDecoder};

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
    width: u32,
    height: u32,
    pixels: Vec<u8>,
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

    fn write_png_to(&self, output: impl Write) -> Result<(), RenderError> {
        let mut encoder = png::Encoder::new(output, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(RenderError::Png)?;
        writer
            .write_image_data(&self.pixels)
            .map_err(RenderError::Png)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterizedText {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RasterizedText {
    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

pub struct TextRasterizer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    loaded_fonts: HashSet<String>,
}

impl TextRasterizer {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        #[cfg(target_os = "windows")]
        load_directwrite_system_fonts(&mut font_system);

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            loaded_fonts: HashSet::new(),
        }
    }

    pub fn load_fonts(
        &mut self,
        fonts: &[ResolvedAsset],
        asset_root: &Path,
    ) -> Result<(), RenderError> {
        for font in fonts {
            if self.loaded_fonts.contains(&font.id) {
                continue;
            }
            let path = local_asset_path(asset_root, font)?;
            self.font_system
                .db_mut()
                .load_font_file(&path)
                .map_err(|source| RenderError::AssetIo {
                    asset: font.id.clone(),
                    source,
                })?;
            self.loaded_fonts.insert(font.id.clone());
        }
        Ok(())
    }

    pub fn rasterize(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        scale: f32,
    ) -> Result<RasterizedText, RenderError> {
        let scale = scale.abs();
        let font_size = style.font_size.unwrap_or(32.0) as f32 * scale;
        let line_height = style
            .line_height
            .map(|line_height| line_height as f32 * scale)
            .unwrap_or(font_size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        let width = max_width.map(|width| width as f32 * scale);
        buffer.set_size(&mut self.font_system, width, None);
        buffer.set_wrap(&mut self.font_system, Wrap::Word);

        let mut attrs = Attrs::new().weight(Weight(style.font_weight.unwrap_or(400)));
        if let Some(family) = style.font_family.as_deref() {
            attrs = attrs.family(Family::Name(family));
        }
        let alignment = style.align.map(|align| match align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        });
        buffer.set_text(
            &mut self.font_system,
            text,
            &attrs,
            Shaping::Advanced,
            alignment,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let measured_width = buffer
            .layout_runs()
            .fold(0.0_f32, |width, run| width.max(run.line_w));
        let measured_height = buffer.layout_runs().fold(0.0_f32, |height, run| {
            height.max(run.line_top + run.line_height)
        });
        let mask_width = width.unwrap_or(measured_width).ceil().max(1.0) as u32;
        let mask_height = measured_height.ceil().max(1.0) as u32;
        let fill = paint_color(style.fill.as_ref())?.unwrap_or(Color::WHITE);
        // A coverage-only alpha mask, used for the stroke's dilation below —
        // meaningful for both ordinary glyphs and color glyphs (an emoji's
        // silhouette dilates the same way plain text would).
        let mut mask = vec![0_u8; mask_width as usize * mask_height as usize];
        // The actual per-pixel glyph color. cosmic-text/swash already decode
        // color glyphs (COLR, sbix, CBDT/CBLC — how system emoji fonts like
        // Apple Color Emoji store their glyphs) into real per-pixel RGBA
        // here, not just a coverage mask; passing `fill` as the base color
        // below means an ordinary (non-color) glyph's pixels come back as
        // `fill` scaled by coverage, so accumulating directly into this
        // buffer reproduces flat-fill text exactly while also capturing
        // multi-color emoji glyphs, which a single mask+solid-fill
        // composite cannot represent.
        let mut glyph_pixels = vec![0_u8; mask_width as usize * mask_height as usize * 4];
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgba(fill.red, fill.green, fill.blue, fill.alpha),
            |x, y, width, height, color| {
                for offset_y in 0..height as i32 {
                    for offset_x in 0..width as i32 {
                        let pixel_x = x + offset_x;
                        let pixel_y = y + offset_y;
                        if pixel_x < 0
                            || pixel_y < 0
                            || pixel_x >= mask_width as i32
                            || pixel_y >= mask_height as i32
                        {
                            continue;
                        }
                        let offset = pixel_y as usize * mask_width as usize + pixel_x as usize;
                        mask[offset] = mask[offset].max(color.a());
                        let pixel_offset = offset * 4;
                        blend(
                            &mut glyph_pixels[pixel_offset..pixel_offset + 4],
                            Color::rgba(color.r(), color.g(), color.b(), color.a()),
                            1.0,
                        );
                    }
                }
            },
        );

        let pixel_count = mask_width as usize * mask_height as usize * 4;
        let mut frame = RgbaFrame {
            width: mask_width,
            height: mask_height,
            pixels: vec![0; pixel_count],
        };
        if let Some(stroke) = &style.stroke {
            let radius = (stroke.width * f64::from(scale)).round().max(0.0) as u32;
            if radius > 0 {
                let stroke_mask = dilate_mask(&mask, mask_width, mask_height, radius);
                let stroke_color = paint_color(Some(&stroke.paint))?.unwrap_or(Color::WHITE);
                composite_mask(
                    &mut frame,
                    &stroke_mask,
                    mask_width,
                    mask_height,
                    0,
                    0,
                    stroke_color,
                    1.0,
                );
            }
        }
        composite_rgba(&mut frame, &glyph_pixels, mask_width, mask_height, 0, 0);
        if !text.contains('\n') {
            frame = trim_transparent_edges(frame, max_width.is_none());
        }
        Ok(RasterizedText {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        })
    }
}

#[cfg(target_os = "windows")]
fn load_directwrite_system_fonts(font_system: &mut FontSystem) {
    use cosmic_text::fontdb::Source;

    let mut loaded_paths = font_system
        .db()
        .faces()
        .filter_map(|face| match &face.source {
            Source::File(path) | Source::SharedFile(path, _) => Some(path.clone()),
            Source::Binary(_) => None,
        })
        .collect::<HashSet<_>>();

    for family in dwrote::FontCollection::get_system(true).families_iter() {
        for index in 0..family.get_font_count() {
            let Ok(font) = family.font(index) else {
                continue;
            };
            let Ok(files) = font.create_font_face().files() else {
                continue;
            };

            for file in files {
                if let Ok(path) = file.font_file_path() {
                    if loaded_paths.insert(path.clone()) {
                        let _ = font_system.db_mut().load_font_file(path);
                    }
                } else if let Ok(bytes) = file.font_file_bytes() {
                    font_system.db_mut().load_font_data(bytes);
                }
            }
        }
    }

    // Adobe keeps synced fonts outside DirectWrite's system collection. Validate
    // its extensionless cache files with DirectWrite before adding them to fontdb.
    let Some(app_data) = std::env::var_os("APPDATA") else {
        return;
    };
    let directory = PathBuf::from(app_data).join("Adobe/CoreSync/plugins/livetype/r");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if loaded_paths.insert(path.clone()) && dwrote::FontFile::new_from_path(&path).is_some() {
            let _ = font_system.db_mut().load_font_file(path);
        }
    }
}

fn trim_transparent_edges(frame: RgbaFrame, trim_horizontal: bool) -> RgbaFrame {
    let mut min_x = frame.width;
    let mut min_y = frame.height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut has_visible_pixel = false;
    for (index, pixel) in frame.pixels.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        has_visible_pixel = true;
        let x = index as u32 % frame.width;
        let y = index as u32 / frame.width;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !has_visible_pixel {
        return frame;
    }
    if !trim_horizontal {
        min_x = 0;
        max_x = frame.width - 1;
    }
    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;
    let pad_left = u32::from(trim_horizontal && width % 2 == 1);
    let pad_top = u32::from(height % 2 == 1);
    if width == frame.width && height == frame.height && pad_left == 0 && pad_top == 0 {
        return frame;
    }
    let output_width = width + pad_left;
    let output_height = height + pad_top;
    let mut pixels = Vec::with_capacity((output_width * output_height * 4) as usize);
    pixels.resize((output_width * pad_top * 4) as usize, 0);
    for y in min_y..=max_y {
        pixels.resize(pixels.len() + (pad_left * 4) as usize, 0);
        let start = ((y * frame.width + min_x) * 4) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&frame.pixels[start..end]);
    }
    RgbaFrame {
        width: output_width,
        height: output_height,
        pixels,
    }
}

impl Default for TextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CpuRenderer {
    options: RenderOptions,
    asset_root: PathBuf,
    text_rasterizer: TextRasterizer,
    images: HashMap<String, DecodedImage>,
    video_decoder: Option<Box<dyn VideoFrameDecoder>>,
}

impl CpuRenderer {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            asset_root: PathBuf::from("."),
            text_rasterizer: TextRasterizer::new(),
            images: HashMap::new(),
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

    fn render_layer(
        &mut self,
        frame: &mut RgbaFrame,
        layer: &Layer,
        parent: ParentState,
    ) -> Result<(), RenderError> {
        if layer.transform.rotation != 0.0 {
            return Err(RenderError::UnsupportedRotation {
                layer: layer.id.clone(),
                degrees: layer.transform.rotation,
            });
        }
        let state = ParentState {
            position: Point {
                x: parent.position.x + layer.transform.position.x * parent.scale.x,
                y: parent.position.y + layer.transform.position.y * parent.scale.y,
            },
            scale: Point {
                x: parent.scale.x * layer.transform.scale.x,
                y: parent.scale.y * layer.transform.scale.y,
            },
            opacity: (parent.opacity * layer.opacity).clamp(0.0, 1.0),
        };
        if state.opacity == 0.0 || state.scale.x == 0.0 || state.scale.y == 0.0 {
            return Ok(());
        }

        match &layer.content {
            LayerContent::Text {
                text,
                style,
                max_width,
            } => self.render_text(
                frame,
                text,
                style,
                *max_width,
                layer.transform.anchor,
                state,
            )?,
            LayerContent::Group { layers } => {
                for child in layers {
                    self.render_layer(frame, child, state)?;
                }
            }
            LayerContent::Video { asset, timing } => {
                if self.video_decoder.is_some() {
                    let image = self.decode_video_frame(&layer.id, asset, timing)?;
                    render_image(frame, &image, layer.transform.anchor, state);
                } else {
                    render_placeholder(
                        frame,
                        layer.transform.anchor,
                        state,
                        Color::rgba(54, 98, 176, 255),
                        320.0,
                        180.0,
                    );
                }
            }
            LayerContent::Image { asset } => {
                let image = self.load_image(asset)?;
                render_image(frame, image, layer.transform.anchor, state);
            }
            LayerContent::Psd {
                asset,
                visible_layers,
                enabled_layers,
                disabled_layers,
            } => {
                let image =
                    self.load_psd(asset, visible_layers, enabled_layers, disabled_layers)?;
                render_image(frame, image, layer.transform.anchor, state);
            }
            LayerContent::Rect {
                width,
                height,
                fill,
                stroke,
                corner_radius,
            } => {
                let image = rasterize_rect(
                    *width,
                    *height,
                    *corner_radius,
                    fill.as_ref(),
                    stroke.as_ref(),
                )?;
                render_image(
                    frame,
                    &DecodedImage {
                        width: image.width(),
                        height: image.height(),
                        pixels: image.into_pixels(),
                    },
                    layer.transform.anchor,
                    state,
                );
            }
            LayerContent::MissingComponent { .. } => render_placeholder(
                frame,
                layer.transform.anchor,
                state,
                Color::rgba(220, 125, 42, 255),
                240.0,
                120.0,
            ),
        }
        Ok(())
    }

    fn render_text(
        &mut self,
        frame: &mut RgbaFrame,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        anchor: Point,
        state: ParentState,
    ) -> Result<(), RenderError> {
        if (state.scale.x.abs() - state.scale.y.abs()).abs() > f64::EPSILON {
            return Err(RenderError::UnsupportedNonUniformTextScale {
                x: state.scale.x,
                y: state.scale.y,
            });
        }
        let scale = state.scale.y.abs() as f32;
        let text = self
            .text_rasterizer
            .rasterize(text, style, max_width, scale)?;
        let image = DecodedImage {
            width: text.width,
            height: text.height,
            pixels: text.pixels,
        };
        render_image(
            frame,
            &image,
            anchor,
            ParentState {
                position: state.position,
                scale: Point { x: 1.0, y: 1.0 },
                opacity: state.opacity,
            },
        );
        Ok(())
    }

    fn load_image(&mut self, asset: &ResolvedAsset) -> Result<&DecodedImage, RenderError> {
        if !self.images.contains_key(&asset.id) {
            let path = self.local_asset_path(asset)?;
            let image = ImageReader::open(&path)
                .map_err(|source| RenderError::AssetIo {
                    asset: asset.id.clone(),
                    source,
                })?
                .decode()
                .map_err(|source| RenderError::ImageDecode {
                    asset: asset.id.clone(),
                    source,
                })?
                .to_rgba8();
            self.images.insert(
                asset.id.clone(),
                DecodedImage {
                    width: image.width(),
                    height: image.height(),
                    pixels: image.into_raw(),
                },
            );
        }
        Ok(self.images.get(&asset.id).expect("image was cached"))
    }

    fn load_psd(
        &mut self,
        asset: &ResolvedAsset,
        visible_layers: &[String],
        enabled_layers: &[String],
        disabled_layers: &[String],
    ) -> Result<&DecodedImage, RenderError> {
        let key = psd_cache_key(asset, visible_layers, enabled_layers, disabled_layers);
        if !self.images.contains_key(&key) {
            let path = self.local_asset_path(asset)?;
            let image = rasterize_psd(
                &asset.id,
                &path,
                visible_layers,
                enabled_layers,
                disabled_layers,
            )?;
            self.images.insert(
                key.clone(),
                DecodedImage {
                    width: image.width,
                    height: image.height,
                    pixels: image.pixels,
                },
            );
        }
        Ok(self.images.get(&key).expect("PSD image was cached"))
    }

    fn decode_video_frame(
        &mut self,
        request_id: &str,
        asset: &ResolvedAsset,
        timing: &MediaTiming,
    ) -> Result<DecodedImage, RenderError> {
        let path = self.local_asset_path(asset)?;
        let frame = self
            .video_decoder
            .as_mut()
            .expect("video decoder presence was checked")
            .decode_frame_for(request_id, &path, timing.source_time_seconds)?;
        Ok(DecodedImage {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        })
    }

    fn local_asset_path(&self, asset: &ResolvedAsset) -> Result<PathBuf, RenderError> {
        local_asset_path(&self.asset_root, asset)
    }
}

fn local_asset_path(asset_root: &Path, asset: &ResolvedAsset) -> Result<PathBuf, RenderError> {
    match &asset.location {
        AssetLocation::File { path } => {
            let path = Path::new(path);
            Ok(if path.is_absolute() {
                path.to_owned()
            } else {
                asset_root.join(path)
            })
        }
        AssetLocation::Url { url } => Err(RenderError::RemoteAsset {
            asset: asset.id.clone(),
            url: url.clone(),
        }),
    }
}

fn psd_cache_key(
    asset: &ResolvedAsset,
    visible_layers: &[String],
    enabled_layers: &[String],
    disabled_layers: &[String],
) -> String {
    format!(
        "psd\0{}\0{}\0{}\0{}",
        asset.id,
        visible_layers.join("\0"),
        enabled_layers.join("\0"),
        disabled_layers.join("\0")
    )
}

/// Rasterizes a PSD portrait into a full-canvas RGBA frame.
///
/// Layer visibility is resolved as: `disabled_layers` always hide, then
/// `enabled_layers` always show (the current lip-sync mouth), then — when
/// `visible_layers` is non-empty — exactly the listed layer paths compose
/// (a portrait preset; the PSD's own saved visibility is ignored), otherwise
/// the PSD's saved per-layer/-folder visibility drives the composite. Each
/// layer is placed at its real PSD coordinates (via [`psd::PsdLayer::rgba`],
/// which returns canvas-sized pixels). Per-layer opacity is applied; group
/// opacity is not (the `psd` crate misreads it as 0 for real PSDTool files).
pub fn rasterize_psd(
    asset: &str,
    path: &Path,
    visible_layers: &[String],
    enabled_layers: &[String],
    disabled_layers: &[String],
) -> Result<RgbaFrame, RenderError> {
    let bytes = fs::read(path).map_err(|source| RenderError::AssetIo {
        asset: asset.to_owned(),
        source,
    })?;
    let psd = psd::Psd::from_bytes(&bytes).map_err(|source| RenderError::PsdDecode {
        asset: asset.to_owned(),
        source,
    })?;
    let visible: HashSet<String> = visible_layers
        .iter()
        .map(|path| normalize_psd_path(path))
        .collect();
    let enabled: HashSet<String> = enabled_layers
        .iter()
        .map(|path| normalize_psd_path(path))
        .collect();
    let disabled: HashSet<String> = disabled_layers
        .iter()
        .map(|path| normalize_psd_path(path))
        .collect();
    let layer_paths: Vec<String> = psd
        .layers()
        .iter()
        .map(|layer| psd_layer_path(&psd, layer))
        .collect();
    // `enabled`/`disabled` come straight from the character's lip-sync
    // configuration, so a typo there should surface rather than silently do
    // nothing. `visible_layers` is a preset that may target a slightly
    // different build of the PSD, so unknown entries there are ignored.
    for requested in enabled.iter().chain(disabled.iter()) {
        if !layer_paths.iter().any(|path| path == requested) {
            return Err(RenderError::MissingPsdLayer {
                asset: asset.to_owned(),
                layer: requested.clone(),
            });
        }
    }
    let use_preset = !visible.is_empty();

    let mut pixels = vec![0; psd.width() as usize * psd.height() as usize * 4];
    for (layer, path) in psd.layers().iter().zip(layer_paths).rev() {
        let shown = if disabled.contains(&path) {
            false
        } else if enabled.contains(&path) {
            true
        } else if use_preset {
            visible.contains(&path)
        } else {
            layer.visible() && psd_ancestors_visible(&psd, layer.parent_id())
        };
        if !shown {
            continue;
        }
        // Group opacity is deliberately not applied: the `psd` crate reads it
        // from the wrong ("bounding section") record and reports 0 for every
        // folder in real PSDTool files. Per-layer opacity is read correctly.
        let opacity = f64::from(layer.opacity()) / 255.0;
        for (destination, source) in pixels.chunks_exact_mut(4).zip(layer.rgba().chunks_exact(4)) {
            blend(
                destination,
                Color::rgba(source[0], source[1], source[2], source[3]),
                opacity,
            );
        }
    }
    Ok(RgbaFrame {
        width: psd.width(),
        height: psd.height(),
        pixels,
    })
}

fn normalize_psd_path(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn psd_layer_path(psd: &psd::Psd, layer: &psd::PsdLayer) -> String {
    let mut names = vec![layer.name()];
    let mut parent = layer.parent_id();
    let mut visited = HashSet::new();
    while let Some(id) = parent {
        if !visited.insert(id) {
            break;
        }
        let Some(group) = psd.groups().get(&id) else {
            break;
        };
        names.push(group.name());
        parent = group.parent_id();
    }
    names.reverse();
    names.join("/")
}

fn psd_ancestors_visible(psd: &psd::Psd, mut parent: Option<u32>) -> bool {
    let mut visited = HashSet::new();
    while let Some(id) = parent {
        if !visited.insert(id) {
            break;
        }
        let Some(group) = psd.groups().get(&id) else {
            break;
        };
        if !group.visible() {
            return false;
        }
        parent = group.parent_id();
    }
    true
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new(RenderOptions::default())
    }
}

#[derive(Clone, Copy, Debug)]
struct ParentState {
    position: Point,
    scale: Point,
    opacity: f64,
}

impl Default for ParentState {
    fn default() -> Self {
        Self {
            position: Point { x: 0.0, y: 0.0 },
            scale: Point { x: 1.0, y: 1.0 },
            opacity: 1.0,
        }
    }
}

fn render_placeholder(
    frame: &mut RgbaFrame,
    anchor: Point,
    state: ParentState,
    color: Color,
    width: f64,
    height: f64,
) {
    let width = (width * state.scale.x.abs()).round() as i32;
    let height = (height * state.scale.y.abs()).round() as i32;
    let left = (state.position.x - f64::from(width) * anchor.x).round() as i32;
    let top = (state.position.y - f64::from(height) * anchor.y).round() as i32;
    fill_rect(frame, left, top, width, height, color, state.opacity);

    let border = Color::rgba(255, 255, 255, 180);
    fill_rect(frame, left, top, width, 2, border, state.opacity);
    fill_rect(
        frame,
        left,
        top + height - 2,
        width,
        2,
        border,
        state.opacity,
    );
    fill_rect(frame, left, top, 2, height, border, state.opacity);
    fill_rect(
        frame,
        left + width - 2,
        top,
        2,
        height,
        border,
        state.opacity,
    );
}

#[derive(Clone, Debug)]
struct DecodedImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn render_image(frame: &mut RgbaFrame, image: &DecodedImage, anchor: Point, state: ParentState) {
    let width = (f64::from(image.width) * state.scale.x.abs())
        .round()
        .max(1.0) as u32;
    let height = (f64::from(image.height) * state.scale.y.abs())
        .round()
        .max(1.0) as u32;
    let left = (state.position.x - f64::from(width) * anchor.x).round() as i32;
    let top = (state.position.y - f64::from(height) * anchor.y).round() as i32;
    for destination_y in 0..height {
        for destination_x in 0..width {
            let source_x = destination_x * image.width / width;
            let source_y = destination_y * image.height / height;
            let source_offset = ((source_y * image.width + source_x) * 4) as usize;
            let x = left + destination_x as i32;
            let y = top + destination_y as i32;
            if x < 0 || y < 0 || x >= frame.width as i32 || y >= frame.height as i32 {
                continue;
            }
            let destination_offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
            blend(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                Color::rgba(
                    image.pixels[source_offset],
                    image.pixels[source_offset + 1],
                    image.pixels[source_offset + 2],
                    image.pixels[source_offset + 3],
                ),
                state.opacity,
            );
        }
    }
}

/// Rasterizes a flat-shaded, optionally rounded and stroked rectangle into an
/// RGBA buffer, anti-aliased by signed distance. Shares `RasterizedText`'s
/// shape (width/height/pixels) so it composites through the exact same
/// `render_image` path text does. Takes the raw `Paint`/`Stroke` composition
/// types (like `TextRasterizer::rasterize` takes `&TextStyle`) so callers,
/// including `mikan-gpu-renderer`, never need their own color parsing.
pub fn rasterize_rect(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<&Paint>,
    stroke: Option<&Stroke>,
) -> Result<RasterizedText, RenderError> {
    let fill = paint_color(fill)?;
    let stroke = stroke
        .map(|stroke| -> Result<(Color, f64), RenderError> {
            let Paint::Solid { color } = &stroke.paint;
            Ok((Color::from_hex(color)?, stroke.width))
        })
        .transpose()?;
    Ok(rasterize_rect_pixels(
        width,
        height,
        corner_radius,
        fill,
        stroke,
    ))
}

fn rasterize_rect_pixels(
    width: f64,
    height: f64,
    corner_radius: f64,
    fill: Option<Color>,
    stroke: Option<(Color, f64)>,
) -> RasterizedText {
    let pixel_width = width.max(0.0).ceil().max(1.0) as u32;
    let pixel_height = height.max(0.0).ceil().max(1.0) as u32;
    let mut pixels = vec![0_u8; pixel_width as usize * pixel_height as usize * 4];

    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let radius = corner_radius.max(0.0).min(half_width.min(half_height));
    let stroke = stroke.filter(|(_, stroke_width)| *stroke_width > 0.0);

    for y in 0..pixel_height {
        for x in 0..pixel_width {
            let px = x as f64 + 0.5 - half_width;
            let py = y as f64 + 0.5 - half_height;
            let outer_distance =
                signed_distance_rounded_box(px, py, half_width, half_height, radius);
            let outer_alpha = (0.5 - outer_distance).clamp(0.0, 1.0);
            if outer_alpha <= 0.0 {
                continue;
            }

            let mut color = fill.unwrap_or(Color::TRANSPARENT);
            if let Some((stroke_color, stroke_width)) = stroke {
                let inner_half_width = (half_width - stroke_width).max(0.0);
                let inner_half_height = (half_height - stroke_width).max(0.0);
                let inner_radius = (radius - stroke_width).max(0.0);
                let inner_distance = signed_distance_rounded_box(
                    px,
                    py,
                    inner_half_width,
                    inner_half_height,
                    inner_radius,
                );
                let inner_alpha = (0.5 - inner_distance).clamp(0.0, 1.0);
                color = Color::rgba(
                    lerp(stroke_color.red, color.red, inner_alpha),
                    lerp(stroke_color.green, color.green, inner_alpha),
                    lerp(stroke_color.blue, color.blue, inner_alpha),
                    lerp(stroke_color.alpha, color.alpha, inner_alpha),
                );
            }

            let offset = (y * pixel_width + x) as usize * 4;
            pixels[offset] = color.red;
            pixels[offset + 1] = color.green;
            pixels[offset + 2] = color.blue;
            pixels[offset + 3] = (f64::from(color.alpha) * outer_alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }

    RasterizedText {
        width: pixel_width,
        height: pixel_height,
        pixels,
    }
}

/// Inigo Quilez's rounded-box signed distance function: negative inside the
/// shape, zero at the edge, positive outside, in the same pixel units as
/// `half_width`/`half_height`/`radius`.
fn signed_distance_rounded_box(
    px: f64,
    py: f64,
    half_width: f64,
    half_height: f64,
    radius: f64,
) -> f64 {
    let qx = px.abs() - half_width + radius;
    let qy = py.abs() - half_height + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn dilate_mask(mask: &[u8], width: u32, height: u32, radius: u32) -> Vec<u8> {
    let mut output = vec![0; mask.len()];
    let radius = radius as i32;
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let alpha = mask[y as usize * width as usize + x as usize];
            if alpha == 0 {
                continue;
            }
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    let target_x = x + offset_x;
                    let target_y = y + offset_y;
                    if target_x < 0
                        || target_y < 0
                        || target_x >= width as i32
                        || target_y >= height as i32
                    {
                        continue;
                    }
                    let offset = target_y as usize * width as usize + target_x as usize;
                    output[offset] = output[offset].max(alpha);
                }
            }
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn composite_mask(
    frame: &mut RgbaFrame,
    mask: &[u8],
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    color: Color,
    opacity: f64,
) {
    for y in 0..height {
        for x in 0..width {
            let destination_x = left + x as i32;
            let destination_y = top + y as i32;
            if destination_x < 0
                || destination_y < 0
                || destination_x >= frame.width as i32
                || destination_y >= frame.height as i32
            {
                continue;
            }
            let alpha = mask[(y * width + x) as usize];
            if alpha == 0 {
                continue;
            }
            let destination_offset =
                ((destination_y as u32 * frame.width + destination_x as u32) * 4) as usize;
            blend(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                color,
                opacity * f64::from(alpha) / 255.0,
            );
        }
    }
}

/// Like `composite_mask`, but `source` already carries its own per-pixel RGBA
/// (used for glyph rendering, where a color emoji glyph's pixels vary in hue
/// across the glyph, not just coverage).
fn composite_rgba(
    frame: &mut RgbaFrame,
    source: &[u8],
    width: u32,
    height: u32,
    left: i32,
    top: i32,
) {
    for y in 0..height {
        for x in 0..width {
            let destination_x = left + x as i32;
            let destination_y = top + y as i32;
            if destination_x < 0
                || destination_y < 0
                || destination_x >= frame.width as i32
                || destination_y >= frame.height as i32
            {
                continue;
            }
            let source_offset = ((y * width + x) * 4) as usize;
            let alpha = source[source_offset + 3];
            if alpha == 0 {
                continue;
            }
            let destination_offset =
                ((destination_y as u32 * frame.width + destination_x as u32) * 4) as usize;
            let color = Color::rgba(
                source[source_offset],
                source[source_offset + 1],
                source[source_offset + 2],
                alpha,
            );
            blend(
                &mut frame.pixels[destination_offset..destination_offset + 4],
                color,
                1.0,
            );
        }
    }
}

fn paint_color(paint: Option<&Paint>) -> Result<Option<Color>, RenderError> {
    paint
        .map(|paint| match paint {
            Paint::Solid { color } => Color::from_hex(color),
        })
        .transpose()
}

fn fill_rect(
    frame: &mut RgbaFrame,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    color: Color,
    opacity: f64,
) {
    let right = (left + width).clamp(0, frame.width as i32);
    let bottom = (top + height).clamp(0, frame.height as i32);
    let left = left.clamp(0, frame.width as i32);
    let top = top.clamp(0, frame.height as i32);
    for y in top..bottom {
        for x in left..right {
            let offset = ((y as u32 * frame.width + x as u32) * 4) as usize;
            blend(&mut frame.pixels[offset..offset + 4], color, opacity);
        }
    }
}

fn blend(destination: &mut [u8], source: Color, opacity: f64) {
    let source_alpha = (f64::from(source.alpha) / 255.0) * opacity.clamp(0.0, 1.0);
    let destination_alpha = f64::from(destination[3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha == 0.0 {
        destination.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let source_value = f64::from([source.red, source.green, source.blue][channel]);
        let destination_value = f64::from(destination[channel]);
        let output = (source_value * source_alpha
            + destination_value * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = output.round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

#[derive(Debug)]
pub enum RenderError {
    SurfaceTooLarge {
        width: u32,
        height: u32,
    },
    InvalidColor(String),
    UnsupportedRotation {
        layer: String,
        degrees: f64,
    },
    UnsupportedNonUniformTextScale {
        x: f64,
        y: f64,
    },
    RemoteAsset {
        asset: String,
        url: String,
    },
    AssetIo {
        asset: String,
        source: io::Error,
    },
    ImageDecode {
        asset: String,
        source: image::ImageError,
    },
    PsdDecode {
        asset: String,
        source: psd::PsdError,
    },
    MissingPsdLayer {
        asset: String,
        layer: String,
    },
    Media(MediaError),
    Time(mikan_composition::TimeError),
    Io(io::Error),
    Png(png::EncodingError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceTooLarge { width, height } => {
                write!(formatter, "surface {width}x{height} is too large")
            }
            Self::InvalidColor(color) => write!(formatter, "invalid color `{color}`"),
            Self::UnsupportedRotation { layer, degrees } => write!(
                formatter,
                "CPU reference renderer does not support {degrees} degree rotation on layer `{layer}`"
            ),
            Self::UnsupportedNonUniformTextScale { x, y } => write!(
                formatter,
                "CPU reference renderer does not support non-uniform text scale ({x}, {y})"
            ),
            Self::RemoteAsset { asset, url } => {
                write!(
                    formatter,
                    "remote asset `{asset}` is not available locally: {url}"
                )
            }
            Self::AssetIo { asset, source } => {
                write!(formatter, "could not read asset `{asset}`: {source}")
            }
            Self::ImageDecode { asset, source } => {
                write!(
                    formatter,
                    "could not decode image asset `{asset}`: {source}"
                )
            }
            Self::PsdDecode { asset, source } => {
                write!(formatter, "could not decode PSD asset `{asset}`: {source}")
            }
            Self::MissingPsdLayer { asset, layer } => {
                write!(formatter, "PSD asset `{asset}` has no layer `{layer}`")
            }
            Self::Media(error) => write!(formatter, "could not decode video frame: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate video time: {error}"),
            Self::Io(error) => write!(formatter, "image I/O failed: {error}"),
            Self::Png(error) => write!(formatter, "PNG encoding failed: {error}"),
        }
    }
}

impl Error for RenderError {}

impl From<MediaError> for RenderError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl From<mikan_composition::TimeError> for RenderError {
    fn from(error: mikan_composition::TimeError) -> Self {
        Self::Time(error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mikan_composition::{
        AssetLocation, EvaluatedTransform, Layer, LayerContent, MediaTiming, Rational,
        ResolvedAsset, Scene, TextStyle, Time,
    };
    use mikan_media::{MediaError, VideoFrame, VideoFrameDecoder};

    use super::*;

    #[test]
    fn rasterizes_color_emoji_glyphs_when_a_color_font_is_available() {
        let mut rasterizer = TextRasterizer::new();
        let rasterized = rasterizer
            .rasterize(
                "\u{1F525}", // fire emoji: multi-colored (red/orange/yellow) on
                // any real color-emoji font, unlike a flat single-fill glyph.
                &TextStyle {
                    font_size: Some(64.0),
                    fill: Some(Paint::Solid {
                        color: "#FFFFFFFF".to_owned(),
                    }),
                    ..TextStyle::default()
                },
                None,
                1.0,
            )
            .unwrap();

        let distinct_colors: std::collections::HashSet<[u8; 3]> = rasterized
            .pixels()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();

        // A regression back to alpha-only mask compositing would flatten
        // every opaque pixel to the single fill color (white here), so this
        // is the direct check that color glyph data survives rasterization.
        // Environments without any color-emoji font (uncommon, but possible
        // outside macOS) fall back to a monochrome glyph outline instead of
        // failing — not a regression this test can detect there.
        if distinct_colors.len() <= 1 {
            eprintln!(
                "skipping color emoji assertion: no color-emoji font available in this environment"
            );
            return;
        }
        assert!(distinct_colors.len() > 1);
    }

    #[test]
    fn renders_text_to_a_png() {
        let scene = Scene {
            width: 320,
            height: 180,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "hello".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 160.0, y: 90.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                content: LayerContent::Text {
                    text: "Hello, Mikan!".to_owned(),
                    style: TextStyle {
                        font_size: Some(24.0),
                        ..TextStyle::default()
                    },
                    max_width: None,
                },
            }],
        };
        let mut renderer = CpuRenderer::default();
        let frame = renderer.render(&scene).unwrap();
        let png = frame.encode_png().unwrap();

        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(
            frame
                .pixels()
                .chunks_exact(4)
                .any(|pixel| pixel == [255, 255, 255, 255])
        );
    }

    #[test]
    fn renders_a_filled_rounded_rect_with_a_stroke() {
        let scene = Scene {
            width: 200,
            height: 120,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "card".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 100.0, y: 60.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                content: LayerContent::Rect {
                    width: 100.0,
                    height: 60.0,
                    fill: Some(Paint::Solid {
                        color: "#3366CCFF".to_owned(),
                    }),
                    stroke: Some(mikan_composition::Stroke {
                        paint: Paint::Solid {
                            color: "#FFFFFFFF".to_owned(),
                        },
                        width: 4.0,
                    }),
                    corner_radius: 12.0,
                },
            }],
        };
        let mut renderer = CpuRenderer::default();
        let frame = renderer.render(&scene).unwrap();

        // Center of the rect is inside the fill, away from the stroke band.
        let center_offset = ((60 * frame.width() + 100) * 4) as usize;
        assert_eq!(
            &frame.pixels()[center_offset..center_offset + 4],
            &[0x33, 0x66, 0xCC, 0xFF]
        );

        // A pixel just outside the corner radius stays background (transparent
        // over the render's own background, so at least distinct from the fill
        // and stroke colors).
        let corner_offset = ((32 * frame.width() + 52) * 4) as usize;
        let corner_pixel = &frame.pixels()[corner_offset..corner_offset + 4];
        assert_ne!(corner_pixel, [0x33, 0x66, 0xCC, 0xFF]);
        assert_ne!(corner_pixel, [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn centers_visible_single_line_text_on_its_transform() {
        let scene = Scene {
            width: 1280,
            height: 720,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "title".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 640.0, y: 360.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                content: LayerContent::Text {
                    text: "Mikan".to_owned(),
                    style: TextStyle {
                        font_size: Some(96.0),
                        fill: Some(Paint::Solid {
                            color: "#FFA13BFF".to_owned(),
                        }),
                        align: Some(mikan_composition::TextAlign::Center),
                        ..TextStyle::default()
                    },
                    max_width: None,
                },
            }],
        };
        let mut renderer = CpuRenderer::default();
        let frame = renderer.render(&scene).unwrap();
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0;
        let mut max_y = 0;
        for (index, pixel) in frame.pixels().chunks_exact(4).enumerate() {
            if pixel != [20, 22, 28, 255] {
                let x = index as u32 % frame.width();
                let y = index as u32 / frame.width();
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        let center_x = f64::from(min_x + max_x) / 2.0;
        let center_y = f64::from(min_y + max_y) / 2.0;
        assert!((center_x - 640.0).abs() <= 0.5, "center x was {center_x}");
        assert!((center_y - 360.0).abs() <= 0.5, "center y was {center_y}");
    }

    #[test]
    fn alpha_composites_in_painter_order() {
        let mut destination = [0, 0, 0, 255];
        blend(&mut destination, Color::rgba(200, 100, 0, 255), 0.5);
        assert_eq!(destination, [100, 50, 0, 255]);
    }

    #[test]
    fn decodes_and_draws_a_real_image() {
        let scene = Scene {
            width: 2,
            height: 2,
            frame_rate: Rational::new(60, 1),
            time: Time::ZERO,
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "checker".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 1.0, y: 1.0 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                content: LayerContent::Image {
                    asset: ResolvedAsset {
                        id: "checker".to_owned(),
                        location: AssetLocation::File {
                            path: "tests/assets/checker.ppm".to_owned(),
                        },
                    },
                },
            }],
        };
        let mut renderer = CpuRenderer::default().with_asset_root(env!("CARGO_MANIFEST_DIR"));
        let frame = renderer.render(&scene).unwrap();

        assert_eq!(
            frame.pixels(),
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]
        );
    }

    #[test]
    fn renders_a_video_frame_from_the_injected_decoder() {
        struct Decoder;

        impl VideoFrameDecoder for Decoder {
            fn decode_frame(
                &mut self,
                path: &Path,
                source_time_seconds: f64,
            ) -> Result<VideoFrame, MediaError> {
                assert_eq!(path, Path::new("./clip.mp4"));
                assert_eq!(source_time_seconds, 2.0);
                Ok(VideoFrame {
                    width: 1,
                    height: 1,
                    pixels: vec![12, 34, 56, 255],
                })
            }
        }

        let scene = Scene {
            width: 1,
            height: 1,
            frame_rate: Rational::new(60, 1),
            time: Time::new(3, 2),
            fonts: Vec::new(),
            layers: vec![Layer {
                id: "video".to_owned(),
                transform: EvaluatedTransform {
                    position: Point { x: 0.5, y: 0.5 },
                    ..EvaluatedTransform::default()
                },
                opacity: 1.0,
                content: LayerContent::Video {
                    asset: ResolvedAsset {
                        id: "clip".to_owned(),
                        location: AssetLocation::File {
                            path: "clip.mp4".to_owned(),
                        },
                    },
                    timing: MediaTiming {
                        local_time: Time::new(1, 2),
                        source_start: Time::new(1, 1),
                        source_time_seconds: 2.0,
                        playback_rate: 2.0,
                    },
                },
            }],
        };
        let mut renderer = CpuRenderer::default().with_video_decoder(Decoder);
        let frame = renderer.render(&scene).unwrap();
        assert_eq!(frame.pixels(), &[12, 34, 56, 255]);
    }

    // `examples/assets/lipsync-fixture.psd` mimics a real "tachie" PSD: a
    // 240x320 canvas with every folder saved hidden and a `face/mouth`
    // group of six small vowel shapes at their true positions. Regenerate it
    // with `packages/react/scripts/make-lipsync-fixture.mjs`.
    fn lipsync_fixture_psd() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd")
    }

    fn pixel_at(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * frame.width + x) * 4) as usize;
        frame.pixels[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn rasterize_psd_without_a_preset_renders_nothing_when_every_folder_is_hidden() {
        let frame = rasterize_psd("fixture", &lipsync_fixture_psd(), &[], &[], &[]).unwrap();
        assert_eq!((frame.width, frame.height), (240, 320));
        assert!(
            frame.pixels.chunks_exact(4).all(|pixel| pixel[3] == 0),
            "a saved-all-hidden PSD with no preset should compose to nothing"
        );
    }

    #[test]
    fn rasterize_psd_composes_the_preset_and_the_selected_mouth_at_real_coordinates() {
        let preset = [
            "body".to_owned(),
            "body/base".to_owned(),
            "body/outfit-navy".to_owned(),
            "face".to_owned(),
            "face/eyes".to_owned(),
            "face/eyes/open".to_owned(),
        ];
        let frame = rasterize_psd(
            "fixture",
            &lipsync_fixture_psd(),
            &preset,
            &["face/mouth/a".to_owned()],
            &["face/mouth/o".to_owned()],
        )
        .unwrap();

        // The navy outfit rect covers (56,176)..(184,296).
        assert_eq!(pixel_at(&frame, 120, 220), [40, 60, 130, 255]);
        // The "a" mouth is a 24x20 rect at (108,142) — force-enabled even
        // though its layer and every ancestor folder are saved hidden. Its
        // red channel dominates, unlike the skin behind it.
        let mouth = pixel_at(&frame, 120, 150);
        assert_eq!(mouth[3], 255);
        assert!(mouth[0] > 150 && mouth[0] > mouth[1] + 40 && mouth[0] > mouth[2] + 40);
        // A point clear of every visible layer stays transparent.
        assert_eq!(pixel_at(&frame, 5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn rasterize_psd_disabled_layers_win_over_enabled() {
        let mouth = "face/mouth/a".to_owned();
        let with = rasterize_psd(
            "fixture",
            &lipsync_fixture_psd(),
            &["body".to_owned(), "body/base".to_owned()],
            std::slice::from_ref(&mouth),
            &[],
        )
        .unwrap();
        let without = rasterize_psd(
            "fixture",
            &lipsync_fixture_psd(),
            &["body".to_owned(), "body/base".to_owned()],
            std::slice::from_ref(&mouth),
            std::slice::from_ref(&mouth),
        )
        .unwrap();
        assert_ne!(pixel_at(&with, 120, 150), pixel_at(&without, 120, 150));
        // With the mouth suppressed the pixel is the bare skin base.
        assert_eq!(pixel_at(&without, 120, 150), [250, 224, 205, 255]);
    }
}
