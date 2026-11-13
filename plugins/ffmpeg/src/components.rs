//! The plugin's components: their ids, settings schemas, and what
//! `describe` says of each, which depends on the encoders the linked
//! FFmpeg build has.

use crate::{DECODER_ID, Profile};
use celesta_media::{
    ComponentDescription, ComponentKind, PixelFormat, SampleRates, SelectOption, SettingField,
    SettingKind, SettingValue,
};
use ffmpeg_sys_next::{
    AVCodecConfig, av_guess_format, avcodec_find_encoder_by_name, avcodec_get_supported_config,
};
use std::ffi::{CString, c_int, c_void};
use std::ptr::{null, null_mut};

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

    /// The build that serves it: a GPL encoder needs [`Profile::X264`]'s
    /// program (or one of its own), never the LGPL one.
    pub(crate) const fn profile(self) -> Profile {
        match self {
            Self::X264 => Profile::X264,
            Self::ProRes | Self::Vp9 => Profile::Standard,
        }
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
                PixelFormat::Nv12,
                PixelFormat::P010,
                PixelFormat::Rgba8,
                PixelFormat::Rgba16,
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

/// An audio encoder component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioCodec {
    /// AAC-LC through FFmpeg's own `aac` encoder.
    Aac,
    /// Opus through libopus.
    Opus,
    /// Uncompressed PCM, packed by the plugin itself.
    Pcm,
}

impl AudioCodec {
    pub(crate) const ALL: [Self; 3] = [Self::Aac, Self::Opus, Self::Pcm];

    /// The component's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Opus => "opus",
            Self::Pcm => "pcm",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|codec| codec.id() == id)
    }

    /// The build that serves it.
    pub(crate) const fn profile(self) -> Profile {
        match self {
            Self::Aac | Self::Opus | Self::Pcm => Profile::Standard,
        }
    }

    /// The libavcodec encoder it runs; none for PCM.
    pub(crate) const fn encoder(self) -> Option<&'static str> {
        match self {
            Self::Aac => Some("aac"),
            Self::Opus => Some("libopus"),
            Self::Pcm => None,
        }
    }

    /// `Ok` when the linked FFmpeg has its encoder, else why not.
    pub(crate) fn availability(self) -> Result<(), String> {
        self.encoder().map_or(Ok(()), availability)
    }

    /// The sample rates it takes.
    pub(crate) fn sample_rates(self) -> SampleRates {
        match self {
            Self::Aac => SampleRates::Only(supported_sample_rates("aac")),
            Self::Opus => SampleRates::Only(vec![48_000]),
            Self::Pcm => SampleRates::Any,
        }
    }

    /// Its settings schema.
    pub(crate) fn settings(self) -> Vec<SettingField> {
        match self {
            Self::Aac | Self::Opus => vec![field(
                "bitrate",
                "Bitrate",
                SettingValue::Number(192_000.0),
                SettingKind::Number {
                    min: Some(32_000.0),
                    max: Some(512_000.0),
                    step: None,
                    integer: true,
                    unit: Some("bit/s".to_owned()),
                },
            )],
            Self::Pcm => vec![select(
                "format",
                "Sample format",
                &[
                    ("s16le", "16-bit integer"),
                    ("s24le", "24-bit integer"),
                    ("f32le", "32-bit float"),
                ],
                "s24le",
            )],
        }
    }
}

/// A muxer component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MuxerFormat {
    Mp4,
    Mov,
    Webm,
}

impl MuxerFormat {
    pub(crate) const ALL: [Self; 3] = [Self::Mp4, Self::Mov, Self::Webm];

    /// The component's id, which is also libavformat's name for it.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.id() == id)
    }

    /// The build that serves it.
    pub(crate) const fn profile(self) -> Profile {
        match self {
            Self::Mp4 | Self::Mov | Self::Webm => Profile::Standard,
        }
    }

    /// `Ok` when the linked FFmpeg has its muxer, else why not.
    pub(crate) fn availability(self) -> Result<(), String> {
        let name = CString::new(self.id()).expect("muxer names have no NUL");
        // SAFETY: `name` is NUL-terminated and outlives the call.
        if unsafe { av_guess_format(name.as_ptr(), null(), null()) }.is_null() {
            Err(format!("this FFmpeg build has no {} muxer", self.id()))
        } else {
            Ok(())
        }
    }

    /// Its settings schema.
    pub(crate) fn settings(self) -> Vec<SettingField> {
        match self {
            Self::Mp4 | Self::Mov => vec![field(
                "faststart",
                "Fast start",
                SettingValue::Boolean(true),
                SettingKind::Boolean,
            )],
            Self::Webm => Vec::new(),
        }
    }
}

