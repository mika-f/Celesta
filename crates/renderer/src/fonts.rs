use crate::assets::{is_sfnt_or_woff, local_asset_path, read_asset};
use crate::error::RenderError;
use crate::linebreak::{is_emoji_cluster, is_visible_character};
use crate::text::{FontFallback, MissingGlyphs};
use celesta_composition::{ResolvedAsset, TextStyle};
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
    locale_systems: HashMap<String, FontSystem>,
    /// The database `font_system` was constructed from, before any font was
    /// loaded into it: `FontSystem` derives fallback state from its initial
    /// database, so [`Self::fork`] rebuilds from this one.
    pub(crate) initial_db: fontdb::Database,
    pub(crate) swash_cache: SwashCache,
    pub(crate) loaded_fonts: HashSet<PathBuf>,
    /// `matched_weight` results by family and requested weight, cleared
    /// whenever a font is loaded.
    pub(crate) matched_weights: HashMap<(String, u16), Option<u16>>,
    /// `color_emoji_family`'s result once looked up, cleared whenever a
    /// font is loaded.
    pub(crate) color_emoji_family: Option<Option<String>>,
    /// `missing_characters` results by locale, text, family, and weight, cleared
    /// whenever a font is loaded.
    pub(crate) missing_characters: HashMap<(String, String, String, u16), Vec<char>>,
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
            matched_weights: HashMap::new(),
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
            matched_weights: HashMap::new(),
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

    pub fn load_fonts(
        &mut self,
        fonts: &[ResolvedAsset],
        asset_root: &Path,
    ) -> Result<(), RenderError> {
        for font in fonts {
            // Keyed by the resolved file, not the id: a React `<Font>` keeps
            // its `name` when its `src` changes across a reload.
            let path = local_asset_path(asset_root, font)?;
            if self.loaded_fonts.contains(&path) {
                continue;
            }
            let data = read_asset(&font.id, &path)?;
            if !is_sfnt_or_woff(&data) && celesta_remote::is_font_stylesheet(&data) {
                // A web font stylesheet (e.g. Google Fonts): load every face
                // its `@font-face` rules point to, under their CSS family too.
                let css = String::from_utf8_lossy(&data);
                let faces = celesta_remote::stylesheet_font_faces(&css, &font.location);
                if faces.is_empty() {
                    return Err(RenderError::InvalidFont {
                        asset: font.id.clone(),
                        reason: "the stylesheet has no @font-face url()",
                    });
                }
                for face in faces {
                    let face_asset = ResolvedAsset {
                        id: font.id.clone(),
                        location: face.location,
                    };
                    let face_path = local_asset_path(asset_root, &face_asset)?;
                    if !self.loaded_fonts.contains(&face_path) {
                        let face_data = read_asset(&font.id, &face_path)?;
                        self.load_font_data(&font.id, face_data, face.family)?;
                        self.loaded_fonts.insert(face_path);
                    }
                }
            } else {
                self.load_font_data(&font.id, data, None)?;
            }
            self.loaded_fonts.insert(path);
        }
        Ok(())
    }

    /// Loads a TrueType/OpenType font, or a WOFF/WOFF2 one after unpacking it.
    /// `alias` adds a family name the faces also match, besides the ones
    /// stored in the file.
    pub(crate) fn load_font_data(
        &mut self,
        asset: &str,
        data: Vec<u8>,
        alias: Option<String>,
    ) -> Result<(), RenderError> {
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
        let database = self.font_system.db_mut();
        let ids = database.load_font_source(fontdb::Source::Binary(Arc::new(data)));
        if ids.is_empty() {
            return Err(invalid("no font faces found"));
        }
        self.locale_systems.clear();
        self.matched_weights.clear();
        self.color_emoji_family = None;
        self.missing_characters.clear();
        self.shaped_buffers.clear();
        let Some(alias) = alias else {
            return Ok(());
        };
        for id in ids {
            let Some(mut face) = database.face(id).cloned() else {
                continue;
            };
            if face.families.iter().any(|(family, _)| *family == alias) {
                continue;
            }
            face.families
                .push((alias.clone(), fontdb::Language::English_UnitedStates));
            database.remove_face(id);
            database.push_face_info(face);
        }
        Ok(())
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

    /// The fallback `style` is drawn with on `layer` when its `fontFamily`
    /// has no loaded or installed face; `None` when it has one or names no
    /// family.
    pub fn font_fallback(&mut self, layer: &str, style: &TextStyle) -> Option<FontFallback> {
        let family = style.font_family.as_deref()?;
        let weight = style.font_weight.unwrap_or(400);
        self.matched_weight(family, weight)
            .is_none()
            .then(|| FontFallback {
                layer: layer.to_owned(),
                family: family.to_owned(),
                weight,
            })
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

    /// The characters of `text` that `style`'s `fontFamily` has no glyph
    /// for, drawn on `layer` with another font instead; `None` when the
    /// family draws all of them, names no family, or has no face at all
    /// (which [`Self::font_fallback`] reports). Emoji that another font
    /// draws are left out, since they are meant to come from a color emoji
    /// font, and so are whitespace and invisible characters. Characters no
    /// font has, emoji included, are drawn as a missing-glyph box and
    /// always reported.
    pub fn missing_glyphs(
        &mut self,
        layer: &str,
        text: &str,
        style: &TextStyle,
    ) -> Option<MissingGlyphs> {
        let family = style.font_family.as_deref()?;
        self.select_language(style.lang.as_deref());
        let weight = style.font_weight.unwrap_or(400);
        self.matched_weight(family, weight)?;
        let key = (
            self.font_system.locale().to_owned(),
            text.to_owned(),
            family.to_owned(),
            weight,
        );
        let characters = match self.missing_characters.get(&key) {
            Some(characters) => characters.clone(),
            None => {
                let characters = self.missing_characters(text, style, family);
                // Text that changes every frame (a counter, a subtitle)
                // would otherwise grow this without bound.
                if self.missing_characters.len() >= 4096 {
                    self.missing_characters.clear();
                }
                self.missing_characters.insert(key, characters.clone());
                characters
            }
        };
        (!characters.is_empty()).then(|| MissingGlyphs {
            layer: layer.to_owned(),
            family: family.to_owned(),
            weight,
            characters,
        })
    }

    pub(crate) fn missing_characters(
        &mut self,
        text: &str,
        style: &TextStyle,
        family: &str,
    ) -> Vec<char> {
        // The font each character is drawn with does not depend on the
        // wrap width or the size, so shape on one unwrapped line at scale 1.
        let buffer = self.shaped_buffer(text, style, None, 1.0);
        let database = self.font_system.db();
        let mut characters = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                // Glyph 0 is `.notdef`: no font had the character, and the
                // missing-glyph box is drawn.
                let drawn = glyph.glyph_id != 0;
                let from_family = drawn
                    && database
                        .face(glyph.font_id)
                        .is_some_and(|face| face.families.iter().any(|(name, _)| name == family));
                let Some(cluster) = run.text.get(glyph.start..glyph.end) else {
                    continue;
                };
                // An emoji drawn from a color emoji font is expected; one no
                // font has is a box like any other missing character.
                if from_family || (drawn && is_emoji_cluster(cluster)) {
                    continue;
                }
                for character in cluster.chars().filter(|&c| is_visible_character(c)) {
                    if !characters.contains(&character) {
                        characters.push(character);
                    }
                }
            }
        }
        characters
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
