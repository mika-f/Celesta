use crate::assets::{is_sfnt_or_woff, local_asset_path, read_asset};
use crate::error::RenderError;
use crate::font_stack::{FaceRun, falls_back_to_loaded};
use crate::linebreak::{is_emoji_cluster, is_visible_character};
use crate::shaping::{source_offsets, usable_font_runs};
use crate::text::{FontFallback, MissingGlyphs};
use celesta_composition::{FontFamily, FontFeatures, ResolvedAsset, TextStyle};
use cosmic_text::{Family, FontSystem, SwashCache, Weight, fontdb};
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "windows")]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct TextRasterizer {
    pub(crate) font_system: FontSystem,
    pub(crate) shaped_buffers: HashMap<String, Arc<cosmic_text::Buffer>>,
    /// The system's normalized locale, used by styles without `lang`.
    pub(crate) default_locale: String,
    /// Inactive systems, so alternating languages retain their shaping caches.
    pub(crate) locale_systems: HashMap<String, FontSystem>,
    /// The database `font_system` was constructed from, before any font was
    /// loaded into it: `FontSystem` derives fallback state from its initial
    /// database, so [`Self::fork`] rebuilds from this one.
    pub(crate) initial_db: fontdb::Database,
    pub(crate) swash_cache: SwashCache,
    pub(crate) loaded_fonts: HashSet<PathBuf>,
    /// The families each loaded font file, or stylesheet, added: a file's
    /// own family names, a stylesheet's private face families.
    asset_families: HashMap<PathBuf, Vec<Arc<str>>>,
    /// The families of the font list last given to [`Self::load_fonts`], in
    /// its order, which `fontFallback: 'loaded'` text falls back to.
    pub(crate) fallback_families: Arc<[Arc<str>]>,
    /// Advanced whenever a font loads or `fallback_families` changes.
    fonts_revision: u64,
    /// `matched_weight` results by family and requested weight, cleared
    /// whenever a font is loaded.
    pub(crate) matched_weights: HashMap<(String, u16), Option<u16>>,
    /// `matched_face`'s face by family and matched weight, cleared whenever
    /// a font is loaded. Every locale's system shares the database's ids.
    pub(crate) matched_faces: HashMap<(String, u16), Option<fontdb::ID>>,
    /// Each stylesheet face has a private family so shaping can select a
    /// particular Unicode subset instead of only the family's first file.
    pub(crate) stylesheet_faces: HashMap<String, Vec<Arc<str>>>,
    /// `face_runs` results for text with a font stack, by text and the
    /// style fields they depend on, cleared whenever a font is loaded.
    pub(crate) stacked_runs: HashMap<String, Arc<[FaceRun]>>,
    /// `feature_family`'s family for each family and feature set, cleared
    /// with the faces it added (`feature_faces`) whenever a font is loaded.
    pub(crate) feature_families: HashMap<(String, FontFeatures), Arc<str>>,
    pub(crate) feature_faces: Vec<fontdb::ID>,
    /// `color_emoji_family`'s result once looked up, cleared whenever a
    /// font is loaded.
    pub(crate) color_emoji_family: Option<Option<String>>,
    /// `missing_characters` results by locale, text, and requested faces,
    /// cleared whenever a font is loaded.
    pub(crate) missing_characters: HashMap<(String, String, String), MissingByFace>,
}

/// Characters each requested family (or font stack) and weight has no
/// glyph for.
pub(crate) type MissingByFace = Vec<(Vec<String>, u16, Vec<char>)>;

/// The family (or font stack) and weight pairs `style` asks for, the text's
/// own first, then each font run's (its family, or the text's), once each.
fn requested_faces(style: &TextStyle) -> Vec<(&FontFamily, u16)> {
    let weight = style.font_weight.unwrap_or(400);
    let mut faces: Vec<(&FontFamily, u16)> = style
        .font_family
        .as_ref()
        .map(|family| (family, weight))
        .into_iter()
        .collect();
    // An empty run covers nothing and shaping drops it, so it asks for no face.
    for run in style.font_runs.iter().filter(|run| run.start < run.end) {
        if let Some(family) = run.font_family.as_ref().or(style.font_family.as_ref()) {
            let face = (family, run.font_weight.or(style.font_weight).unwrap_or(400));
            if !faces.contains(&face) {
                faces.push(face);
            }
        }
    }
    faces
}

