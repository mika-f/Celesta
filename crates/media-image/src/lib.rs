//! Celesta's built-in image components: the `builtin` plugin's video
//! encoders `png`, `jpeg`, and `webp`, and its muxers `image` (one frame,
//! one file) and `imageSequence` (a file per frame, from a pattern). They run
//! in Celesta's own process through the same [`MediaProvider`] traits as a
//! plugin's components, so a still needs no media plugin and no process.
//!
//! The encoders write a whole image file per frame, as one packet; the
//! muxers write each packet's bytes to its file. With default settings they
//! write the bytes the exporter's still export wrote before them. See the
//! media components spec
//! (`docs/superpowers/specs/2026-11-08-media-components-design.md`, Stills
//! and sequences).

mod encoders;
mod muxers;

use celesta_media::{
    ComponentDescription, ComponentKind, MediaError, MediaProvider, Muxer, RegisteredPlugin,
    SessionId, VideoEncoder,
};
use celesta_plugin_host::{
    CELESTA_VERSION, MediaManifest, MediaMuxerEntry, MediaOrigin, MediaVideoEncoderEntry,
    RESERVED_PLUGIN_ID,
};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The PNG encoder's id, which is also its codec (and family).
pub const PNG: &str = "png";

/// The JPEG encoder's id, which is also its codec (and family).
pub const JPEG: &str = "jpeg";

/// The WebP encoder's id, which is also its codec (and family).
pub const WEBP: &str = "webp";

/// The id of the muxer that writes one frame to one file.
pub const IMAGE: &str = "image";

/// The id of the muxer that writes a file per frame from a pattern.
pub const IMAGE_SEQUENCE: &str = "imageSequence";

/// The extensions both muxers write.
pub const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// The codec families both muxers take, which are the encoders' ids.
const CODECS: [&str; 3] = [PNG, JPEG, WEBP];

/// The template a sequence's pattern holds once, in its last component.
const FRAME_FIELD: &str = "%06d";

/// The built-in components, as a manifest declares a plugin's.
pub fn manifest() -> MediaManifest {
    let encoder = |id: &str, name: &str| MediaVideoEncoderEntry {
        id: id.to_owned(),
        name: name.to_owned(),
        codecs: vec![id.to_owned()],
        input_formats: Some(
            encoders::input_formats(id)
                .iter()
                .map(ToString::to_string)
                .collect(),
        ),
    };
    let muxer = |id: &str, name: &str, sequence| MediaMuxerEntry {
        id: id.to_owned(),
        name: name.to_owned(),
        extensions: EXTENSIONS.map(str::to_owned).to_vec(),
        video: CODECS.map(str::to_owned).to_vec(),
        audio: None,
        default_video: Some(PNG.to_owned()),
        default_audio: None,
        trims_leading_audio: false,
        sequence,
    };
    MediaManifest {
        priority: 0,
        decoders: Vec::new(),
        video_encoders: vec![
            encoder(PNG, "PNG"),
            encoder(JPEG, "JPEG"),
            encoder(WEBP, "WebP"),
        ],
        audio_encoders: Vec::new(),
        muxers: vec![
            muxer(IMAGE, "Image", false),
            muxer(IMAGE_SEQUENCE, "Image sequence", true),
        ],
    }
}

/// The built-in components as the `builtin` media plugin, made in this
/// process: its factory ignores the kill switch and what it is made for.
pub fn registered() -> RegisteredPlugin {
    RegisteredPlugin {
        id: RESERVED_PLUGIN_ID.to_owned(),
        name: "Built-in".to_owned(),
        version: CELESTA_VERSION.to_owned(),
        origin: MediaOrigin::BuiltIn,
        directory: PathBuf::new(),
        executable: None,
        media: manifest(),
        factory: Arc::new(
            |_, _| Ok(Box::new(ImageComponents::default()) as Box<dyn MediaProvider>),
        ),
    }
}

/// The built-in components' provider: its open encoders and muxers.
#[derive(Default)]
pub struct ImageComponents {
    encoders: HashMap<SessionId, encoders::Session>,
    muxers: HashMap<SessionId, muxers::Session>,
    /// The last session number given, one count for every kind.
    last: u64,
}

impl ImageComponents {
    fn next_session(&mut self) -> SessionId {
        self.last += 1;
        SessionId(self.last)
    }
}

impl MediaProvider for ImageComponents {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        let component = |kind, id: &str| ComponentDescription {
            kind,
            id: id.to_owned(),
            available: true,
            reason: None,
            settings: Vec::new(),
            input_formats: Vec::new(),
            transports: Vec::new(),
            sample_rates: None,
            channels: Vec::new(),
        };
        let encoder = |id: &str| ComponentDescription {
            settings: encoders::settings(id),
            input_formats: encoders::input_formats(id).to_vec(),
            ..component(ComponentKind::VideoEncoder, id)
        };
        Ok(vec![
            encoder(PNG),
            encoder(JPEG),
            encoder(WEBP),
            component(ComponentKind::Muxer, IMAGE),
            component(ComponentKind::Muxer, IMAGE_SEQUENCE),
        ])
    }

    fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> {
        Some(self)
    }

    fn muxer(&mut self) -> Option<&mut dyn Muxer> {
        Some(self)
    }
}

