//! The plugin's components: their ids, settings schemas, and what
//! `describe` says of each, from the encoders Apple's frameworks list. Nothing
//! here calls a framework, so it builds and is tested on every system; the
//! lists come from `apple.rs` on macOS.

use crate::DECODER_ID;
use celesta_media::settings::{InvalidSetting, validate};
use celesta_media::{
    ComponentDescription, ComponentKind, MediaError, PixelFormat, SampleRates, SelectOption,
    SettingField, SettingKind, SettingValue, Settings,
};

/// A four-character code as Apple's frameworks hold it: the four bytes
/// big-endian in a `u32`.
const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// `kAudioFormatMPEG4AAC`, the format id `kAudioFormatProperty_Encoders`
/// lists for AudioToolbox's AAC encoder.
pub(crate) const AAC_FORMAT: u32 = fourcc(b"aac ");

/// A video encoder component, on VideoToolbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoCodec {
    H264,
    Hevc,
    /// ProRes 4444 or 422 HQ.
    ProRes,
}

impl VideoCodec {
    pub(crate) const ALL: [Self; 3] = [Self::H264, Self::Hevc, Self::ProRes];

    /// The component's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
            Self::ProRes => "prores",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|codec| codec.id() == id)
    }

    /// The codec type (`CMVideoCodecType`) whose encoder in
    /// `VTCopyVideoEncoderList` makes the component available: ProRes
    /// needs 4444's, `ap4h`.
    pub(crate) const fn codec_type(self) -> u32 {
        match self {
            Self::H264 => fourcc(b"avc1"),
            Self::Hevc => fourcc(b"hvc1"),
            Self::ProRes => fourcc(b"ap4h"),
        }
    }

    /// Its name in an unavailable component's reason.
    const fn display_name(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Hevc => "HEVC",
            Self::ProRes => "ProRes 4444",
        }
    }

    /// Every format it takes under any settings, best first, as the
    /// manifest's `inputFormats` lists them.
    pub(crate) fn input_formats(self) -> Vec<PixelFormat> {
        match self {
            Self::H264 => vec![PixelFormat::Nv12, PixelFormat::Rgba16],
            Self::Hevc => vec![PixelFormat::Nv12, PixelFormat::P010, PixelFormat::Rgba16],
            Self::ProRes => vec![PixelFormat::Rgba16],
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
                settings
            }
            Self::ProRes => vec![select(
                "profile",
                "Profile",
                &[("4444", "4444"), ("hq", "422 HQ")],
                "4444",
            )],
        }
    }
}

/// One VideoToolbox compression property a video encoder's settings set:
/// `kVTCompressionPropertyKey_ProfileLevel` (named by its
/// `kVTProfileLevel_` suffix), `Quality` (0 to 1), `AverageBitRate`
/// (bit/s), `MaxKeyFrameInterval` (frames), and `AllowFrameReordering`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum VtSetting {
    ProfileLevel(&'static str),
    Quality(f32),
    AverageBitRate(i64),
    MaxKeyFrameInterval(i32),
    AllowFrameReordering(bool),
}

/// The settings `given` for `codec`, checked against its schema, with the
/// defaults of those not given.
pub(crate) fn checked_settings(
    codec: VideoCodec,
    given: &Settings,
) -> Result<Settings, MediaError> {
    validate(&codec.settings(), given)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })
}

fn text<'a>(settings: &'a Settings, key: &str) -> &'a str {
    match settings.get(key) {
        Some(SettingValue::String(value)) => value,
        _ => "",
    }
}

/// A whole-number setting; none when its field is hidden.
fn whole(settings: &Settings, key: &str) -> Option<i64> {
    match settings.get(key) {
        Some(SettingValue::Number(value)) => Some(*value as i64),
        _ => None,
    }
}

/// Whether ProRes `settings` ask for 422 HQ rather than 4444.
pub(crate) fn prores_hq(settings: &Settings) -> bool {
    text(settings, "profile") == "hq"
}