/// Families of color emoji fonts, most preferred first: the ones macOS and
/// Windows ship, then the ones Linux distributions and apps commonly carry.
pub(crate) const COLOR_EMOJI_FAMILIES: &[&str] = &[
    "Apple Color Emoji",
    "Segoe UI Emoji",
    "Noto Color Emoji",
    "Twemoji Mozilla",
    "Twemoji",
    "Twitter Color Emoji",
    "JoyPixels",
    "EmojiOne Color",
];

impl TextRasterizer {
    pub fn new() -> Self {
        let (locale, database) = FontSystem::new().into_locale_and_db();
        let default_locale = fallback_locale(&locale);
        let initial_db = database.clone();
        let font_system = FontSystem::new_with_locale_and_db(default_locale.clone(), database);
        #[cfg(target_os = "windows")]
        let font_system = {
            let mut font_system = font_system;
            load_directwrite_system_fonts(&mut font_system);
            font_system
        };

        Self {
            font_system,
            default_locale,
            locale_systems: HashMap::new(),
            initial_db,
            swash_cache: SwashCache::new(),
            loaded_fonts: HashSet::new(),
            asset_families: HashMap::new(),
            fallback_families: Arc::from([]),
            fonts_revision: 0,
            matched_weights: HashMap::new(),
            matched_faces: HashMap::new(),
            stylesheet_faces: HashMap::new(),
            stacked_runs: HashMap::new(),
            feature_families: HashMap::new(),
            feature_faces: Vec::new(),
            color_emoji_family: None,
            missing_characters: HashMap::new(),
            shaped_buffers: HashMap::new(),
        }
    }

    /// An independent rasterizer with the same fonts loaded, which draws
    /// any text exactly as this one does, so several can rasterize on
    /// separate threads. Built from the same initial database and then
    /// given this one's current database (face ids included) rather than
    /// reloading the fonts, which would also pick up loaded fonts as
    /// fallback candidates.
    pub fn fork(&self) -> Self {
        Self {
            font_system: self.font_system_for_locale(self.font_system.locale()),
            default_locale: self.default_locale.clone(),
            locale_systems: HashMap::new(),
            initial_db: self.initial_db.clone(),
            swash_cache: SwashCache::new(),
            loaded_fonts: self.loaded_fonts.clone(),
            asset_families: self.asset_families.clone(),
            fallback_families: Arc::clone(&self.fallback_families),
            fonts_revision: self.fonts_revision,
            matched_weights: HashMap::new(),
            matched_faces: HashMap::new(),
            stylesheet_faces: self.stylesheet_faces.clone(),
            stacked_runs: HashMap::new(),
            feature_families: self.feature_families.clone(),
            feature_faces: self.feature_faces.clone(),
            color_emoji_family: None,
            missing_characters: HashMap::new(),
            shaped_buffers: HashMap::new(),
        }
    }

    fn font_system_for_locale(&self, locale: &str) -> FontSystem {
        let mut system =
            FontSystem::new_with_locale_and_db(locale.to_owned(), self.initial_db.clone());
        *system.db_mut() = self.font_system.db().clone();
        system
    }

    pub(crate) fn select_language(&mut self, lang: Option<&str>) {
        let locale = lang
            .filter(|lang| !lang.trim().is_empty())
            .map(fallback_locale);
        let locale = locale.as_deref().unwrap_or(&self.default_locale);
        if self.font_system.locale() == locale {
            return;
        }
        let system = self
            .locale_systems
            .remove(locale)
            .unwrap_or_else(|| self.font_system_for_locale(locale));
        let previous = std::mem::replace(&mut self.font_system, system);
        // Bound the cache even when a composition generates language tags.
        if self.locale_systems.len() >= 16 {
            self.locale_systems.clear();
        }
        self.locale_systems
            .insert(previous.locale().to_owned(), previous);
    }

    /// How many font files have been loaded so far. It only grows, so a
    /// caller caching rasterized text can compare it across frames to tell
    /// when a newly loaded font may change how existing text lays out.
    pub fn loaded_font_count(&self) -> usize {
        self.loaded_fonts.len()
    }

    /// Changes whenever a font loads or the font list's fallback families
    /// change, so a caller caching rasterized text can compare it across
    /// frames to tell when existing text may lay out differently.
    pub fn fonts_revision(&self) -> u64 {
        self.fonts_revision
    }

