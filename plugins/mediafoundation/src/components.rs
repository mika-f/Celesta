//! The plugin's components: their ids, settings schemas, and what
//! `describe` says of each, from the encoders `MFTEnumEx` lists. Nothing
//! here calls Media Foundation, so it builds and is tested on every system;
//! the lists come from `mf/enumerate.rs` on Windows.

use crate::DECODER_ID;
use celesta_media::{
    ComponentDescription, ComponentKind, PixelFormat, SampleRates, SelectOption, SettingField,
    SettingKind, SettingValue,
};

/// A video encoder component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoCodec {
    H264,
    Hevc,
}

impl VideoCodec {
    pub(crate) const ALL: [Self; 2] = [Self::H264, Self::Hevc];

    /// The component's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|codec| codec.id() == id)
    }

    /// Every format it takes under any settings, best first, as the
    /// manifest's `inputFormats` lists them.
    pub(crate) fn input_formats(self) -> Vec<PixelFormat> {
        match self {
            Self::H264 => vec![PixelFormat::Nv12, PixelFormat::Rgba8],
            Self::Hevc => vec![
                PixelFormat::Nv12,
                PixelFormat::P010,
                PixelFormat::Rgba16,
                PixelFormat::Rgba8,
            ],
        }
    }

    /// Its settings schema.
    pub(crate) fn settings(self) -> Vec<SettingField> {
        match self {
            Self::H264 => {
                let mut settings = vec![select(
                    "profile",
                    "Profile",
                    &[("high", "High"), ("main", "Main"), ("baseline", "Baseline")],
                    "high",
                )];
                settings.extend(compression_settings());
                settings.push(hardware_setting());
                settings
            }
            Self::Hevc => {
                let mut settings = compression_settings();
                settings.push(select(
                    "bitDepth",
                    "Bit depth",
                    &[("8", "8-bit (Main)"), ("10", "10-bit (Main 10)")],
                    "8",
                ));
                settings.push(hardware_setting());
                settings
            }
        }
    }

    /// Why `describe` calls it unavailable when `MFTEnumEx` lists no encoder
    /// for it.
    const fn missing_reason(self) -> &'static str {
        match self {
            Self::H264 => "Media Foundation has no H.264 encoder on this system",
            Self::Hevc => {
                "Media Foundation has no HEVC encoder on this system; install a GPU driver with one or the HEVC Video Extensions"
            }
        }
    }
}

/// An audio encoder component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioCodec {
    /// AAC-LC.
    Aac,
}

impl AudioCodec {
    pub(crate) const ALL: [Self; 1] = [Self::Aac];

    /// The component's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Aac => "aac",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|codec| codec.id() == id)
    }

    /// Its settings schema.
    pub(crate) fn settings(self) -> Vec<SettingField> {
        match self {
            Self::Aac => vec![select(
                "bitrate",
                "Bitrate",
                &[
                    ("96000", "96 kbit/s"),
                    ("128000", "128 kbit/s"),
                    ("160000", "160 kbit/s"),
                    ("192000", "192 kbit/s"),
                ],
                "192000",
            )],
        }
    }
}

/// Which encoders `MFTEnumEx` lists on this system: what `describe` needs
/// of Media Foundation besides whether it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Encoders {
    /// An NV12 to H.264 encoder.
    pub h264: bool,
    /// An NV12 to HEVC encoder.
    pub hevc: bool,
    /// A PCM to AAC encoder.
    pub aac: bool,
}

/// What `describe` answers, from whether Media Foundation started
/// (`startup`, the failure when not) and the `encoders` it lists: every
/// component the manifest declares, in its order. The decoder is available
/// whenever Media Foundation started. The plugin has no muxer.
pub(crate) fn describe_from(
    startup: Result<(), String>,
    encoders: &Encoders,
) -> Vec<ComponentDescription> {
    let mut components = Vec::new();
    components.push(match &startup {
        Ok(()) => available(ComponentKind::Decoder, DECODER_ID),
        Err(error) => not_started(ComponentKind::Decoder, DECODER_ID, error),
    });
    components.extend(VideoCodec::ALL.map(|codec| {
        let kind = ComponentKind::VideoEncoder;
        let listed = match codec {
            VideoCodec::H264 => encoders.h264,
            VideoCodec::Hevc => encoders.hevc,
        };
        match &startup {
            Err(error) => not_started(kind, codec.id(), error),
            Ok(()) if listed => ComponentDescription {
                settings: codec.settings(),
                input_formats: codec.input_formats(),
                transports: vec!["sharedMemory".to_owned(), "pipe".to_owned()],
                ..available(kind, codec.id())
            },
            Ok(()) => ComponentDescription::unavailable(kind, codec.id(), codec.missing_reason()),
        }
    }));
    components.extend(AudioCodec::ALL.map(|codec| {
        let kind = ComponentKind::AudioEncoder;
        match &startup {
            Err(error) => not_started(kind, codec.id(), error),
            Ok(()) if encoders.aac => ComponentDescription {
                settings: codec.settings(),
                sample_rates: Some(SampleRates::Only(vec![44_100, 48_000])),
                channels: vec![1, 2],
                ..available(kind, codec.id())
            },
            Ok(()) => ComponentDescription::unavailable(
                kind,
                codec.id(),
                "Media Foundation has no AAC encoder on this system",
            ),
        }
    }));
    components
}

