//! A sample content plugin: an image waving like a flag. The image is cut
//! into vertical strips, one mesh each, so every strip can be shaded on its
//! own (darker where the cloth turns away, lighter where it faces the
//! light). The left edge is the pole and stays put.
//!
//! What the layer gives the plugin:
//!
//! - asset `image`: a PNG or JPEG;
//! - `time`: seconds on the layer's clock;
//! - `data`: `{ amplitude, waves, speed, shading }`, all optional (see
//!   [`Wave`]).
//!
//! A frame depends on nothing else, as Celesta requires: the wave is a
//! function of the time.

use celesta_plugin::{
    ContentFrame, ContentProvider, MeshBlend, MeshFrame, MeshInfo, MeshModel, MeshState, ModelId,
    Opened, PluginError, TextureSource,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::f32::consts::TAU;
use std::path::PathBuf;

/// The plugin's id, as its manifest and its layers name it.
pub const PLUGIN: &str = "flag";

/// Strips across the image, and rows down each strip.
const STRIPS: usize = 48;
const ROWS: usize = 12;
/// Room above and below the image for the wave, as a fraction of its height.
const PADDING: f32 = 0.15;

/// How the flag waves, from the layer's `data`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wave {
    /// How far the free edge swings, as a fraction of the image's height, up
    /// to 0.15. Default 0.06.
    pub amplitude: f32,
    /// Wavelengths across the image. Default 1.5.
    pub waves: f32,
    /// Waves passing a point each second. Default 0.8.
    pub speed: f32,
    /// How much the slope darkens and lightens the strips, 0 to 1. Default 0.3.
    pub shading: f32,
}

impl Wave {
    fn from_data(data: &Value) -> Self {
        let number =
            |key: &str, default: f32| data[key].as_f64().map_or(default, |value| value as f32);
        Self {
            amplitude: number("amplitude", 0.06).clamp(0.0, PADDING),
            waves: number("waves", 1.5),
            speed: number("speed", 0.8),
            shading: number("shading", 0.3).clamp(0.0, 1.0),
        }
    }
}

struct Flag {
    width: f32,
    height: f32,
}

impl Flag {
    /// How far down the cloth is pushed at `u` (0 at the pole, 1 at the free
    /// edge), in pixels, and its slope there.
    fn displacement(&self, wave: &Wave, u: f32, time: f32) -> (f32, f32) {
        let phase = TAU * (wave.waves * u - wave.speed * time);
        let swing = wave.amplitude * self.height;
        // The swing grows from nothing at the pole.
        let offset = swing * u * phase.sin();
        let slope = swing * (phase.sin() + u * TAU * wave.waves * phase.cos()) / self.width;
        (offset, slope)
    }
}

/// The plugin's [`ContentProvider`]: what its process serves, and what tests
/// can call in-process.
#[derive(Default)]
pub struct FlagProvider {
    flags: HashMap<ModelId, Flag>,
    next: u64,
}

fn failed(message: impl Into<String>) -> PluginError {
    PluginError::Failed {
        plugin: PLUGIN.into(),
        message: message.into(),
    }
}

/// One strip's quads, top to bottom: vertex `2 * row + side`, side 0 on the
/// left. Counter-clockwise as seen on the canvas, so the strip faces front.
fn strip_indices() -> Vec<u32> {
    (0..ROWS as u32)
        .flat_map(|row| {
            let (top_left, top_right) = (2 * row, 2 * row + 1);
            let (bottom_left, bottom_right) = (top_left + 2, top_right + 2);
            [
                top_left,
                bottom_left,
                bottom_right,
                top_left,
                bottom_right,
                top_right,
            ]
        })
        .collect()
}

impl ContentProvider for FlagProvider {
    fn open(
        &mut self,
        plugin: &str,
        assets: &BTreeMap<String, PathBuf>,
    ) -> Result<Opened, PluginError> {
        if plugin != PLUGIN {
            return Err(PluginError::UnknownPlugin(plugin.to_owned()));
        }
        let image = assets
            .get("image")
            .ok_or_else(|| failed("the layer has no `image` asset"))?;
        let (width, height) = image::image_dimensions(image)
            .map_err(|error| failed(format!("could not read {}: {error}", image.display())))?;
        let (width, height) = (width as f32, height as f32);

        let indices = strip_indices();
        let meshes = (0..STRIPS)
            .map(|strip| {
                let (left, right) = (
                    strip as f32 / STRIPS as f32,
                    (strip + 1) as f32 / STRIPS as f32,
                );
                MeshInfo {
                    texture: 0,
                    blend: MeshBlend::Normal,
                    double_sided: false,
                    mask: None,
                    // Texture coordinates count up from the bottom row.
                    uvs: (0..=ROWS)
                        .flat_map(|row| {
                            let v = 1.0 - row as f32 / ROWS as f32;
                            [[left, v], [right, v]]
                        })
                        .collect(),
                    indices: indices.clone(),
                }
            })
            .collect();
        self.next += 1;
        let id = ModelId(self.next);
        self.flags.insert(id, Flag { width, height });
        let model = MeshModel {
            canvas: [width, height * (1.0 + 2.0 * PADDING)],
            // Celesta decodes the file itself.
            textures: vec![TextureSource::Path(image.clone())],
            meshes,
        };
        Ok(Opened {
            model: id,
            content: model.into(),
            notices: vec![],
        })
    }