    pub fn load_fonts(
        &mut self,
        fonts: &[ResolvedAsset],
        asset_root: &Path,
    ) -> Result<(), RenderError> {
        let mut families: Vec<Arc<str>> = Vec::new();
        for font in fonts {
            // Keyed by the resolved file, not the id: a React `<Font>` keeps
            // its `name` when its `src` changes across a reload.
            let path = local_asset_path(asset_root, font)?;
            if !self.loaded_fonts.contains(&path) {
                let added = self.load_font_asset(font, &path, asset_root)?;
                self.asset_families.insert(path.clone(), added);
                self.loaded_fonts.insert(path.clone());
            }
            for family in self.asset_families.get(&path).into_iter().flatten() {
                if !families.contains(family) {
                    families.push(Arc::clone(family));
                }
            }
        }
        if *self.fallback_families != *families {
            self.fallback_families = families.into();
            self.fonts_revision += 1;
            self.stacked_runs.clear();
            self.shaped_buffers.clear();
            self.missing_characters.clear();
        }
        Ok(())
    }

    /// Loads the font file or web font stylesheet `font` at `path`, and
    /// returns the families it added, in order.
    fn load_font_asset(
        &mut self,
        font: &ResolvedAsset,
        path: &Path,
        asset_root: &Path,
    ) -> Result<Vec<Arc<str>>, RenderError> {
        let data = read_asset(&font.id, path)?;
        if is_sfnt_or_woff(&data) || !celesta_remote::is_font_stylesheet(&data) {
            return self.load_font_data(&font.id, data, None);
        }
        // A web font stylesheet (e.g. Google Fonts): load every face its
        // `@font-face` rules point to, under their CSS family too.
        let css = String::from_utf8_lossy(&data);
        let faces =
            celesta_remote::stylesheet_font_faces(&css, &font.location).map_err(|source| {
                RenderError::RemoteAsset {
                    asset: font.id.clone(),
                    source,
                }
            })?;
        if faces.is_empty() {
            return Err(RenderError::InvalidFont {
                asset: font.id.clone(),
                reason: "the stylesheet has no @font-face url()",
            });
        }
        let mut families = Vec::new();
        for face in faces {
            let face_asset = ResolvedAsset {
                id: font.id.clone(),
                location: face.location,
            };
            let face_path = local_asset_path(asset_root, &face_asset)?;
            if !self.loaded_fonts.contains(&face_path) {
                let face_data = read_asset(&font.id, &face_path)?;
                let added = self.load_font_data(&font.id, face_data, face.family)?;
                self.asset_families.insert(face_path.clone(), added);
                self.loaded_fonts.insert(face_path.clone());
            }
            families.extend(
                self.asset_families
                    .get(&face_path)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        Ok(families)
    }

    /// Loads a TrueType/OpenType font, or a WOFF/WOFF2 one after unpacking it.
    /// `alias` adds a family name the faces also match, besides the ones
    /// stored in the file.
    pub(crate) fn load_font_data(
        &mut self,
        asset: &str,
        data: Vec<u8>,
        alias: Option<String>,
    ) -> Result<Vec<Arc<str>>, RenderError> {
        let invalid = |reason| RenderError::InvalidFont {
            asset: asset.to_owned(),
            reason,
        };
        let data = match data.get(..4) {
            Some(b"wOFF") => {
                wuff::decompress_woff1(&data).map_err(|_| invalid("invalid WOFF data"))?
            }
            Some(b"wOF2") => {
                wuff::decompress_woff2(&data).map_err(|_| invalid("invalid WOFF2 data"))?
            }
            _ => data,
        };
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(fontdb::Source::Binary(Arc::new(data)));
        if ids.is_empty() {
            return Err(invalid("no font faces found"));
        }
        self.drop_feature_families();
        let database = self.font_system.db_mut();
        self.fonts_revision += 1;
        self.locale_systems.clear();
        self.matched_weights.clear();
        self.matched_faces.clear();
        self.stacked_runs.clear();
        self.color_emoji_family = None;
        self.missing_characters.clear();
        self.shaped_buffers.clear();
        let Some(alias) = alias else {
            let mut families: Vec<Arc<str>> = Vec::new();
            for id in ids {
                let Some((family, _)) = database.face(id).and_then(|face| face.families.first())
                else {
                    continue;
                };
                if !families.iter().any(|name| **name == *family) {
                    families.push(family.as_str().into());
                }
            }
            return Ok(families);
        };
        let mut families = Vec::new();
        for id in ids {
            let Some(mut face) = database.face(id).cloned() else {
                continue;
            };
            let private: Arc<str> = format!("celesta-stylesheet-face-{id:?}").into();
            face.families
                .push((private.to_string(), fontdb::Language::English_UnitedStates));
            if !face.families.iter().any(|(family, _)| *family == alias) {
                face.families
                    .push((alias.clone(), fontdb::Language::English_UnitedStates));
            }
            self.stylesheet_faces
                .entry(alias.clone())
                .or_default()
                .push(Arc::clone(&private));
            families.push(private);
            database.remove_face(id);
            database.push_face_info(face);
        }
        Ok(families)
    }

    /// The weight of the face that CSS font matching (CSS Fonts §5.2) picks
    /// from `family` for `requested`: `requested` itself when the family has
    /// a face of that weight, otherwise the nearest one it has. `None` when
    /// no face of `family` is loaded or installed.
    pub(crate) fn matched_weight(&mut self, family: &str, requested: u16) -> Option<u16> {
        let database = self.font_system.db();
        *self
            .matched_weights
            .entry((family.to_owned(), requested))
            .or_insert_with(|| {
                let id = database.query(&fontdb::Query {
                    families: &[Family::Name(family)],
                    weight: Weight(requested),
                    ..fontdb::Query::default()
                })?;
                Some(database.face(id)?.weight.0)
            })
    }

    /// Whether some family of `family` has a loaded or installed face.
    fn has_face(&mut self, family: &FontFamily, weight: u16) -> bool {
        family
            .names()
            .iter()
            .any(|name| self.matched_weight(name, weight).is_some())
    }

    /// The fallbacks `style` is drawn with on `layer`: one per family (or
    /// font stack) and weight it asks for (its own `fontFamily` or a font
    /// run's) of which no family has a loaded or installed face. Empty when
    /// each has a face or none is named.
    pub fn font_fallback(&mut self, layer: &str, style: &TextStyle) -> Vec<FontFallback> {
        let mut fallbacks = Vec::new();
        for (family, weight) in requested_faces(style) {
            if !self.has_face(family, weight) {
                fallbacks.push(FontFallback {
                    layer: layer.to_owned(),
                    families: family.names().to_vec(),
                    weight,
                });
            }
        }
        fallbacks
    }

    /// The first of [`COLOR_EMOJI_FAMILIES`] with a loaded or installed face.
    pub(crate) fn color_emoji_family(&mut self) -> Option<String> {
        let database = self.font_system.db();
        self.color_emoji_family
            .get_or_insert_with(|| {
                COLOR_EMOJI_FAMILIES
                    .iter()
                    .find(|family| {
                        database
                            .faces()
                            .any(|face| face.families.iter().any(|(name, _)| name == *family))
                    })
                    .map(|family| (*family).to_owned())
            })
            .clone()
    }

    /// The characters of `text` that the family each asks for (`style`'s
    /// `fontFamily`, or its font run's) has no glyph for, drawn on `layer`
    /// with another font instead: one entry per family and weight. For a
    /// font stack, the characters no family of it has, in one entry. Empty
    /// when every family draws its characters or none is named. A family
    /// (or stack) with no face at all is left out ([`Self::font_fallback`]
    /// reports it). Emoji that another font draws are left out, since they
    /// are meant to come from a color emoji font, and so are whitespace and
    /// invisible characters. Characters no font has, emoji included, are
    /// drawn as a missing-glyph box and always reported.
    pub fn missing_glyphs(
        &mut self,
        layer: &str,
        text: &str,
        style: &TextStyle,
    ) -> Vec<MissingGlyphs> {
        let present: Vec<(FontFamily, u16)> = requested_faces(style)
            .into_iter()
            .filter(|&(family, weight)| self.has_face(family, weight))
            .map(|(family, weight)| (family.clone(), weight))
            .collect();
        if present.is_empty() {
            return Vec::new();
        }
        self.select_language(style.lang.as_deref());
        let key = (
            self.font_system.locale().to_owned(),
            text.to_owned(),
            format!(
                "{:?}",
                (
                    &style.font_family,
                    style.font_weight,
                    &style.font_runs,
                    style.font_fallback
                )
            ),
        );
        let groups = match self.missing_characters.get(&key) {
            Some(groups) => groups.clone(),
            None => {
                let groups = self.missing_characters(text, style, &present);
                // Text that changes every frame (a counter, a subtitle)
                // would otherwise grow this without bound.
                if self.missing_characters.len() >= 4096 {
                    self.missing_characters.clear();
                }
                self.missing_characters.insert(key, groups.clone());
                groups
            }
        };
        groups
            .into_iter()
            .map(|(families, weight, characters)| MissingGlyphs {
                layer: layer.to_owned(),
                families,
                weight,
                characters,
            })
            .collect()
    }

    /// The characters each family (or font stack) and weight in `present`
    /// has no glyph for, judging each glyph by the family it asks for.
    pub(crate) fn missing_characters(
        &mut self,
        text: &str,
        style: &TextStyle,
        present: &[(FontFamily, u16)],
    ) -> MissingByFace {
        // The font each character is drawn with does not depend on the
        // wrap width or the size, so shape on one unwrapped line at scale 1.
        let buffer = self.shaped_buffer(text, style, None, 1.0);
        let runs = if style.font_runs.is_empty() {
            Vec::new()
        } else {
            usable_font_runs(style, text.chars().count())
        };
        let offsets = (!runs.is_empty()).then(|| source_offsets(&buffer, text, style));
        // The family (or stack) and weight a glyph asks for: its font
        // run's, or the text's.
        let face_at = |line_i: usize, start: usize| -> Option<(&FontFamily, u16)> {
            let run = offsets.as_ref().and_then(|offsets| {
                let point = offsets[line_i][start];
                let index = runs.partition_point(|run| run.end <= point);
                runs.get(index).filter(|run| run.start <= point)
            });
            let family = run
                .and_then(|run| run.font_family.as_ref())
                .or(style.font_family.as_ref())?;
            let weight = run
                .and_then(|run| run.font_weight)
                .or(style.font_weight)
                .unwrap_or(400);
            Some((family, weight))
        };
        // Loaded fonts the style falls back to draw what they have as
        // intended, like a family of the stack.
        let fallback = falls_back_to_loaded(style).then(|| Arc::clone(&self.fallback_families));
        let fallback = fallback.as_deref().unwrap_or_default();
        let database = self.font_system.db();
        let mut groups: MissingByFace = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let Some((family, weight)) = face_at(run.line_i, glyph.start) else {
                    continue;
                };
                // A family with no face gets the fallback warning instead.
                if !present
                    .iter()
                    .any(|(asked_family, asked)| asked_family == family && *asked == weight)
                {
                    continue;
                }
                let family = family.names();
                // Glyph 0 is `.notdef`: no font had the character, and the
                // missing-glyph box is drawn.
                let drawn = glyph.glyph_id != 0;
                let from_family = drawn
                    && database.face(glyph.font_id).is_some_and(|face| {
                        face.families.iter().any(|(name, _)| {
                            family.contains(name) || fallback.iter().any(|loaded| **loaded == *name)
                        })
                    });
                let Some(cluster) = run.text.get(glyph.start..glyph.end) else {
                    continue;
                };
                // An emoji drawn from a color emoji font is expected; one no
                // font has is a box like any other missing character.
                if from_family || (drawn && is_emoji_cluster(cluster)) {
                    continue;
                }
                let index = match groups
                    .iter()
                    .position(|(name, asked, _)| name == family && *asked == weight)
                {
                    Some(index) => index,
                    None => {
                        groups.push((family.to_vec(), weight, Vec::new()));
                        groups.len() - 1
                    }
                };
                for character in cluster.chars().filter(|&c| is_visible_character(c)) {
                    if !groups[index].2.contains(&character) {
                        groups[index].2.push(character);
                    }
                }
            }
        }
        groups
    }
}