/// The built-in encoder (and codec family) for `extension`, ignoring case:
/// png, jpg|jpeg → jpeg, webp.
pub fn codec_for_extension(extension: &str) -> Option<&'static str> {
    let is = |name: &str| extension.eq_ignore_ascii_case(name);
    if is("png") {
        Some(PNG)
    } else if is("jpg") || is("jpeg") {
        Some(JPEG)
    } else if is("webp") {
        Some(WEBP)
    } else {
        None
    }
}

/// The file `pattern` names for `frame`: its last component's one `%06d`
/// replaced by the zero-padded frame number; the rest of the path is
/// literal. MediaError::Codec when the last component is not UTF-8 or does
/// not hold exactly one `%06d`.
pub fn sequence_path(pattern: &Path, frame: u64) -> Result<PathBuf, MediaError> {
    let refused =
        |why: &str| MediaError::Codec(format!("the sequence pattern {} {why}", pattern.display()));
    let name = pattern
        .file_name()
        .ok_or_else(|| refused("names no file"))?
        .to_str()
        .ok_or_else(|| refused("has a file name that is not UTF-8"))?;
    if name.matches(FRAME_FIELD).count() != 1 {
        return Err(refused("does not hold %06d once in its file name"));
    }
    let name = name.replacen(FRAME_FIELD, &format!("{frame:06}"), 1);
    Ok(pattern.with_file_name(OsString::from(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::settings::validate;
    use celesta_media::{PixelFormat, SettingKind, SettingValue, Settings};

    #[test]
    fn describes_every_component_with_its_settings() {
        let described = ImageComponents::default().describe().unwrap();
        let ids: Vec<(ComponentKind, &str)> = described
            .iter()
            .map(|component| (component.kind, component.id.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                (ComponentKind::VideoEncoder, "png"),
                (ComponentKind::VideoEncoder, "jpeg"),
                (ComponentKind::VideoEncoder, "webp"),
                (ComponentKind::Muxer, "image"),
                (ComponentKind::Muxer, "imageSequence"),
            ]
        );
        let find = |id: &str| described.iter().find(|c| c.id == id).unwrap();
        for component in &described {
            assert!(component.available, "{component:?}");
            assert_eq!(component.reason, None);
            assert!(component.transports.is_empty(), "{component:?}");
            // Every field's default passes, and an empty map comes back as
            // the defaults.
            let defaults: Settings = component
                .settings
                .iter()
                .map(|field| (field.key.clone(), field.default.clone()))
                .collect();
            assert_eq!(
                validate(&component.settings, &Settings::new()).unwrap(),
                defaults,
                "{}",
                component.id
            );
            assert_eq!(
                validate(&component.settings, &defaults).unwrap(),
                defaults,
                "{}",
                component.id
            );
        }
        let keys = |id: &str| -> Vec<(String, SettingValue)> {
            find(id)
                .settings
                .iter()
                .map(|field| (field.key.clone(), field.default.clone()))
                .collect()
        };
        let text = |value: &str| SettingValue::String(value.to_owned());
        assert_eq!(
            keys("png"),
            [
                ("compression".to_owned(), text("balanced")),
                ("depth".to_owned(), text("8")),
                ("transparent".to_owned(), SettingValue::Boolean(false)),
            ]
        );
        assert_eq!(
            keys("jpeg"),
            [("quality".to_owned(), SettingValue::Number(90.0))]
        );
        assert_eq!(
            keys("webp"),
            [
                ("lossless".to_owned(), SettingValue::Boolean(false)),
                ("quality".to_owned(), SettingValue::Number(90.0)),
                ("transparent".to_owned(), SettingValue::Boolean(false)),
            ]
        );
        let png = find("png");
        let options = |key: &str| match &png.settings.iter().find(|f| f.key == key).unwrap().kind {
            SettingKind::Select { options } => options
                .iter()
                .map(|option| (option.value.clone(), option.label.clone()))
                .collect::<Vec<_>>(),
            kind => panic!("{key} is {kind:?}"),
        };
        let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter()
                .map(|(value, label)| (value.to_string(), label.to_string()))
                .collect()
        };
        assert_eq!(
            options("compression"),
            pairs(&[
                ("none", "None"),
                ("fastest", "Fastest"),
                ("fast", "Fast"),
                ("balanced", "Balanced"),
                ("high", "High"),
            ])
        );
        assert_eq!(
            options("depth"),
            pairs(&[("8", "8 bits per channel"), ("16", "16 bits per channel")])
        );
        let quality = find("jpeg").settings[0].clone();
        assert_eq!(quality.label, "Quality");
        assert_eq!(
            quality.description.as_deref(),
            Some("Higher is sharper and larger.")
        );
        assert_eq!(
            quality.kind,
            SettingKind::Number {
                min: Some(1.0),
                max: Some(100.0),
                step: Some(1.0),
                integer: true,
                unit: None,
            }
        );
        let webp_quality = &find("webp").settings[1];
        assert_eq!(webp_quality.kind, quality.kind);
        assert_eq!(
            webp_quality.visible_when,
            [("lossless".to_owned(), vec![SettingValue::Boolean(false)])].into()
        );
        assert_eq!(
            find("png").input_formats,
            [PixelFormat::Rgba16, PixelFormat::Rgba8]
        );
        assert_eq!(find("jpeg").input_formats, [PixelFormat::Rgba8]);
        assert_eq!(find("webp").input_formats, [PixelFormat::Rgba8]);
        for muxer in ["image", "imageSequence"] {
            assert!(find(muxer).settings.is_empty());
            assert!(find(muxer).input_formats.is_empty());
        }
    }

    #[test]
    fn registers_as_the_builtin_plugin() {
        let plugin = registered();
        assert_eq!(plugin.id, "builtin");
        assert_eq!(plugin.version, CELESTA_VERSION);
        assert_eq!(plugin.origin, MediaOrigin::BuiltIn);
        assert_eq!(plugin.directory, PathBuf::new());
        assert_eq!(plugin.executable, None);
        let media = &plugin.media;
        assert!(media.decoders.is_empty() && media.audio_encoders.is_empty());
        let encoders: Vec<(&str, &str, &[String])> = media
            .video_encoders
            .iter()
            .map(|e| (e.id.as_str(), e.name.as_str(), e.codecs.as_slice()))
            .collect();
        assert_eq!(
            encoders,
            [
                ("png", "PNG", &["png".to_owned()][..]),
                ("jpeg", "JPEG", &["jpeg".to_owned()][..]),
                ("webp", "WebP", &["webp".to_owned()][..]),
            ]
        );
        let names: Vec<(&str, &str, bool)> = media
            .muxers
            .iter()
            .map(|m| (m.id.as_str(), m.name.as_str(), m.sequence))
            .collect();
        assert_eq!(
            names,
            [
                ("image", "Image", false),
                ("imageSequence", "Image sequence", true)
            ]
        );
        for muxer in &media.muxers {
            assert_eq!(muxer.extensions, ["png", "jpg", "jpeg", "webp"]);
            assert_eq!(muxer.video, ["png", "jpeg", "webp"]);
            assert_eq!(muxer.default_video.as_deref(), Some("png"));
            assert_eq!(muxer.audio, None);
            assert_eq!(muxer.default_audio, None);
            assert!(!muxer.trims_leading_audio);
        }
        let mut provider = (plugin.factory)(
            &celesta_plugin_host::PluginKillSwitch::default(),
            celesta_media::ProviderUse::Describe,
        )
        .unwrap();
        assert_eq!(provider.describe().unwrap().len(), 5);
        assert!(provider.video_encoder().is_some() && provider.muxer().is_some());
        assert!(provider.decoder().is_none() && provider.audio_encoder().is_none());
        assert!(provider.pipeline().is_none());
    }

    #[test]
    fn the_pattern_is_only_the_last_component() {
        assert_eq!(
            sequence_path(Path::new("run%d/50%-x/frame-%06d.png"), 7).unwrap(),
            Path::new("run%d/50%-x/frame-000007.png")
        );
        assert_eq!(
            sequence_path(Path::new("frame-%06d.webp"), 1_234_567).unwrap(),
            Path::new("frame-1234567.webp")
        );
        for refused in ["run/frame.png", "run/%06d-%06d.png", "run/%6d.png"] {
            assert!(
                matches!(
                    sequence_path(Path::new(refused), 0),
                    Err(MediaError::Codec(_))
                ),
                "{refused}"
            );
        }
        #[cfg(unix)]
        {
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt;
            let not_utf8 = Path::new(OsStr::from_bytes(b"run/frame-\xff-%06d.png"));
            assert!(matches!(
                sequence_path(not_utf8, 0),
                Err(MediaError::Codec(_))
            ));
            let parent = Path::new(OsStr::from_bytes(b"run\xff%d/frame-%06d.png"));
            assert_eq!(
                sequence_path(parent, 42).unwrap().as_os_str().as_bytes(),
                b"run\xff%d/frame-000042.png"
            );
        }
    }

    #[test]
    fn codec_for_extension_follows_the_extension() {
        assert_eq!(codec_for_extension("PNG"), Some("png"));
        assert_eq!(codec_for_extension("png"), Some("png"));
        assert_eq!(codec_for_extension("jpg"), Some("jpeg"));
        assert_eq!(codec_for_extension("JPEG"), Some("jpeg"));
        assert_eq!(codec_for_extension("webp"), Some("webp"));
        assert_eq!(codec_for_extension("gif"), None);
        assert_eq!(codec_for_extension(""), None);
    }
}
