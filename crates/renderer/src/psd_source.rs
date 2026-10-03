//! Shared PSD portrait compositing for both renderers.
//!
//! A portrait is drawn as many layer combinations (expression × mouth ×
//! blink), so a composite per combination at the PSD's full size quickly
//! adds up to gigabytes. Instead each PSD is parsed once, each layer is
//! decoded once per resolution and kept cropped to its bounds, and a
//! combination is composited from those layers at the smallest power-of-two
//! reduction that is still at least as dense as it is drawn. Everything is
//! kept in one cache bounded by bytes, dropping the least recently used.
use super::{
    Color, RenderError, blend, blend_mixed,
    image_source::{fit_within, resize_rgba},
    psd_blend_channel,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::Arc,
};

/// Bytes [`PsdSources::default`] keeps across parsed documents, decoded
/// layers, and composites.
pub const DEFAULT_BUDGET: usize = 1 << 30;

/// Halvings past which a portrait is not reduced further.
const MAX_LEVEL: u32 = 12;

/// A composited portrait: `width` x `height` straight RGBA pixels covering
/// the whole `canvas_width` x `canvas_height` PSD canvas.
#[derive(Clone)]
pub struct PsdImage {
    pub pixels: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub canvas_width: u32,
    pub canvas_height: u32,
}

pub struct PsdSources {
    budget: usize,
    bytes: usize,
    tick: u64,
    entries: HashMap<String, Entry>,
}

struct Entry {
    value: Value,
    bytes: usize,
    last_used: u64,
}

#[derive(Clone)]
enum Value {
    Document(Arc<Document>),
    Layer(Arc<LayerRaster>),
    Composite(PsdImage),
}

struct Document {
    psd: psd::Psd,
    /// Each layer's `folder/…/name` path, in `psd.layers()` order.
    paths: Vec<String>,
}

/// One layer reduced to a level and cropped to its bounds, at
/// (`left`, `top`) of the reduced canvas.
struct LayerRaster {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Default for PsdSources {
    fn default() -> Self {
        Self::with_budget(DEFAULT_BUDGET)
    }
}

impl PsdSources {
    /// A cache holding at most `budget` bytes, besides the entry used last.
    pub fn with_budget(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            tick: 0,
            entries: HashMap::new(),
        }
    }

    /// Composites the PSD at `path` for drawing at `density` output pixels
    /// per PSD pixel.
    ///
    /// Layer visibility is resolved as: `disabled_layers` always hide, then
    /// `enabled_layers` always show (the current lip-sync mouth), then — when
    /// `visible_layers` is non-empty — exactly the listed layer paths compose
    /// (a portrait preset; the PSD's own saved visibility is ignored),
    /// otherwise the PSD's saved per-layer/-folder visibility drives the
    /// composite. Each layer is placed at its real PSD coordinates.
    /// Per-layer opacity is applied; group opacity is not (the `psd` crate
    /// misreads it as 0 for real PSDTool files).
    pub fn render(
        &mut self,
        asset: &str,
        path: &Path,
        visible_layers: &[String],
        enabled_layers: &[String],
        disabled_layers: &[String],
        density: f64,
    ) -> Result<PsdImage, RenderError> {
        self.render_within(
            asset,
            path,
            visible_layers,
            enabled_layers,
            disabled_layers,
            density,
            u32::MAX,
        )
    }