    fn frame(
        &mut self,
        model: ModelId,
        time: f64,
        data: &Value,
    ) -> Result<ContentFrame, PluginError> {
        let flag = self
            .flags
            .get(&model)
            .ok_or_else(|| failed(format!("no model {}", model.0)))?;
        let wave = Wave::from_data(data);
        let time = time as f32;
        let top = flag.height * PADDING;
        let meshes = (0..STRIPS)
            .map(|strip| {
                let edges = [strip, strip + 1].map(|edge| edge as f32 / STRIPS as f32);
                let offsets = edges.map(|u| flag.displacement(&wave, u, time).0);
                let positions = (0..=ROWS)
                    .flat_map(|row| {
                        let y = top + flag.height * row as f32 / ROWS as f32;
                        [0, 1].map(|side| [edges[side] * flag.width, y + offsets[side]])
                    })
                    .collect();
                // Cloth sloping down to the right turns away from a light
                // on the left and darkens; sloping up, it catches the light.
                let (_, slope) = flag.displacement(&wave, (edges[0] + edges[1]) / 2.0, time);
                let light = (slope * 2.5).clamp(-1.0, 1.0) * wave.shading;
                let dark = 1.0 - light.max(0.0);
                let bright = (-light).max(0.0) * 0.5;
                MeshState {
                    index: strip,
                    visible: true,
                    opacity: 1.0,
                    multiply: [dark; 3],
                    screen: [bright; 3],
                    positions,
                }
            })
            .collect();
        Ok(MeshFrame { meshes }.into())
    }

    fn close(&mut self, model: ModelId) {
        self.flags.remove(&model);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_plugin::ContentModel;

    fn opened(provider: &mut FlagProvider) -> (ModelId, MeshModel, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flag.png");
        image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let opened = provider
            .open(PLUGIN, &BTreeMap::from([("image".to_owned(), path)]))
            .unwrap();
        let ContentModel::Meshes(model) = opened.content else {
            panic!("a flag is meshes");
        };
        (opened.model, model, directory)
    }

    fn meshes(frame: ContentFrame) -> MeshFrame {
        let ContentFrame::Meshes(frame) = frame else {
            panic!("a flag is meshes");
        };
        frame
    }

    #[test]
    fn the_canvas_has_room_for_the_wave() {
        let (_, model, _directory) = opened(&mut FlagProvider::default());
        assert_eq!(model.canvas, [200.0, 130.0]);
        assert_eq!(model.meshes.len(), STRIPS);
        assert_eq!(model.meshes[0].uvs.len(), 2 * (ROWS + 1));
    }

    #[test]
    fn the_pole_stays_and_the_free_edge_waves_with_time() {
        let mut provider = FlagProvider::default();
        let (id, _, _directory) = opened(&mut provider);
        let data = serde_json::json!({ "amplitude": 0.1 });
        let at =
            |provider: &mut FlagProvider, time| meshes(provider.frame(id, time, &data).unwrap());
        let (first, later) = (at(&mut provider, 0.0), at(&mut provider, 0.3));
        // The pole's top vertex, then the free edge's.
        let pole = |frame: &MeshFrame| frame.meshes[0].positions[0];
        for frame in [&first, &later] {
            assert!(pole(frame)[0] == 0.0 && (pole(frame)[1] - 15.0).abs() < 1e-4);
        }
        let edge = |frame: &MeshFrame| frame.meshes[STRIPS - 1].positions[1];
        assert_ne!(edge(&first), edge(&later));
        assert!((edge(&later)[1] - 15.0).abs() <= 10.0 + 1e-3);
        // Asked again, a time gives the same frame.
        assert_eq!(at(&mut provider, 0.3), later);
    }

    #[test]
    fn needs_an_image() {
        let error = FlagProvider::default()
            .open(PLUGIN, &BTreeMap::new())
            .unwrap_err();
        assert!(error.to_string().contains("`image`"), "{error}");
    }
}