/// What `codec` takes of `offered` (best first) at `width` x `height`, and
/// whether it keeps alpha: 4:2:0 YUV of the bit depth when it is offered,
/// else `rgba16` (a software renderer's offer), which VideoToolbox converts
/// with the session's matrix; ProRes always takes `rgba16`, and 4444 alone
/// keeps its alpha.
///
/// H.264 and HEVC refuse an odd width or height: VideoToolbox encodes them
/// 4:2:0 whatever the input, and crops such a frame to even dimensions
/// without an error, so the stream would not match its track.
pub(crate) fn choose_input(
    codec: VideoCodec,
    settings: &Settings,
    width: u32,
    height: u32,
    offered: &[PixelFormat],
) -> Result<(PixelFormat, bool), MediaError> {
    let yuv = match codec {
        VideoCodec::H264 => Some(PixelFormat::Nv12),
        VideoCodec::Hevc if text(settings, "bitDepth") == "10" => Some(PixelFormat::P010),
        VideoCodec::Hevc => Some(PixelFormat::Nv12),
        VideoCodec::ProRes => None,
    };
    if yuv.is_some() && !(width.is_multiple_of(2) && height.is_multiple_of(2)) {
        return Err(MediaError::Codec(format!(
            "{} needs even dimensions, not {width}x{height}",
            codec.display_name()
        )));
    }
    let wanted: Vec<PixelFormat> = yuv.into_iter().chain([PixelFormat::Rgba16]).collect();
    let input = wanted
        .iter()
        .copied()
        .find(|format| offered.contains(format))
        .ok_or_else(|| {
            let names: Vec<String> = wanted.iter().map(ToString::to_string).collect();
            MediaError::InvalidSettings {
                key: "input".to_owned(),
                message: format!("needs {}", names.join(" or ")),
            }
        })?;
    let alpha = codec == VideoCodec::ProRes && !prores_hq(settings);
    Ok((input, alpha))
}

/// The VideoToolbox properties `settings` give `codec`, checked against its
/// schema (the defaults fill what is not given). ProRes has none: its codec
/// type picks the profile.
pub(crate) fn vt_settings(
    codec: VideoCodec,
    settings: &Settings,
) -> Result<Vec<VtSetting>, MediaError> {
    let settings = checked_settings(codec, settings)?;
    let profile = match codec {
        VideoCodec::H264 => match text(&settings, "profile") {
            "main" => "H264_Main_AutoLevel",
            "baseline" => "H264_Baseline_AutoLevel",
            _ => "H264_High_AutoLevel",
        },
        VideoCodec::Hevc if text(&settings, "bitDepth") == "10" => "HEVC_Main10_AutoLevel",
        VideoCodec::Hevc => "HEVC_Main_AutoLevel",
        VideoCodec::ProRes => return Ok(Vec::new()),
    };
    let mut properties = vec![VtSetting::ProfileLevel(profile)];
    if text(&settings, "rateControl") == "bitrate" {
        properties.extend(whole(&settings, "bitrate").map(VtSetting::AverageBitRate));
    } else if let Some(quality) = whole(&settings, "quality") {
        properties.push(VtSetting::Quality(quality as f32 / 100.0));
    }
    if let Some(interval) = whole(&settings, "keyframeInterval") {
        properties.push(VtSetting::MaxKeyFrameInterval(interval as i32));
    }
    // Baseline has no B-frames: VideoToolbox never reorders it, and its SPS
    // does not say so, so allowing reordering would only shift its decode
    // times by the unknown depth.
    properties.push(VtSetting::AllowFrameReordering(
        profile != "H264_Baseline_AutoLevel"
            && settings.get("bFrames") != Some(&SettingValue::Boolean(false)),
    ));
    Ok(properties)
}

/// An audio encoder component, on AudioToolbox.
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
            Self::Aac => vec![field(
                "bitrate",
                "Bitrate",
                SettingValue::Number(192_000.0),
                SettingKind::Number {
                    min: Some(32_000.0),
                    max: Some(320_000.0),
                    step: None,
                    integer: true,
                    unit: Some("bit/s".to_owned()),
                },
            )],
        }
    }
}

