use crate::measure::MeasureTextRequest;
use crate::probe::MediaProbeRequest;
use crate::properties::{PropertyIssue, PropertyLayer};
use crate::types::{
    ComponentPropertyField, ComponentPropertySchema, ReactAudioClipDescriptor,
    ReactCompositionConfig,
};
use celesta_composition::{Layer, Scene, Time};
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
    MeasureText {
        #[serde(rename = "measureText")]
        measure_text: Box<MeasureTextRequest>,
    },
    Ok {
        scene: Scene,
        #[serde(default)]
        audio: Vec<ReactAudioClipDescriptor>,
    },
    Components {
        components: Vec<Option<Vec<Layer>>>,
    },
    Err {
        error: String,
    },
}

/// Every key a [`Response`] can carry, read in one pass. An untagged enum
/// would buffer the whole message (a frame's scene is hundreds of KiB) and
/// replay it against each variant in turn, more than doubling parse time.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawResponse {
    pub(crate) collected_audio: Option<Vec<ReactAudioClipDescriptor>>,
    pub(crate) measure_text: Option<Box<MeasureTextRequest>>,
    pub(crate) scene: Option<Scene>,
    #[serde(default)]
    pub(crate) audio: Vec<ReactAudioClipDescriptor>,
    pub(crate) components: Option<Vec<Option<Vec<Layer>>>>,
    pub(crate) error: Option<String>,
}

impl TryFrom<RawResponse> for Response {
    type Error = &'static str;

    fn try_from(raw: RawResponse) -> Result<Self, Self::Error> {
        // The order the untagged enum used to try its variants in.
        if let Some(collected_audio) = raw.collected_audio {
            Ok(Self::CollectedAudio { collected_audio })
        } else if let Some(measure_text) = raw.measure_text {
            Ok(Self::MeasureText { measure_text })
        } else if let Some(scene) = raw.scene {
            Ok(Self::Ok {
                scene,
                audio: raw.audio,
            })
        } else if let Some(components) = raw.components {
            Ok(Self::Components { components })
        } else if let Some(error) = raw.error {
            Ok(Self::Err { error })
        } else {
            Err("response has none of collectedAudio, measureText, scene, components or error")
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ReadyMessage {
    Ready {
        config: ReactCompositionConfig,
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
    InvalidProperties {
        #[serde(rename = "invalidProperties")]
        invalid_properties: Vec<PropertyIssue>,
    },
    Error {
        error: String,
    },
}