    /// [`Self::render`], shrunk as needed so neither side of the composite
    /// exceeds `max_dimension` (a GPU's largest texture). The drawn density
    /// is then lower than asked for, and the draw enlarges the composite.
    #[allow(clippy::too_many_arguments)]
    pub fn render_within(
        &mut self,
        asset: &str,
        path: &Path,
        visible_layers: &[String],
        enabled_layers: &[String],
        disabled_layers: &[String],
        density: f64,
        max_dimension: u32,
    ) -> Result<PsdImage, RenderError> {
        self.tick += 1;
        let requested = level_for(density);
        let key = format!(
            "composite\0{requested}\0{max_dimension}\0{asset}\0{}\0{}\0{}",
            visible_layers.join("\0"),
            enabled_layers.join("\0"),
            disabled_layers.join("\0")
        );
        if let Some(Value::Composite(image)) = self.get(&key) {
            return Ok(image);
        }
        let document = self.document(asset, path)?;
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
        // `enabled`/`disabled` come straight from the character's lip-sync
        // configuration, so a typo there should surface rather than silently
        // do nothing. `visible_layers` is a preset that may target a slightly
        // different build of the PSD, so unknown entries there are ignored.
        for requested in enabled.iter().chain(disabled.iter()) {
            if !document.paths.iter().any(|path| path == requested) {
                return Err(RenderError::MissingPsdLayer {
                    asset: asset.to_owned(),
                    layer: requested.clone(),
                });
            }
        }
        let use_preset = !visible.is_empty();

        let psd = &document.psd;
        // A level that would still exceed `max_dimension` after one more
        // halving is skipped, so the composite is at most 2x too large and
        // the resampling below shrinks it no more than it has to.
        let largest_at = |level: u32| psd.width().max(psd.height()).div_ceil(1 << level);
        let mut level = requested;
        while level < MAX_LEVEL && largest_at(level + 1) > max_dimension {
            level += 1;
        }
        let factor = 1 << level;
        let width = psd.width().div_ceil(factor);
        let height = psd.height().div_ceil(factor);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for (index, (layer, path)) in psd.layers().iter().zip(&document.paths).enumerate().rev() {
            let shown = if disabled.contains(path) {
                false
            } else if enabled.contains(path) {
                true
            } else if use_preset {
                visible.contains(path)
            } else {
                layer.visible() && psd_ancestors_visible(psd, layer.parent_id())
            };
            if !shown {
                continue;
            }
            let raster = self.layer(asset, &document, index, level);
            // Group opacity is deliberately not applied: the `psd` crate reads
            // it from the wrong ("bounding section") record and reports 0 for
            // every folder in real PSDTool files. Per-layer opacity is read
            // correctly. Likewise each layer's own blend mode is applied, and
            // a folder's is not (folders pass through).
            let opacity = f64::from(layer.opacity()) / 255.0;
            // The `psd` crate does not export its `BlendMode` type, only its
            // values, so modes are told apart by name.
            let mix = psd_blend_channel(&format!("{:?}", layer.blend_mode()));
            let row_bytes = raster.width as usize * 4;
            for row in 0..raster.height as usize {
                let start =
                    ((raster.top as usize + row) * width as usize + raster.left as usize) * 4;
                let destination = &mut pixels[start..start + row_bytes];
                let source = &raster.pixels[row * row_bytes..(row + 1) * row_bytes];
                for (destination, source) in
                    destination.chunks_exact_mut(4).zip(source.chunks_exact(4))
                {
                    if source[3] == 0 {
                        continue;
                    }
                    let source = Color::rgba(source[0], source[1], source[2], source[3]);
                    match mix {
                        Some(mix) => blend_mixed(destination, source, opacity, mix),
                        None => blend(destination, source, opacity),
                    }
                }
            }
        }
        let (fit_width, fit_height) = fit_within(width, height, max_dimension);
        let pixels = if (fit_width, fit_height) == (width, height) {
            pixels
        } else {
            resize_rgba(width, height, &pixels, fit_width, fit_height).into_raw()
        };
        let image = PsdImage {
            pixels: Arc::new(pixels),
            width: fit_width,
            height: fit_height,
            canvas_width: psd.width(),
            canvas_height: psd.height(),
        };
        self.insert(key, Value::Composite(image.clone()), image.pixels.len());
        Ok(image)
    }

    /// Bytes currently cached.
    pub fn cached_bytes(&self) -> usize {
        self.bytes
    }

    fn document(&mut self, asset: &str, path: &Path) -> Result<Arc<Document>, RenderError> {
        let key = format!("document\0{asset}");
        if let Some(Value::Document(document)) = self.get(&key) {
            return Ok(document);
        }
        let bytes = fs::read(path).map_err(|source| RenderError::AssetIo {
            asset: asset.to_owned(),
            source,
        })?;
        let psd = psd::Psd::from_bytes(&bytes).map_err(|source| RenderError::PsdDecode {
            asset: asset.to_owned(),
            source,
        })?;
        let paths = psd
            .layers()
            .iter()
            .map(|layer| psd_layer_path(&psd, layer))
            .collect();
        let document = Arc::new(Document { psd, paths });
        // The parsed document holds about as much as the file's channel data.
        self.insert(key, Value::Document(document.clone()), bytes.len());
        Ok(document)
    }