/// cosmic-text matches CJK fallback locales exactly. Normalize language and
/// region/script tags to the keys its platform tables recognize.
pub(crate) fn fallback_locale(lang: &str) -> String {
    let lang = lang.trim().split(['.', '@']).next().unwrap_or_default();
    let mut parts = lang.split(['-', '_']);
    let language = parts.next().unwrap_or_default().to_ascii_lowercase();
    if language != "zh" {
        return language;
    }
    let mut script = None;
    let mut region = None;
    // Extensions and private-use subtags do not describe the language.
    for part in parts.take_while(|part| part.len() != 1) {
        if part.eq_ignore_ascii_case("hans") || part.eq_ignore_ascii_case("hant") {
            script = Some(part);
        } else if part.len() == 2 {
            region = Some(part);
        }
    }
    let is_region = |value: &str| region.is_some_and(|region| region.eq_ignore_ascii_case(value));
    if script.is_some_and(|script| script.eq_ignore_ascii_case("hans")) {
        "zh-CN"
    } else if is_region("HK") || is_region("MO") {
        "zh-HK"
    } else if is_region("TW") || script.is_some_and(|script| script.eq_ignore_ascii_case("hant")) {
        "zh-TW"
    } else {
        "zh-CN"
    }
    .to_owned()
}

#[cfg(target_os = "windows")]
pub(crate) fn load_directwrite_system_fonts(font_system: &mut FontSystem) {
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

impl Default for TextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}