/// `kind` `id` as unavailable because `MFStartup` failed with `error`.
fn not_started(kind: ComponentKind, id: &str, error: &str) -> ComponentDescription {
    ComponentDescription::unavailable(
        kind,
        id,
        format!(
            "Media Foundation is not available on this system ({error}); on Windows Server add the Media Foundation feature, on an N edition install the Media Feature Pack"
        ),
    )
}

/// `kind` `id` as available, with nothing else described.
fn available(kind: ComponentKind, id: &str) -> ComponentDescription {
    ComponentDescription {
        available: true,
        reason: None,
        ..ComponentDescription::unavailable(kind, id, "")
    }
}

/// What H.264 and HEVC share after H.264's profile: rate control, the
/// keyframe interval, and B-frames.
fn compression_settings() -> Vec<SettingField> {
    let rate_control = in_group(
        select(
            "rateControl",
            "Rate control",
            &[
                ("quality", "Constant quality"),
                ("bitrate", "Average bitrate"),
            ],
            "quality",
        ),
        "Rate control",
    );
    let mut quality = when(
        in_group(
            integer("quality", "Quality", 0.0, 100.0, 70.0),
            "Rate control",
        ),
        "rateControl",
        "quality",
    );
    quality.description = Some("Higher is better quality and larger.".to_owned());
    let bitrate = when(
        in_group(
            field(
                "bitrate",
                "Bitrate",
                SettingValue::Number(8_000_000.0),
                SettingKind::Number {
                    min: Some(100_000.0),
                    max: None,
                    step: None,
                    integer: true,
                    unit: Some("bit/s".to_owned()),
                },
            ),
            "Rate control",
        ),
        "rateControl",
        "bitrate",
    );
    let mut keyframe_interval =
        integer("keyframeInterval", "Keyframe interval", 1.0, 1000.0, 250.0);
    keyframe_interval.advanced = true;
    keyframe_interval.description = Some("The most frames between keyframes.".to_owned());
    let mut b_frames = field(
        "bFrames",
        "B-frames",
        SettingValue::Boolean(true),
        SettingKind::Boolean,
    );
    b_frames.advanced = true;
    b_frames.description = Some(
        "Reordered frames compress better; turn off for players that cannot decode them."
            .to_owned(),
    );
    vec![rate_control, quality, bitrate, keyframe_interval, b_frames]
}

/// Whether to use the GPU's encoder.
fn hardware_setting() -> SettingField {
    let mut hardware = field(
        "hardware",
        "Hardware encoder",
        SettingValue::Boolean(true),
        SettingKind::Boolean,
    );
    hardware.advanced = true;
    hardware.description = Some(
        "Use the GPU's encoder when Windows has one; off uses Microsoft's software encoder."
            .to_owned(),
    );
    hardware
}

fn field(key: &str, label: &str, default: SettingValue, kind: SettingKind) -> SettingField {
    SettingField {
        key: key.to_owned(),
        label: label.to_owned(),
        default,
        kind,
        description: None,
        group: None,
        advanced: false,
        visible_when: Default::default(),
    }
}

/// A select of `(value, label)` options.
fn select(key: &str, label: &str, options: &[(&str, &str)], default: &str) -> SettingField {
    let options = options
        .iter()
        .map(|(value, label)| SelectOption {
            value: (*value).to_owned(),
            label: (*label).to_owned(),
        })
        .collect();
    field(
        key,
        label,
        SettingValue::String(default.to_owned()),
        SettingKind::Select { options },
    )
}

/// A whole number from `min` to `max` in steps of 1.
fn integer(key: &str, label: &str, min: f64, max: f64, default: f64) -> SettingField {
    field(
        key,
        label,
        SettingValue::Number(default),
        SettingKind::Number {
            min: Some(min),
            max: Some(max),
            step: Some(1.0),
            integer: true,
            unit: None,
        },
    )
}

/// `field` shown only while `key` is `value`.
fn when(mut field: SettingField, key: &str, value: &str) -> SettingField {
    field
        .visible_when
        .insert(key.to_owned(), vec![SettingValue::String(value.to_owned())]);
    field
}