    fn layer(
        &mut self,
        asset: &str,
        document: &Document,
        index: usize,
        level: u32,
    ) -> Arc<LayerRaster> {
        let key = format!("layer\0{level}\0{index}\0{asset}");
        if let Some(Value::Layer(raster)) = self.get(&key) {
            return raster;
        }
        let raster = Arc::new(reduce_layer(
            &document.psd,
            &document.psd.layers()[index],
            level,
        ));
        self.insert(key, Value::Layer(raster.clone()), raster.pixels.len());
        raster
    }

    fn get(&mut self, key: &str) -> Option<Value> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.tick;
        Some(entry.value.clone())
    }

    fn insert(&mut self, key: String, value: Value, bytes: usize) {
        self.bytes += bytes;
        if let Some(previous) = self.entries.insert(
            key.clone(),
            Entry {
                value,
                bytes,
                last_used: self.tick,
            },
        ) {
            self.bytes -= previous.bytes;
        }
        while self.bytes > self.budget {
            let Some(oldest) = self
                .entries
                .iter()
                .filter(|(candidate, _)| **candidate != key)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(candidate, _)| candidate.clone())
            else {
                break;
            };
            let evicted = self.entries.remove(&oldest).expect("evicted entry exists");
            self.bytes -= evicted.bytes;
        }
    }
}

/// The number of halvings that leave a portrait drawn at `density` output
/// pixels per PSD pixel still at least that dense, so it is never stretched
/// and shrunk at most 2x when drawn.
pub fn level_for(density: f64) -> u32 {
    if density >= 1.0 {
        0
    } else if density > 0.0 {
        ((1.0 / density).log2().floor() as u32).min(MAX_LEVEL)
    } else {
        // Zero, negative, or NaN: nothing visible is drawn.
        MAX_LEVEL
    }
}

