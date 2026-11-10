//! The test codec's four components: their ids, what its manifest declares
//! of them, and what `describe` says (settings schemas included).

use celesta_media::{
    ComponentDescription, ComponentKind, PixelFormat, SampleRates, SelectOption, SettingField,
    SettingKind, SettingValue,
};
use celesta_plugin_host::MediaManifest;
use serde_json::{Value, json};

/// The id of the codec's one decoder, which reads a test codec or WAV file
/// of any extension.
pub const DECODER_ID: &str = "test";

/// The id of the video encoder, which stores frames in any pixel format.
pub const VIDEO_ENCODER_ID: &str = "video";

/// The id of the audio encoder, which stores `f32` samples.
pub const AUDIO_ENCODER_ID: &str = "pcm";

/// The id of the muxer, which writes a test codec file whatever its
/// extension.
pub const MUXER_ID: &str = "file";

/// The decoder's display name.
pub const DECODER_NAME: &str = "Test codec";

/// The codec string of the video encoder's track.
pub const VIDEO_CODEC: &str = "ctst";

/// The codec string of the audio encoder's track.
pub const AUDIO_CODEC: &str = "lpcm.f32le";

/// The pixel formats the video encoder takes, in the order Celesta prefers
/// them, the two it never offers last.
pub(crate) const INPUT_FORMATS: [PixelFormat; 6] = [
    PixelFormat::Yuv420p,
    PixelFormat::Yuv420p10,
    PixelFormat::Rgba16,
    PixelFormat::Rgba8,
    PixelFormat::Nv12,
    PixelFormat::P010,
];

/// The largest priming delay the audio encoder adds, in sample frames.
pub(crate) const MAX_DELAY: f64 = 65_536.0;

/// The rate the audio encoder's `sampleRate` setting can fix.
pub(crate) const FIXED_SAMPLE_RATE: u32 = 48_000;

/// The extensions the muxer writes.
const EXTENSIONS: [&str; 4] = ["mp4", "mov", "webm", "ctst"];

/// What the plugin's manifest declares of its components, as JSON.
pub(crate) fn manifest_json() -> Value {
    let formats: Vec<String> = INPUT_FORMATS.iter().map(ToString::to_string).collect();
    json!({
        "decoders": [{ "id": DECODER_ID, "name": DECODER_NAME, "extensions": [] }],
        "videoEncoders": [{
            "id": VIDEO_ENCODER_ID,
            "name": "Test video encoder",
            "codecs": [VIDEO_CODEC],
            "inputFormats": formats,
        }],
        "audioEncoders": [{
            "id": AUDIO_ENCODER_ID,
            "name": "Test audio encoder",
            "codecs": ["lpcm"],
        }],
        "muxers": [{
            "id": MUXER_ID,
            "name": "Test muxer",
            "extensions": EXTENSIONS,
            "video": [VIDEO_CODEC],
            "audio": ["lpcm"],
            "defaultVideo": VIDEO_CODEC,
            "defaultAudio": "lpcm",
            "trimsLeadingAudio": true,
        }],
    })
}

/// [`manifest_json`] as a [`MediaManifest`].
pub(crate) fn manifest() -> MediaManifest {
    let (manifest, problems) = MediaManifest::from_json(manifest_json());
    assert!(
        problems.is_empty(),
        "the test manifest is valid: {problems:?}"
    );
    manifest
}

fn option(value: &str, label: &str) -> SelectOption {
    SelectOption {
        value: value.to_owned(),
        label: label.to_owned(),
    }
}

fn field(
    key: &str,
    label: &str,
    default: SettingValue,
    kind: SettingKind,
    description: &str,
) -> SettingField {
    SettingField {
        key: key.to_owned(),
        label: label.to_owned(),
        default,
        kind,
        description: Some(description.to_owned()),
        group: None,
        advanced: false,
        visible_when: Default::default(),
    }
}

/// The video encoder's settings: `input` and `alpha`.
pub(crate) fn video_settings() -> Vec<SettingField> {
    let mut options = vec![option("auto", "First offered")];
    options.extend(
        [
            PixelFormat::Rgba8,
            PixelFormat::Rgba16,
            PixelFormat::Yuv420p,
            PixelFormat::Yuv420p10,
            PixelFormat::Nv12,
            PixelFormat::P010,
        ]
        .map(|format| option(&format.to_string(), &format.to_string())),
    );
    let input = field(
        "input",
        "Input format",
        SettingValue::String("auto".to_owned()),
        SettingKind::Select { options },
        "The pixel format to store; auto takes the first one offered.",
    );
    let mut alpha = field(
        "alpha",
        "Keep alpha",
        SettingValue::Boolean(false),
        SettingKind::Boolean,
        "Stores the frames' alpha; needs rgba16 as the input.",
    );
    alpha.advanced = true;
    vec![input, alpha]
}

/// The audio encoder's settings: `delay` and `sampleRate`.
pub(crate) fn audio_settings() -> Vec<SettingField> {
    let mut delay = field(
        "delay",
        "Delay",
        SettingValue::Number(0.0),
        SettingKind::Number {
            min: Some(0.0),
            max: Some(MAX_DELAY),
            step: None,
            integer: true,
            unit: Some("frames".to_owned()),
        },
        "Frames of silence the encoder adds before the audio, for the muxer to hide.",
    );
    delay.advanced = true;
    let mut rate = field(
        "sampleRate",
        "Sample rate",
        SettingValue::String("any".to_owned()),
        SettingKind::Select {
            options: vec![
                option("any", "As requested"),
                option(&FIXED_SAMPLE_RATE.to_string(), "48000 Hz"),
            ],
        },
        "The rate the encoder answers; Celesta converts the mix to it.",
    );
    rate.advanced = true;
    vec![delay, rate]
}

/// What `describe` answers: the four components, all available.
pub(crate) fn describe() -> Vec<ComponentDescription> {
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
    vec![
        component(ComponentKind::Decoder, DECODER_ID),
        ComponentDescription {
            settings: video_settings(),
            input_formats: INPUT_FORMATS.to_vec(),
            transports: vec!["sharedMemory".to_owned(), "pipe".to_owned()],
            ..component(ComponentKind::VideoEncoder, VIDEO_ENCODER_ID)
        },
        ComponentDescription {
            settings: audio_settings(),
            sample_rates: Some(SampleRates::Any),
            channels: vec![1, 2],
            ..component(ComponentKind::AudioEncoder, AUDIO_ENCODER_ID)
        },
        component(ComponentKind::Muxer, MUXER_ID),
    ]
}