fn in_group(mut field: SettingField, group: &str) -> SettingField {
    field.group = Some(group.to_owned());
    field
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::settings::validate;
    use celesta_media::{ComponentKind, SampleRates, SettingValue, Settings};
    use celesta_plugin_host::{
        MediaAudioEncoderEntry, MediaDecoderEntry, MediaManifest, MediaVideoEncoderEntry,
    };

    const EVERY_ENCODER: Encoders = Encoders {
        h264: true,
        hevc: true,
        aac: true,
    };

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|&value| value.to_owned()).collect()
    }

    fn manifest() -> MediaManifest {
        let plugin: serde_json::Value =
            serde_json::from_str(include_str!("../plugin.json")).unwrap();
        let (manifest, problems) = MediaManifest::from_json(plugin["media"].clone());
        assert!(problems.is_empty(), "{problems:?}");
        manifest
    }

    fn keys(settings: &[SettingField]) -> Vec<&str> {
        settings.iter().map(|field| field.key.as_str()).collect()
    }

    fn setting<'a>(settings: &'a [SettingField], key: &str) -> &'a SettingField {
        settings
            .iter()
            .find(|field| field.key == key)
            .unwrap_or_else(|| panic!("no {key}"))
    }

    fn text(value: &str) -> SettingValue {
        SettingValue::String(value.to_owned())
    }

    fn filled(settings: &[SettingField]) -> Settings {
        validate(settings, &Settings::new()).unwrap()
    }

    #[test]
    fn plugin_json_declares_every_component() {
        let manifest = manifest();
        assert_eq!(
            manifest.decoders,
            [MediaDecoderEntry {
                id: "mediafoundation".to_owned(),
                name: "Media Foundation".to_owned(),
                extensions: strings(&["mp4", "mov", "m4a"]),
            }]
        );
        let video =
            |id: &str, name: &str, codecs: &[&str], inputs: &[&str]| MediaVideoEncoderEntry {
                id: id.to_owned(),
                name: name.to_owned(),
                codecs: strings(codecs),
                input_formats: Some(strings(inputs)),
            };
        assert_eq!(
            manifest.video_encoders,
            [
                video(
                    "h264",
                    "H.264 (Media Foundation)",
                    &["avc1"],
                    &["nv12", "rgba8"]
                ),
                video(
                    "hevc",
                    "HEVC (Media Foundation)",
                    &["hvc1"],
                    &["nv12", "p010", "rgba16", "rgba8"]
                ),
            ]
        );
        assert_eq!(
            manifest.audio_encoders,
            [MediaAudioEncoderEntry {
                id: "aac".to_owned(),
                name: "AAC (Media Foundation)".to_owned(),
                codecs: strings(&["mp4a"]),
            }]
        );
        // Task 1 ruled that the plugin has no muxer.
        assert_eq!(manifest.muxers, []);
    }

    #[test]
    fn describe_matches_the_manifest() {
        let manifest = manifest();
        let described = describe_from(Ok(()), &EVERY_ENCODER);
        let expected: Vec<(ComponentKind, &str)> = []
            .into_iter()
            .chain(
                manifest
                    .decoders
                    .iter()
                    .map(|entry| (ComponentKind::Decoder, entry.id.as_str())),
            )
            .chain(
                manifest
                    .video_encoders
                    .iter()
                    .map(|entry| (ComponentKind::VideoEncoder, entry.id.as_str())),
            )
            .chain(
                manifest
                    .audio_encoders
                    .iter()
                    .map(|entry| (ComponentKind::AudioEncoder, entry.id.as_str())),
            )
            .collect();
        let listed: Vec<(ComponentKind, &str)> = described
            .iter()
            .map(|component| (component.kind, component.id.as_str()))
            .collect();
        assert_eq!(listed, expected);
        for component in &described {
            assert!(component.available, "{component:?}");
            assert_eq!(component.reason, None, "{component:?}");
        }
        for entry in &manifest.video_encoders {
            let component = described
                .iter()
                .find(|component| {
                    component.kind == ComponentKind::VideoEncoder && component.id == entry.id
                })
                .unwrap();
            assert_eq!(component.transports, ["sharedMemory", "pipe"]);
            let inputs: Vec<String> = component
                .input_formats
                .iter()
                .map(ToString::to_string)
                .collect();
            assert_eq!(Some(inputs), entry.input_formats, "{}", entry.id);
            assert_eq!(
                component.settings,
                VideoCodec::from_id(&entry.id).unwrap().settings()
            );
        }
        let aac = described
            .iter()
            .find(|component| component.kind == ComponentKind::AudioEncoder)
            .unwrap();
        assert_eq!(
            aac.sample_rates,
            Some(SampleRates::Only(vec![44_100, 48_000]))
        );
        assert_eq!(aac.channels, [1, 2]);
        assert_eq!(aac.settings, AudioCodec::Aac.settings());
    }

    #[test]
    fn a_missing_encoder_has_a_reason() {
        let described = describe_from(
            Ok(()),
            &Encoders {
                h264: true,
                hevc: false,
                aac: false,
            },
        );
        let reason = |kind: ComponentKind, id: &str| {
            let component = described
                .iter()
                .find(|component| component.kind == kind && component.id == id)
                .unwrap();
            assert_eq!(
                component.available,
                component.reason.is_none(),
                "{component:?}"
            );
            component.reason.clone()
        };
        assert_eq!(reason(ComponentKind::VideoEncoder, "h264"), None);
        assert_eq!(
            reason(ComponentKind::VideoEncoder, "hevc").as_deref(),
            Some(
                "Media Foundation has no HEVC encoder on this system; install a GPU driver with one or the HEVC Video Extensions"
            )
        );
        assert_eq!(
            reason(ComponentKind::AudioEncoder, "aac").as_deref(),
            Some("Media Foundation has no AAC encoder on this system")
        );
        assert_eq!(reason(ComponentKind::Decoder, "mediafoundation"), None);

        let no_h264 = describe_from(
            Ok(()),
            &Encoders {
                h264: false,
                ..EVERY_ENCODER
            },
        );
        let h264 = no_h264.iter().find(|component| component.id == "h264");
        assert_eq!(
            h264.and_then(|component| component.reason.as_deref()),
            Some("Media Foundation has no H.264 encoder on this system")
        );
    }

    #[test]
    fn a_failed_startup_makes_everything_unavailable() {
        let described = describe_from(Err("MFStartup failed: 0xC00D36E3".into()), &EVERY_ENCODER);
        assert_eq!(described.len(), 4);
        for component in &described {
            assert!(!component.available, "{component:?}");
            let reason = component.reason.as_deref().unwrap();
            assert!(reason.contains("MFStartup failed: 0xC00D36E3"), "{reason}");
            assert!(reason.contains("Media Feature Pack"), "{reason}");
        }
    }

    #[test]
    fn settings_follow_the_decisions() {
        assert_eq!(
            keys(&VideoCodec::H264.settings()),
            [
                "profile",
                "rateControl",
                "quality",
                "bitrate",
                "keyframeInterval",
                "bFrames",
                "hardware"
            ]
        );
        assert_eq!(
            keys(&VideoCodec::Hevc.settings()),
            [
                "rateControl",
                "quality",
                "bitrate",
                "keyframeInterval",
                "bFrames",
                "bitDepth",
                "hardware"
            ]
        );
        assert_eq!(keys(&AudioCodec::Aac.settings()), ["bitrate"]);

        let h264 = VideoCodec::H264.settings();
        let defaults = filled(&h264);
        assert_eq!(defaults["profile"], text("high"));
        assert_eq!(defaults["rateControl"], text("quality"));
        assert_eq!(defaults["quality"], SettingValue::Number(70.0));
        assert_eq!(defaults["keyframeInterval"], SettingValue::Number(250.0));
        assert_eq!(defaults["bFrames"], SettingValue::Boolean(true));
        assert_eq!(defaults["hardware"], SettingValue::Boolean(true));
        // `bitrate` is hidden while the rate control is quality.
        assert!(!defaults.contains_key("bitrate"));
        let by_bitrate: Settings = [("rateControl".to_owned(), text("bitrate"))]
            .into_iter()
            .collect();
        let by_bitrate = validate(&h264, &by_bitrate).unwrap();
        assert_eq!(by_bitrate["bitrate"], SettingValue::Number(8_000_000.0));
        assert!(!by_bitrate.contains_key("quality"));

        let hevc = filled(&VideoCodec::Hevc.settings());
        assert_eq!(hevc["bitDepth"], text("8"));
        assert_eq!(hevc["hardware"], SettingValue::Boolean(true));
        assert_eq!(
            filled(&AudioCodec::Aac.settings())["bitrate"],
            text("192000")
        );

        let hardware = setting(&h264, "hardware");
        assert!(hardware.advanced);
        assert_eq!(hardware.label, "Hardware encoder");
        assert!(setting(&h264, "bFrames").advanced);
        assert!(setting(&h264, "keyframeInterval").advanced);
        assert_eq!(
            setting(&h264, "quality").group.as_deref(),
            Some("Rate control")
        );
    }

    #[test]
    fn ids_name_their_components() {
        for codec in VideoCodec::ALL {
            assert_eq!(VideoCodec::from_id(codec.id()), Some(codec));
        }
        for codec in AudioCodec::ALL {
            assert_eq!(AudioCodec::from_id(codec.id()), Some(codec));
        }
        assert_eq!(VideoCodec::ALL.map(VideoCodec::id), ["h264", "hevc"]);
        assert_eq!(VideoCodec::from_id("libx264"), None);
        assert_eq!(AudioCodec::from_id("opus"), None);
    }
}
