use crate::decode::DecodeAudioRequest;
use crate::measure::MeasureTextRequest;
use crate::probe::MediaProbeRequest;
use crate::properties::{PropertyIssue, PropertyLayer};
use crate::props_edit::OrderedObject;
use crate::statics::StaticDefinition;
use crate::types::{
    ComponentPropertyField, ComponentPropertySchema, LayerSource, ReactAudioClipDescriptor,
    ReactCompositionConfig, ReactCompositionInfo, ReactSubtitleDescriptor,
};
use celesta_composition::{Layer, Scene, ShaderSource, Time};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The first stdin line when the CLI runs with `--properties-stdin`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PropertyInputsMessage<'a> {
    pub(crate) property_inputs: &'a [PropertyLayer],
}

#[derive(Serialize)]
pub(crate) struct Request<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) time: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<ProjectPayload<'a>>,
    /// Composition facts plus the requested time a component-resolution
    /// request's hooks should see (absent for frame requests, which carry
    /// `time` instead).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) runtime: Option<ResolutionRuntime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) components: Option<Vec<ComponentRequest<'a>>>,
    /// Asks for scene transforms with default-valued fields left out, which
    /// `EvaluatedTransform` fills back in. Without it the CLI sends every field.
    #[serde(
        rename = "compactTransforms",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub(crate) compact_transforms: bool,
    /// Asks for a frame past the composition's end, evaluated as if the
    /// video went on (see [`crate::ReactBridge::evaluate_past_end`]).
    #[serde(rename = "openEnd", skip_serializing_if = "std::ops::Not::not")]
    pub(crate) open_end: bool,
    /// Asks the runtime to draw debug guides (`useDebugGuides()`) in this
    /// frame (see [`crate::ReactBridge::set_debug_guides`]).
    #[serde(rename = "debugGuides", skip_serializing_if = "std::ops::Not::not")]
    pub(crate) debug_guides: bool,
    /// Asks the runtime to evaluate this frame as the editor preview, where
    /// `useIsPreview()` is true (see [`crate::ReactBridge::set_preview`]).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) preview: bool,
    /// Sent with frame requests: the runtime may then refer to `<Static>`
    /// subtrees by key (see [`crate::statics`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) statics: Option<StaticsRequest>,
}

/// Which frames' `<Static>` definitions this side has taken in.
#[derive(Serialize)]
pub(crate) struct StaticsRequest {
    /// The `seq` of the last frame decoded; 0 before any.
    pub(crate) decoded: u64,
}

/// The runtime context sent alongside component-resolution requests; the
/// TypeScript side turns it into the `CompositionRuntimeContext` those
/// components' `useCurrentFrame()`/`useVideoConfig()` read.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolutionRuntime {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: f64,
    pub(crate) duration_in_frames: u64,
    pub(crate) time: Time,
    pub(crate) preview: bool,
}

#[derive(Serialize)]
pub(crate) struct ProjectPayload<'a> {
    pub(crate) layers: &'a [Layer],
    pub(crate) tracks: &'a BTreeMap<String, Vec<Layer>>,
}

#[derive(Serialize)]
pub(crate) struct ComponentRequest<'a> {
    pub(crate) component: &'a str,
    pub(crate) props: &'a BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(try_from = "RawResponse")]
pub(crate) enum Response {
    CollectedAudio {
        #[serde(rename = "collectedAudio")]
        collected_audio: Vec<ReactAudioClipDescriptor>,
    },
    CollectedSubtitles {
        #[serde(rename = "collectedSubtitles")]
        collected_subtitles: Vec<ReactSubtitleDescriptor>,
    },
    MeasureText {
        #[serde(rename = "measureText")]
        measure_text: Box<MeasureTextRequest>,
    },
    Ok {
        scene: Scene,
        #[serde(default)]
        audio: Vec<ReactAudioClipDescriptor>,
        /// Absent when the frame shows none.
        #[serde(default)]
        subtitles: Vec<ReactSubtitleDescriptor>,
        /// The runtime's echo of an `openEnd` request; an older runtime
        /// ignores the request and leaves it out.
        #[serde(default, rename = "openEnd")]
        open_end: bool,
        /// The frame's number among those answered with `<Static>`
        /// references; absent otherwise.
        seq: Option<u64>,
        /// `<Static>` subtrees this frame defines.
        statics: Vec<StaticDefinition>,
        /// Keys no later frame refers to.
        dropped_statics: Vec<u32>,
    },
    Components {
        components: Vec<Option<Vec<Layer>>>,
        /// Absent from older runtimes, whose resolutions never used shaders.
        shaders: Vec<ShaderSource>,
    },
    LayerSource {
        /// `None` when no layer of the frame has the requested id.
        chain: Option<Vec<LayerSource>>,
    },
    Err {
        error: String,
    },
}

