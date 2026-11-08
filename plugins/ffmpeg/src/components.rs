//! The plugin's components: their ids, settings schemas, and what
//! `describe` says of each, which depends on the encoders the linked
//! FFmpeg build has.

use crate::DECODER_ID;
use celesta_media::{
    ComponentDescription, ComponentKind, PixelFormat, SelectOption, SettingField, SettingKind,
    SettingValue,
};
use ffmpeg_sys_next::avcodec_find_encoder_by_name;
use std::ffi::CString;

/// Why the audio encoders and the muxers are unavailable until they are
/// ported to libavformat.
const NOT_YET: &str = "arrives with the muxers";

/// The audio encoders the manifest declares.
const AUDIO_ENCODERS: [&str; 3] = ["aac", "opus", "pcm"];

/// The muxers the manifest declares.
const MUXERS: [&str; 3] = ["mp4", "mov", "webm"];

/// The x264 presets, fastest first.
const X264_PRESETS: [&str; 9] = [
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

/// A video encoder component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoCodec {
    /// H.264 through libx264.
    X264,
    /// ProRes 4444 or 422 HQ through `prores_ks`.
    ProRes,
    /// VP9 through libvpx.
    Vp9,
}

impl VideoCodec {
    pub(crate) const ALL: [Self; 3] = [Self::X264, Self::ProRes, Self::Vp9];

    /// The component's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::X264 => "libx264",
            Self::ProRes => "prores",
            Self::Vp9 => "vp9",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|codec| codec.id() == id)
    }

    /// The libavcodec encoder it runs.
    pub(crate) const fn encoder(self) -> &'static str {
        match self {
            Self::X264 => "libx264",
            Self::ProRes => "prores_ks",
            Self::Vp9 => "libvpx-vp9",
        }
    }

    /// Every format it takes under any settings, best first, as the
    /// manifest's `inputFormats` lists them.
    pub(crate) fn input_formats(self) -> Vec<PixelFormat> {
        match self {
            Self::X264 => vec![
                PixelFormat::Yuv420p,
                PixelFormat::Yuv420p10,
                PixelFormat::Rgba16,
                PixelFormat::Rgba8,
            ],
            Self::ProRes | Self::Vp9 => vec![PixelFormat::Rgba16],
        }
    }

    /// Its settings schema.
    pub(crate) fn settings(self) -> Vec<SettingField> {
        match self {
            Self::X264 => x264_settings(),
            Self::ProRes => vec![select(
                "profile",
                "Profile",
                &[("4444", "4444"), ("hq", "422 HQ")],
                "4444",
            )],
            Self::Vp9 => vec![
                integer("crf", "CRF", 0.0, 63.0, 20.0),
                field(
                    "alpha",
                    "Keep alpha",
                    SettingValue::Boolean(true),
                    SettingKind::Boolean,
                ),
            ],
        }
    }
}

/// `Ok` when the linked FFmpeg has `encoder`, else why not.
pub(crate) fn availability(encoder: &str) -> Result<(), String> {
    let missing = || format!("this FFmpeg build has no {encoder}");
    let name = CString::new(encoder).map_err(|_| missing())?;
    // SAFETY: `name` is a NUL-terminated string that outlives the call.
    if unsafe { avcodec_find_encoder_by_name(name.as_ptr()) }.is_null() {
        Err(missing())
    } else {
        Ok(())
    }
}

/// What `describe` answers: every component the manifest declares.
pub(crate) fn describe() -> Vec<ComponentDescription> {
    let mut components = vec![ComponentDescription {
        available: true,
        reason: None,
        ..ComponentDescription::unavailable(ComponentKind::Decoder, DECODER_ID, "")
    }];
    components.extend(
        VideoCodec::ALL.map(|codec| match availability(codec.encoder()) {
            Ok(()) => ComponentDescription {
                available: true,
                reason: None,
                settings: codec.settings(),
                input_formats: codec.input_formats(),
                transports: vec!["pipe".to_owned()],
                ..ComponentDescription::unavailable(ComponentKind::VideoEncoder, codec.id(), "")
            },
            Err(reason) => {
                ComponentDescription::unavailable(ComponentKind::VideoEncoder, codec.id(), reason)
            }
        }),
    );
    components.extend(
        AUDIO_ENCODERS
            .map(|id| ComponentDescription::unavailable(ComponentKind::AudioEncoder, id, NOT_YET)),
    );
    components.extend(
        MUXERS.map(|id| ComponentDescription::unavailable(ComponentKind::Muxer, id, NOT_YET)),
    );
    components
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

fn x264_settings() -> Vec<SettingField> {
    let options = X264_PRESETS
        .iter()
        .map(|&preset| {
            let mut label = preset.to_owned();
            label[..1].make_ascii_uppercase();
            SelectOption {
                value: preset.to_owned(),
                label,
            }
        })
        .collect();
    let mut preset = field(
        "preset",
        "Preset",
        SettingValue::String("medium".to_owned()),
        SettingKind::Select { options },
    );
    preset.description = Some("Slower presets make smaller files at the same quality.".to_owned());
    let rate_control = in_group(
        select(
            "rateControl",
            "Rate control",
            &[
                ("crf", "Constant quality (CRF)"),
                ("vbr", "Average bitrate"),
            ],
            "crf",
        ),
        "Rate control",
    );
    let mut crf = when(
        in_group(integer("crf", "CRF", 0.0, 51.0, 18.0), "Rate control"),
        "rateControl",
        "crf",
    );
    crf.description = Some("Lower is higher quality and larger.".to_owned());
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
        "vbr",
    );
    let mut bit_depth = select(
        "bitDepth",
        "Bit depth",
        &[("8", "8-bit"), ("10", "10-bit (High 10)")],
        "8",
    );
    bit_depth.description = Some("10-bit H.264 plays in fewer players.".to_owned());
    let mut tune = field(
        "tune",
        "Tune",
        SettingValue::String(String::new()),
        SettingKind::String,
    );
    tune.advanced = true;
    tune.description = Some("An x264 tune, such as film or animation.".to_owned());
    vec![preset, rate_control, crf, bitrate, bit_depth, tune]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_encoder_has_a_reason() {
        assert_eq!(
            availability("no_such_encoder"),
            Err("this FFmpeg build has no no_such_encoder".to_owned())
        );
        assert_eq!(availability("libx264"), Ok(()));
    }

    #[test]
    fn x264_presets_are_capitalized() {
        let SettingKind::Select { options } = &x264_settings()[0].kind else {
            panic!("preset is a select");
        };
        assert_eq!(options.len(), 9);
        assert_eq!(options[0].label, "Ultrafast");
        assert_eq!(options[8].value, "veryslow");
    }
}