/// Decodes `layer` and box-filters it by `2^level`, cropped to the reduced
/// pixels its bounds touch.
fn reduce_layer(psd: &psd::Psd, layer: &psd::PsdLayer, level: u32) -> LayerRaster {
    let (canvas_width, canvas_height) = (psd.width(), psd.height());
    let factor = 1 << level;
    // `rgba()` places the layer on a canvas-sized buffer, transparent
    // outside the layer's bounds. Bounds are inclusive in the `psd` crate.
    let clamp = |value: i32, limit: u32| value.clamp(0, limit as i32) as u32;
    let left = clamp(layer.layer_left(), canvas_width) / factor;
    let top = clamp(layer.layer_top(), canvas_height) / factor;
    let right = clamp(layer.layer_right().saturating_add(1), canvas_width).div_ceil(factor);
    let bottom = clamp(layer.layer_bottom().saturating_add(1), canvas_height).div_ceil(factor);
    if right <= left || bottom <= top {
        return LayerRaster {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
    }
    let rgba = layer.rgba();
    let (width, height) = (right - left, bottom - top);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in top..bottom {
        let source_rows = y * factor..((y + 1) * factor).min(canvas_height);
        for x in left..right {
            let source_columns = x * factor..((x + 1) * factor).min(canvas_width);
            // Averaged with premultiplied alpha, so transparent pixels do not
            // darken the edges.
            let mut sums = [0_u64; 4];
            for source_y in source_rows.clone() {
                let row = source_y as usize * canvas_width as usize;
                for source_x in source_columns.clone() {
                    let offset = (row + source_x as usize) * 4;
                    let alpha = u64::from(rgba[offset + 3]);
                    for channel in 0..3 {
                        sums[channel] += u64::from(rgba[offset + channel]) * alpha;
                    }
                    sums[3] += alpha;
                }
            }
            let count = (source_rows.len() * source_columns.len()) as u64;
            let alpha = sums[3];
            if alpha == 0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for sum in &sums[..3] {
                pixels.push(((sum + alpha / 2) / alpha) as u8);
            }
            pixels.push(((alpha + count / 2) / count) as u8);
        }
    }
    LayerRaster {
        left,
        top,
        width,
        height,
        pixels,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // See `lipsync_fixture_psd` in the crate tests: a 240x320 canvas.
    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/lipsync-fixture.psd")
    }

    fn preset() -> Vec<String> {
        [
            "body",
            "body/base",
            "body/outfit-navy",
            "face",
            "face/eyes",
            "face/eyes/open",
        ]
        .map(str::to_owned)
        .to_vec()
    }

    fn pixel_at(image: &PsdImage, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * image.width + x) * 4) as usize;
        image.pixels[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn levels_keep_at_least_the_drawn_density() {
        assert_eq!(level_for(2.0), 0);
        assert_eq!(level_for(1.0), 0);
        assert_eq!(level_for(0.75), 0);
        assert_eq!(level_for(0.5), 1);
        assert_eq!(level_for(0.15), 2);
        assert_eq!(level_for(0.0), MAX_LEVEL);
        assert_eq!(level_for(f64::NAN), MAX_LEVEL);
        assert_eq!(level_for(1e-9), MAX_LEVEL);
    }

    #[test]
    fn composites_at_a_reduced_resolution_for_a_shrunk_portrait() {
        let mut sources = PsdSources::default();
        let mouth = ["face/mouth/a".to_owned()];
        let full = sources
            .render("fixture", &fixture(), &preset(), &mouth, &[], 1.0)
            .unwrap();
        let quarter = sources
            .render("fixture", &fixture(), &preset(), &mouth, &[], 0.25)
            .unwrap();
        assert_eq!((full.width, full.height), (240, 320));
        assert_eq!((quarter.width, quarter.height), (60, 80));
        assert_eq!((quarter.canvas_width, quarter.canvas_height), (240, 320));
        // Inside the navy outfit, (56,176)..(184,296), and clear of it.
        assert_eq!(pixel_at(&quarter, 30, 55), pixel_at(&full, 120, 220));
        assert_eq!(pixel_at(&quarter, 1, 1), [0, 0, 0, 0]);
    }

    #[test]
    fn shrinks_a_composite_to_fit_the_largest_texture() {
        let mut sources = PsdSources::default();
        let full = sources
            .render("fixture", &fixture(), &preset(), &[], &[], 1.0)
            .unwrap();
        // 320 halves to 160, which fits exactly: no resampling.
        let halved = sources
            .render_within("fixture", &fixture(), &preset(), &[], &[], 1.0, 160)
            .unwrap();
        assert_eq!((halved.width, halved.height), (120, 160));
        assert_eq!(pixel_at(&halved, 60, 118), pixel_at(&full, 120, 236));
        // 160 is still too tall, so the 120x160 level shrinks to 75x100
        // rather than halving again to 60x80.
        let fitted = sources
            .render_within("fixture", &fixture(), &preset(), &[], &[], 1.0, 100)
            .unwrap();
        assert_eq!((fitted.width, fitted.height), (75, 100));
        assert_eq!((fitted.canvas_width, fitted.canvas_height), (240, 320));
        assert_eq!(fitted.pixels.len(), 75 * 100 * 4);
        assert_eq!(pixel_at(&fitted, 37, 73), pixel_at(&full, 120, 236));
        assert_eq!(pixel_at(&fitted, 1, 1), [0, 0, 0, 0]);
        // A smaller drawn size that already fits is not affected.
        let quarter = sources
            .render_within("fixture", &fixture(), &preset(), &[], &[], 0.25, 100)
            .unwrap();
        assert_eq!((quarter.width, quarter.height), (60, 80));
    }

    #[test]
    fn reuses_a_cached_composite() {
        let mut sources = PsdSources::default();
        let first = sources
            .render("fixture", &fixture(), &preset(), &[], &[], 1.0)
            .unwrap();
        let again = sources
            .render("fixture", &fixture(), &preset(), &[], &[], 1.0)
            .unwrap();
        assert!(Arc::ptr_eq(&first.pixels, &again.pixels));
    }

    #[test]
    fn evicts_the_least_recently_used_entries_past_the_budget() {
        // Room for about one full-size composite (240x320 RGBA is 307200
        // bytes) next to the document and its layers, not for six.
        let budget = 1 << 20;
        let mut sources = PsdSources::with_budget(budget);
        let mouths = ["closed", "a", "i", "u", "e", "o"];
        let mut first = None;
        for mouth in mouths {
            let enabled = [format!("face/mouth/{mouth}")];
            let image = sources
                .render("fixture", &fixture(), &preset(), &enabled, &[], 1.0)
                .unwrap();
            first.get_or_insert(image);
            assert!(
                sources.cached_bytes() <= budget,
                "{}",
                sources.cached_bytes()
            );
        }
        let enabled = ["face/mouth/closed".to_owned()];
        let again = sources
            .render("fixture", &fixture(), &preset(), &enabled, &[], 1.0)
            .unwrap();
        assert!(!Arc::ptr_eq(&first.unwrap().pixels, &again.pixels));
    }
}