/// The sample rates the linked FFmpeg's `encoder` takes; none when it does
/// not list them.
fn supported_sample_rates(encoder: &str) -> Vec<u32> {
    let Ok(name) = CString::new(encoder) else {
        return Vec::new();
    };
    // SAFETY: `name` outlives the call; the list libavcodec answers is
    // static, ends with 0, and has `count` rates before it.
    unsafe {
        let codec = avcodec_find_encoder_by_name(name.as_ptr());
        if codec.is_null() {
            return Vec::new();
        }
        let mut rates: *const c_void = null();
        let mut count: c_int = 0;
        let result = avcodec_get_supported_config(
            null_mut(),
            codec,
            AVCodecConfig::AV_CODEC_CONFIG_SAMPLE_RATE,
            0,
            &mut rates,
            &mut count,
        );
        match usize::try_from(count) {
            Ok(count) if result >= 0 && !rates.is_null() => {
                std::slice::from_raw_parts(rates.cast::<c_int>(), count)
                    .iter()
                    .filter_map(|&rate| u32::try_from(rate).ok())
                    .collect()
            }
            _ => Vec::new(),
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
                transports: vec!["sharedMemory".to_owned(), "pipe".to_owned()],
                ..ComponentDescription::unavailable(ComponentKind::VideoEncoder, codec.id(), "")
            },
            Err(reason) => {
                ComponentDescription::unavailable(ComponentKind::VideoEncoder, codec.id(), reason)
            }
        }),
    );
    components.extend(AudioCodec::ALL.map(|codec| match codec.availability() {
        Ok(()) => ComponentDescription {
            available: true,
            reason: None,
            settings: codec.settings(),
            sample_rates: Some(codec.sample_rates()),
            channels: vec![1, 2],
            ..ComponentDescription::unavailable(ComponentKind::AudioEncoder, codec.id(), "")
        },
        Err(reason) => {
            ComponentDescription::unavailable(ComponentKind::AudioEncoder, codec.id(), reason)
        }
    }));
    components.extend(MuxerFormat::ALL.map(|format| match format.availability() {
        Ok(()) => ComponentDescription {
            available: true,
            reason: None,
            settings: format.settings(),
            ..ComponentDescription::unavailable(ComponentKind::Muxer, format.id(), "")
        },
        Err(reason) => ComponentDescription::unavailable(ComponentKind::Muxer, format.id(), reason),
    }));
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
        "Speed preset",
        SettingValue::String("medium".to_owned()),
        SettingKind::Select { options },
    );
    preset.description = Some("Slower presets make smaller files at the same quality.".to_owned());
    let rate_control = in_group(
        select(
            "rateControl",
            "Mode",
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
        assert_eq!(availability("prores_ks"), Ok(()));
    }

    #[test]
    fn aac_lists_the_rates_it_takes() {
        let SampleRates::Only(rates) = AudioCodec::Aac.sample_rates() else {
            panic!("aac lists its rates");
        };
        for rate in [8_000, 44_100, 48_000, 96_000] {
            assert!(rates.contains(&rate), "{rates:?}");
        }
        assert!(!rates.contains(&12_345));
    }

    #[test]
    fn every_muxer_is_linked() {
        for format in MuxerFormat::ALL {
            assert_eq!(format.availability(), Ok(()), "{format:?}");
        }
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

    #[test]
    fn x264_names_speed_preset_and_mode() {
        let settings = x264_settings();
        let preset = settings
            .iter()
            .find(|field| field.key == "preset")
            .expect("x264 has a preset");
        assert_eq!(preset.label, "Speed preset");
        let rate_control = settings
            .iter()
            .find(|field| field.key == "rateControl")
            .expect("x264 has a rate control");
        assert_eq!(rate_control.label, "Mode");
        assert_eq!(rate_control.group.as_deref(), Some("Rate control"));
    }

    #[test]
    fn no_field_is_labelled_as_its_group() {
        let codecs = VideoCodec::ALL
            .into_iter()
            .map(|codec| codec.settings())
            .chain(AudioCodec::ALL.into_iter().map(|codec| codec.settings()));
        for field in codecs.flatten() {
            assert!(
                field.group.as_ref() != Some(&field.label),
                "{} is labelled as its group",
                field.key
            );
        }
    }
}