/// The body of a `layerSource` reply.
#[derive(Deserialize)]
pub(crate) struct LayerSourceReply {
    pub(crate) chain: Option<Vec<LayerSource>>,
}

/// Every key a [`Response`] can carry, read in one pass. An untagged enum
/// would buffer the whole message (a frame's scene is hundreds of KiB) and
/// replay it against each variant in turn, more than doubling parse time.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawResponse {
    pub(crate) collected_audio: Option<Vec<ReactAudioClipDescriptor>>,
    pub(crate) collected_subtitles: Option<Vec<ReactSubtitleDescriptor>>,
    pub(crate) measure_text: Option<Box<MeasureTextRequest>>,
    pub(crate) scene: Option<Scene>,
    #[serde(default)]
    pub(crate) audio: Vec<ReactAudioClipDescriptor>,
    #[serde(default)]
    pub(crate) subtitles: Vec<ReactSubtitleDescriptor>,
    pub(crate) components: Option<Vec<Option<Vec<Layer>>>>,
    pub(crate) layer_source: Option<LayerSourceReply>,
    #[serde(default)]
    pub(crate) shaders: Vec<ShaderSource>,
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) open_end: bool,
    pub(crate) seq: Option<u64>,
    #[serde(default)]
    pub(crate) statics: Vec<StaticDefinition>,
    #[serde(default)]
    pub(crate) dropped_statics: Vec<u32>,
}

impl TryFrom<RawResponse> for Response {
    type Error = &'static str;

    fn try_from(raw: RawResponse) -> Result<Self, Self::Error> {
        // The order the untagged enum used to try its variants in.
        if let Some(collected_audio) = raw.collected_audio {
            Ok(Self::CollectedAudio { collected_audio })
        } else if let Some(collected_subtitles) = raw.collected_subtitles {
            Ok(Self::CollectedSubtitles {
                collected_subtitles,
            })
        } else if let Some(measure_text) = raw.measure_text {
            Ok(Self::MeasureText { measure_text })
        } else if let Some(scene) = raw.scene {
            Ok(Self::Ok {
                scene,
                audio: raw.audio,
                subtitles: raw.subtitles,
                open_end: raw.open_end,
                seq: raw.seq,
                statics: raw.statics,
                dropped_statics: raw.dropped_statics,
            })
        } else if let Some(components) = raw.components {
            Ok(Self::Components {
                components,
                shaders: raw.shaders,
            })
        } else if let Some(reply) = raw.layer_source {
            Ok(Self::LayerSource { chain: reply.chain })
        } else if let Some(error) = raw.error {
            Ok(Self::Err { error })
        } else {
            Err(
                "response has none of collectedAudio, collectedSubtitles, measureText, scene, components, layerSource or error",
            )
        }
    }
}

/// Just the declared order of the ready message's `propertySchema` keys,
/// which the `BTreeMap` in [`ReadyMessage::Ready`] sorts away.
#[derive(Deserialize)]
struct ReadyOrder {
    #[serde(default, rename = "propertySchema")]
    schema: Option<OrderedObject>,
}

/// The `propertySchema` keys of a ready message in declaration order; empty
/// when the entry declared none. The message already parsed once, so a parse
/// failure here is not expected and just yields no order.
pub(crate) fn ready_property_order(message: &[u8]) -> Vec<String> {
    serde_json::from_slice::<ReadyOrder>(message)
        .ok()
        .and_then(|order| order.schema)
        .map(|schema| schema.0.into_iter().map(|(key, _)| key).collect())
        .unwrap_or_default()
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ReadyMessage {
    Ready {
        config: ReactCompositionConfig,
        #[serde(default, rename = "compositionId")]
        composition_id: Option<String>,
        #[serde(default)]
        compositions: Vec<ReactCompositionInfo>,
        #[serde(default, rename = "componentSchemas")]
        component_schemas: BTreeMap<String, ComponentPropertySchema>,
        /// `null` (or absent) when the entry never called
        /// `defineProjectProperties()`; an empty object means it declared an
        /// intentionally empty schema.
        #[serde(default, rename = "propertySchema")]
        project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
    },
    ProbeMedia {
        #[serde(rename = "probeMedia")]
        probe_media: MediaProbeRequest,
    },
    MeasureText {
        #[serde(rename = "measureText")]
        measure_text: Box<MeasureTextRequest>,
    },
    DecodeAudio {
        #[serde(rename = "decodeAudio")]
        decode_audio: DecodeAudioRequest,
    },
    InvalidProperties {
        #[serde(rename = "invalidProperties")]
        invalid_properties: Vec<PropertyIssue>,
    },
    Error {
        error: String,
    },
}