/// The bitrate (bit/s) AAC `given` settings ask for, checked against its
/// schema; the default when none is given.
pub(crate) fn aac_bitrate(given: &Settings) -> Result<u32, MediaError> {
    let settings = validate(&AudioCodec::Aac.settings(), given)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    Ok(whole(&settings, "bitrate").map_or(192_000, |bitrate| bitrate as u32))
}

/// A muxer component, on `AVAssetWriter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MuxerFormat {
    Mp4,
    Mov,
}

impl MuxerFormat {
    pub(crate) const ALL: [Self; 2] = [Self::Mp4, Self::Mov];

    /// The component's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.id() == id)
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
        }
    }
}

/// `Ok` when `encoders`, the codec types `VTCopyVideoEncoderList` lists,
/// have `codec`'s, else why not.
pub(crate) fn video_availability(codec: VideoCodec, encoders: &[u32]) -> Result<(), String> {
    if encoders.contains(&codec.codec_type()) {
        Ok(())
    } else {
        Err(format!(
            "VideoToolbox has no {} encoder on this Mac",
            codec.display_name()
        ))
    }
}

/// `Ok` when `encoders`, the format ids `kAudioFormatProperty_Encoders`
/// lists, have AAC's, else why not.
pub(crate) fn aac_availability(encoders: &[u32]) -> Result<(), String> {
    if encoders.contains(&AAC_FORMAT) {
        Ok(())
    } else {
        Err("AudioToolbox has no AAC encoder on this Mac".to_owned())
    }
}

/// What `describe` answers, from the encoders VideoToolbox lists
/// (`video_encoders`, codec types), the encoders AudioToolbox lists
/// (`audio_encoders`, format ids), and the sample rates its AAC encoder
/// takes: every component the manifest declares, in its order. The decoder
/// and the muxers are always available.
pub(crate) fn describe_from(
    video_encoders: &[u32],
    audio_encoders: &[u32],
    sample_rates: &[u32],
) -> Vec<ComponentDescription> {
    let mut components = vec![available(ComponentKind::Decoder, DECODER_ID)];
    components.extend(VideoCodec::ALL.map(|codec| {
        match video_availability(codec, video_encoders) {
            Ok(()) => ComponentDescription {
                settings: codec.settings(),
                input_formats: codec.input_formats(),
                transports: vec!["sharedMemory".to_owned(), "pipe".to_owned()],
                ..available(ComponentKind::VideoEncoder, codec.id())
            },
            Err(reason) => {
                ComponentDescription::unavailable(ComponentKind::VideoEncoder, codec.id(), reason)
            }
        }
    }));
    components.extend(
        AudioCodec::ALL.map(|codec| match aac_availability(audio_encoders) {
            Ok(()) => ComponentDescription {
                settings: codec.settings(),
                sample_rates: Some(SampleRates::Only(sample_rates.to_vec())),
                channels: vec![1, 2],
                ..available(ComponentKind::AudioEncoder, codec.id())
            },
            Err(reason) => {
                ComponentDescription::unavailable(ComponentKind::AudioEncoder, codec.id(), reason)
            }
        }),
    );
    components.extend(MuxerFormat::ALL.map(|format| ComponentDescription {
        settings: format.settings(),
        ..available(ComponentKind::Muxer, format.id())
    }));
    components
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
    use celesta_media::{ComponentKind, MediaError, SampleRates, SettingValue, Settings};
    use celesta_plugin_host::{
        MediaAudioEncoderEntry, MediaDecoderEntry, MediaManifest, MediaMuxerEntry,
        MediaVideoEncoderEntry,
    };

    /// The rates AudioToolbox's AAC encoder lists on macOS 26.
    const RATES: [u32; 9] = [
        8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
    ];

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

    fn every_encoder() -> (Vec<u32>, Vec<u32>) {
        (
            VideoCodec::ALL.map(VideoCodec::codec_type).to_vec(),
            vec![AAC_FORMAT],
        )
    }

    fn text(value: &str) -> SettingValue {
        SettingValue::String(value.to_owned())
    }

    fn keys(settings: &[SettingField]) -> Vec<&str> {
        settings.iter().map(|field| field.key.as_str()).collect()
    }

    fn field<'a>(settings: &'a [SettingField], key: &str) -> &'a SettingField {
        settings
            .iter()
            .find(|field| field.key == key)
            .unwrap_or_else(|| panic!("no {key}"))
    }

    fn visible_when(field: &SettingField) -> Vec<(&str, Vec<SettingValue>)> {
        field
            .visible_when
            .iter()
            .map(|(key, values)| (key.as_str(), values.clone()))
            .collect()
    }

    fn filled(settings: &[SettingField]) -> Settings {
        validate(settings, &Settings::new()).unwrap()
    }

    fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    #[test]
    fn plugin_json_declares_every_component() {
        let manifest = manifest();
        assert_eq!(
            manifest.decoders,
            [MediaDecoderEntry {
                id: "avfoundation".to_owned(),
                name: "AVFoundation".to_owned(),
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
                    "H.264 (VideoToolbox)",
                    &["avc1"],
                    &["nv12", "rgba16"]
                ),
                video(
                    "hevc",
                    "HEVC (VideoToolbox)",
                    &["hvc1"],
                    &["nv12", "p010", "rgba16"]
                ),
                video(
                    "prores",
                    "ProRes (VideoToolbox)",
                    &["ap4h", "apch"],
                    &["rgba16"]
                ),
            ]
        );
        assert_eq!(
            manifest.audio_encoders,
            [MediaAudioEncoderEntry {
                id: "aac".to_owned(),
                name: "AAC (AudioToolbox)".to_owned(),
                codecs: strings(&["mp4a"]),
            }]
        );
        let muxer = |id: &str, name: &str, extensions: &[&str], video: &[&str], audio: &[&str]| {
            MediaMuxerEntry {
                id: id.to_owned(),
                name: name.to_owned(),
                extensions: strings(extensions),
                video: strings(video),
                audio: Some(strings(audio)),
                default_video: Some(video[0].to_owned()),
                default_audio: Some("mp4a".to_owned()),
                trims_leading_audio: true,
                sequence: false,
            }
        };
        let mut mov = muxer(
            "mov",
            "QuickTime (AVFoundation)",
            &["mov"],
            &["avc1", "hvc1", "ap4h", "apch"],
            &["mp4a", "lpcm"],
        );
        mov.default_video = Some("ap4h".to_owned());
        assert_eq!(
            manifest.muxers,
            [
                muxer(
                    "mp4",
                    "MP4 (AVFoundation)",
                    &["mp4", "m4v"],
                    &["avc1", "hvc1"],
                    &["mp4a"]
                ),
                mov,
            ]
        );
    }

    #[test]
    fn describe_matches_the_manifest() {
        let manifest = manifest();
        let (video, audio) = every_encoder();
        let described = describe_from(&video, &audio, &RATES);
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
            .chain(
                manifest
                    .muxers
                    .iter()
                    .map(|entry| (ComponentKind::Muxer, entry.id.as_str())),
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
        assert_eq!(aac.sample_rates, Some(SampleRates::Only(RATES.to_vec())));
        assert_eq!(aac.channels, [1, 2]);
        assert_eq!(aac.settings, AudioCodec::Aac.settings());
        for format in MuxerFormat::ALL {
            let component = described
                .iter()
                .find(|component| {
                    component.kind == ComponentKind::Muxer && component.id == format.id()
                })
                .unwrap();
            assert_eq!(component.settings, format.settings());
        }
    }

    #[test]
    fn a_missing_encoder_has_a_reason() {
        let described = describe_from(&[VideoCodec::H264.codec_type()], &[], &RATES);
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
            Some("VideoToolbox has no HEVC encoder on this Mac")
        );
        assert_eq!(
            reason(ComponentKind::VideoEncoder, "prores").as_deref(),
            Some("VideoToolbox has no ProRes 4444 encoder on this Mac")
        );
        assert_eq!(
            reason(ComponentKind::AudioEncoder, "aac").as_deref(),
            Some("AudioToolbox has no AAC encoder on this Mac")
        );
        assert_eq!(reason(ComponentKind::Decoder, "avfoundation"), None);
        assert_eq!(reason(ComponentKind::Muxer, "mp4"), None);
        assert_eq!(reason(ComponentKind::Muxer, "mov"), None);
        assert_eq!(
            video_availability(VideoCodec::H264, &[]),
            Err("VideoToolbox has no H.264 encoder on this Mac".to_owned())
        );
        // ProRes 422 HQ alone is not ProRes 4444.
        assert!(video_availability(VideoCodec::ProRes, &[u32::from_be_bytes(*b"apch")]).is_err());
        assert_eq!(aac_availability(&[AAC_FORMAT]), Ok(()));
    }

    #[test]
    fn ids_name_their_components() {
        for codec in VideoCodec::ALL {
            assert_eq!(VideoCodec::from_id(codec.id()), Some(codec));
        }
        for codec in AudioCodec::ALL {
            assert_eq!(AudioCodec::from_id(codec.id()), Some(codec));
        }
        for format in MuxerFormat::ALL {
            assert_eq!(MuxerFormat::from_id(format.id()), Some(format));
        }
        assert_eq!(VideoCodec::from_id("libx264"), None);
        assert_eq!(AudioCodec::from_id("opus"), None);
        assert_eq!(MuxerFormat::from_id("webm"), None);
        assert_eq!(
            VideoCodec::ALL.map(VideoCodec::codec_type),
            [*b"avc1", *b"hvc1", *b"ap4h"].map(u32::from_be_bytes)
        );
        assert_eq!(AAC_FORMAT, u32::from_be_bytes(*b"aac "));
    }

    #[test]
    fn odd_sizes_and_rgb_offers_take_rgba16() {
        use PixelFormat::{Nv12, P010, Rgba8, Rgba16};
        let none = Settings::new();
        let offerable = PixelFormat::OFFERABLE;
        let choose = |codec, given: &Settings, width, height, offered: &[PixelFormat]| {
            choose_input(codec, given, width, height, offered).unwrap()
        };
        let refusal =
            |codec, given: &Settings, width, height, offered: &[PixelFormat]| match choose_input(
                codec, given, width, height, offered,
            ) {
                Err(MediaError::Codec(message)) => message,
                other => panic!("{codec:?} took {width}x{height}: {other:?}"),
            };
        for (codec, name) in [(VideoCodec::H264, "H.264"), (VideoCodec::Hevc, "HEVC")] {
            assert_eq!(choose(codec, &none, 64, 36, &offerable), (Nv12, false));
            assert_eq!(
                choose(codec, &none, 64, 36, &[Rgba16, Rgba8]),
                (Rgba16, false)
            );
            // VideoToolbox crops 4:2:0 to even dimensions, whatever the
            // input, so an odd size is refused rather than shrunk.
            for (width, height) in [(63, 35), (64, 35), (63, 36)] {
                for offered in [&offerable[..], &[Rgba16, Rgba8]] {
                    assert_eq!(
                        refusal(codec, &none, width, height, offered),
                        format!("{name} needs even dimensions, not {width}x{height}")
                    );
                }
            }
        }
        let ten = settings(&[("bitDepth", text("10"))]);
        assert_eq!(
            choose(VideoCodec::Hevc, &ten, 64, 36, &offerable),
            (P010, false)
        );
        assert_eq!(
            refusal(VideoCodec::Hevc, &ten, 63, 36, &offerable),
            "HEVC needs even dimensions, not 63x36"
        );
        assert_eq!(
            choose(VideoCodec::Hevc, &ten, 64, 36, &[Rgba16, Rgba8]),
            (Rgba16, false)
        );
        // VideoToolbox's H.264 is 8-bit only.
        assert_eq!(
            choose(VideoCodec::H264, &none, 64, 36, &[P010, Rgba16]),
            (Rgba16, false)
        );
        let prores = |profile| settings(&[("profile", text(profile))]);
        assert_eq!(
            choose(VideoCodec::ProRes, &prores("4444"), 64, 36, &offerable),
            (Rgba16, true)
        );
        assert_eq!(
            choose(VideoCodec::ProRes, &none, 63, 35, &offerable),
            (Rgba16, true)
        );
        assert_eq!(
            choose(VideoCodec::ProRes, &prores("hq"), 64, 36, &offerable),
            (Rgba16, false)
        );
        for (codec, width, height) in [
            (VideoCodec::H264, 64, 36),
            (VideoCodec::Hevc, 64, 36),
            (VideoCodec::ProRes, 64, 36),
            (VideoCodec::ProRes, 63, 35),
        ] {
            assert!(
                matches!(
                    choose_input(codec, &none, width, height, &[Rgba8]),
                    Err(MediaError::InvalidSettings { key, .. }) if key == "input"
                ),
                "{codec:?} {width}x{height}"
            );
        }
    }

    #[test]
    fn settings_become_videotoolbox_properties() {
        use VtSetting::{
            AllowFrameReordering, AverageBitRate, MaxKeyFrameInterval, ProfileLevel, Quality,
        };
        let none = Settings::new();
        assert_eq!(
            vt_settings(VideoCodec::H264, &none).unwrap(),
            [
                ProfileLevel("H264_High_AutoLevel"),
                Quality(0.7),
                MaxKeyFrameInterval(250),
                AllowFrameReordering(true),
            ]
        );
        assert_eq!(
            vt_settings(VideoCodec::H264, &filled(&VideoCodec::H264.settings())).unwrap(),
            vt_settings(VideoCodec::H264, &none).unwrap()
        );
        for (profile, level) in [
            ("main", "H264_Main_AutoLevel"),
            ("baseline", "H264_Baseline_AutoLevel"),
        ] {
            let given = settings(&[("profile", text(profile))]);
            assert_eq!(
                vt_settings(VideoCodec::H264, &given).unwrap()[0],
                ProfileLevel(level)
            );
        }
        // Baseline has no B-frames, whatever `bFrames` says, so its decode
        // times are not shifted.
        for b_frames in [true, false] {
            let given = settings(&[
                ("profile", text("baseline")),
                ("bFrames", SettingValue::Boolean(b_frames)),
            ]);
            assert_eq!(
                vt_settings(VideoCodec::H264, &given).unwrap().last(),
                Some(&AllowFrameReordering(false))
            );
        }
        let main = settings(&[("profile", text("main"))]);
        assert_eq!(
            vt_settings(VideoCodec::H264, &main).unwrap().last(),
            Some(&AllowFrameReordering(true))
        );
        let given = settings(&[
            ("rateControl", text("bitrate")),
            ("bitrate", SettingValue::Number(5_000_000.0)),
            ("bFrames", SettingValue::Boolean(false)),
            ("keyframeInterval", SettingValue::Number(60.0)),
        ]);
        assert_eq!(
            vt_settings(VideoCodec::H264, &given).unwrap(),
            [
                ProfileLevel("H264_High_AutoLevel"),
                AverageBitRate(5_000_000),
                MaxKeyFrameInterval(60),
                AllowFrameReordering(false),
            ]
        );
        let given = settings(&[("quality", SettingValue::Number(35.0))]);
        assert_eq!(
            vt_settings(VideoCodec::H264, &given).unwrap()[1],
            Quality(0.35)
        );
        assert_eq!(
            vt_settings(VideoCodec::Hevc, &none).unwrap(),
            [
                ProfileLevel("HEVC_Main_AutoLevel"),
                Quality(0.7),
                MaxKeyFrameInterval(250),
                AllowFrameReordering(true),
            ]
        );
        let ten = settings(&[("bitDepth", text("10"))]);
        assert_eq!(
            vt_settings(VideoCodec::Hevc, &ten).unwrap()[0],
            ProfileLevel("HEVC_Main10_AutoLevel")
        );
        for profile in ["4444", "hq"] {
            let given = settings(&[("profile", text(profile))]);
            assert_eq!(vt_settings(VideoCodec::ProRes, &given).unwrap(), []);
        }
        assert!(matches!(
            vt_settings(VideoCodec::H264, &settings(&[("profile", text("high10"))])),
            Err(MediaError::InvalidSettings { key, .. }) if key == "profile"
        ));
        assert!(matches!(
            vt_settings(VideoCodec::Hevc, &settings(&[("profile", text("main"))])),
            Err(MediaError::InvalidSettings { key, .. }) if key == "profile"
        ));
    }

    #[test]
    fn settings_follow_the_decisions() {
        let h264 = VideoCodec::H264.settings();
        assert_eq!(
            keys(&h264),
            [
                "profile",
                "rateControl",
                "quality",
                "bitrate",
                "keyframeInterval",
                "bFrames"
            ]
        );
        assert_eq!(
            visible_when(field(&h264, "quality")),
            [("rateControl", vec![text("quality")])]
        );
        assert_eq!(
            visible_when(field(&h264, "bitrate")),
            [("rateControl", vec![text("bitrate")])]
        );
        assert!(field(&h264, "keyframeInterval").advanced);
        assert!(field(&h264, "bFrames").advanced);
        assert_eq!(field(&h264, "bFrames").label, "B-frames");
        assert_eq!(
            field(&h264, "bitrate").default,
            SettingValue::Number(8_000_000.0)
        );
        assert_eq!(
            filled(&h264),
            settings(&[
                ("profile", text("high")),
                ("rateControl", text("quality")),
                ("quality", SettingValue::Number(70.0)),
                ("keyframeInterval", SettingValue::Number(250.0)),
                ("bFrames", SettingValue::Boolean(true)),
            ])
        );
        let given = settings(&[("rateControl", text("bitrate"))]);
        assert_eq!(
            validate(&h264, &given).unwrap()["bitrate"],
            SettingValue::Number(8_000_000.0)
        );
        assert!(
            validate(
                &h264,
                &settings(&[("quality", SettingValue::Number(101.0))])
            )
            .is_err()
        );
        assert!(validate(&h264, &settings(&[("profile", text("high10"))])).is_err());

        let hevc = VideoCodec::Hevc.settings();
        assert_eq!(
            keys(&hevc),
            [
                "rateControl",
                "quality",
                "bitrate",
                "keyframeInterval",
                "bFrames",
                "bitDepth"
            ]
        );
        assert_eq!(
            filled(&hevc),
            settings(&[
                ("rateControl", text("quality")),
                ("quality", SettingValue::Number(70.0)),
                ("keyframeInterval", SettingValue::Number(250.0)),
                ("bFrames", SettingValue::Boolean(true)),
                ("bitDepth", text("8")),
            ])
        );
        assert!(validate(&hevc, &settings(&[("bitDepth", text("10"))])).is_ok());

        let prores = VideoCodec::ProRes.settings();
        assert_eq!(keys(&prores), ["profile"]);
        assert_eq!(filled(&prores), settings(&[("profile", text("4444"))]));
        assert!(validate(&prores, &settings(&[("profile", text("hq"))])).is_ok());

        let aac = AudioCodec::Aac.settings();
        assert_eq!(keys(&aac), ["bitrate"]);
        assert_eq!(
            filled(&aac),
            settings(&[("bitrate", SettingValue::Number(192_000.0))])
        );
        for (bitrate, valid) in [
            (31_999.0, false),
            (32_000.0, true),
            (320_000.0, true),
            (320_001.0, false),
        ] {
            let given = settings(&[("bitrate", SettingValue::Number(bitrate))]);
            assert_eq!(validate(&aac, &given).is_ok(), valid, "{bitrate}");
        }

        assert_eq!(aac_bitrate(&Settings::new()).unwrap(), 192_000);
        assert_eq!(
            aac_bitrate(&settings(&[("bitrate", SettingValue::Number(64_000.0))])).unwrap(),
            64_000
        );
        assert!(matches!(
            aac_bitrate(&settings(&[("bitrate", SettingValue::Number(16_000.0))])),
            Err(MediaError::InvalidSettings { key, .. }) if key == "bitrate"
        ));

        for format in MuxerFormat::ALL {
            let muxer = format.settings();
            assert_eq!(keys(&muxer), ["faststart"]);
            assert_eq!(field(&muxer, "faststart").label, "Fast start");
            assert_eq!(
                filled(&muxer),
                settings(&[("faststart", SettingValue::Boolean(true))])
            );
        }
    }
}
