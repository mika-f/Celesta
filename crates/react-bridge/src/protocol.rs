use crate::decode::DecodeAudioRequest;
use crate::measure::MeasureTextRequest;
use crate::occurrences::OccurrenceIndex;
use crate::probe::MediaProbeRequest;
use crate::properties::{PropertyIssue, PropertyLayer};
use crate::statics::StaticDefinition;
use crate::types::{
    ComponentPropertyField, ComponentPropertySchema, FrameSpan, LayerSource, LayerSourceReport,
    LegacyImport, PrepareDependency, PrimitivePropertyTable, PropOverride,
    ReactAudioClipDescriptor, ReactCompositionConfig, ReactCompositionInfo,
    ReactSubtitleDescriptor, ReceivedProps, TimingWindow,
};
use celesta_composition::{Layer, Scene, ShaderSource, Time};
use serde::de::{Error as _, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

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
    /// Values elements take in this frame only (see
    /// [`crate::ReactBridge::scene_at_with_overrides`]); left out when empty.
    #[serde(rename = "propOverrides", skip_serializing_if = "<[_]>::is_empty")]
    pub(crate) prop_overrides: &'a [PropOverride],
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
    CollectedOccurrences {
        #[serde(rename = "collectedOccurrences")]
        collected_occurrences: OccurrenceIndex,
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
        report: Option<LayerSourceReport>,
    },
    Err {
        error: String,
    },
}

/// The body of a `layerSource` reply.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayerSourceReply {
    pub(crate) chain: Option<Vec<LayerSource>>,
    #[serde(default)]
    pub(crate) local_frame: f64,
    /// Absent from older runtimes, which the editor reads as `[localFrame]`.
    #[serde(default)]
    pub(crate) candidate_frames: Vec<f64>,
    #[serde(default)]
    pub(crate) props: ReceivedProps,
    /// Absent from runtimes that do not count; read as 1 so nothing is taken
    /// for a repeated element by accident.
    #[serde(default = "one")]
    pub(crate) render_count: u32,
    /// Absent from older runtimes.
    #[serde(default)]
    pub(crate) windows: Vec<TimingWindow>,
    /// Absent from older runtimes.
    #[serde(default)]
    pub(crate) span: Option<FrameSpan>,
}

fn one() -> u32 {
    1
}

impl From<LayerSourceReply> for Option<LayerSourceReport> {
    fn from(reply: LayerSourceReply) -> Self {
        Some(LayerSourceReport {
            chain: reply.chain?,
            local_frame: reply.local_frame,
            candidate_frames: reply.candidate_frames,
            props: reply.props,
            render_count: reply.render_count,
            windows: reply.windows,
            span: reply.span,
        })
    }
}

/// Every key a [`Response`] can carry, read in one pass. An untagged enum
/// would buffer the whole message (a frame's scene is hundreds of KiB) and
/// replay it against each variant in turn, more than doubling parse time.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawResponse {
    pub(crate) collected_audio: Option<Vec<ReactAudioClipDescriptor>>,
    pub(crate) collected_subtitles: Option<Vec<ReactSubtitleDescriptor>>,
    pub(crate) collected_occurrences: Option<OccurrenceIndex>,
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
        } else if let Some(collected_occurrences) = raw.collected_occurrences {
            Ok(Self::CollectedOccurrences {
                collected_occurrences,
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
            Ok(Self::LayerSource {
                report: reply.into(),
            })
        } else if let Some(error) = raw.error {
            Ok(Self::Err { error })
        } else {
            Err(
                "response has none of collectedAudio, collectedSubtitles, collectedOccurrences, measureText, scene, components, layerSource or error",
            )
        }
    }
}

/// A message the runtime sends before it is ready, or the ready message
/// itself. Read in one pass by [`RawReady`], so a field that fails to parse
/// is named in the error.
#[expect(
    clippy::large_enum_variant,
    reason = "read a handful of times per spawn, never stored"
)]
pub(crate) enum ReadyMessage {
    Ready {
        config: ReactCompositionConfig,
        composition_id: Option<String>,
        compositions: Vec<ReactCompositionInfo>,
        component_schemas: BTreeMap<String, ComponentPropertySchema>,
        primitive_properties: BTreeMap<String, PrimitivePropertyTable>,
        /// `null` (or absent) when the entry never called
        /// `defineProjectProperties()`; an empty object means it declared an
        /// intentionally empty schema.
        project_property_schema: Option<BTreeMap<String, ComponentPropertyField>>,
        /// `project_property_schema`'s keys in the order the entry declared
        /// them, which the `BTreeMap` sorts away.
        project_property_order: Vec<String>,
        dependencies: Vec<PrepareDependency>,
        legacy_imports: Vec<LegacyImport>,
    },
    ProbeMedia {
        probe_media: MediaProbeRequest,
    },
    MeasureText {
        measure_text: Box<MeasureTextRequest>,
    },
    DecodeAudio {
        decode_audio: DecodeAudioRequest,
    },
    InvalidProperties {
        invalid_properties: Vec<PropertyIssue>,
    },
    Error {
        error: String,
    },
}

impl<'de> Deserialize<'de> for ReadyMessage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RawReady::deserialize(deserializer)?
            .try_into()
            .map_err(D::Error::custom)
    }
}

/// Every key a [`ReadyMessage`] can carry. An untagged enum would report
/// only that the message matched no variant; this names the key whose value
/// failed to parse, such as a malformed `componentSchemas` entry from a
/// runtime of another version.
#[derive(Default)]
struct RawReady {
    config: Option<ReactCompositionConfig>,
    composition_id: Option<String>,
    compositions: Vec<ReactCompositionInfo>,
    component_schemas: BTreeMap<String, ComponentPropertySchema>,
    primitive_properties: BTreeMap<String, PrimitivePropertyTable>,
    property_schema: Option<Ordered<ComponentPropertyField>>,
    dependencies: Vec<PrepareDependency>,
    legacy_imports: Vec<LegacyImport>,
    probe_media: Option<MediaProbeRequest>,
    measure_text: Option<Box<MeasureTextRequest>>,
    decode_audio: Option<DecodeAudioRequest>,
    invalid_properties: Option<Vec<PropertyIssue>>,
    error: Option<String>,
}

/// The keys [`RawReady`] reads; others are skipped.
const READY_KEYS: [&str; 13] = [
    "config",
    "compositionId",
    "compositions",
    "componentSchemas",
    "primitiveProperties",
    "propertySchema",
    "dependencies",
    "legacyImports",
    "probeMedia",
    "measureText",
    "decodeAudio",
    "invalidProperties",
    "error",
];

impl<'de> Deserialize<'de> for RawReady {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RawVisitor;

        impl<'de> Visitor<'de> for RawVisitor {
            type Value = RawReady;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a message from the React runtime")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawReady, A::Error> {
                let mut raw = RawReady::default();
                let mut seen: Vec<&'static str> = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    let Some(&known) = READY_KEYS.iter().find(|known| **known == key) else {
                        map.next_value::<IgnoredAny>()?;
                        continue;
                    };
                    // As a derived struct does, rather than keep the last.
                    if seen.contains(&known) {
                        return Err(A::Error::duplicate_field(known));
                    }
                    seen.push(known);
                    match known {
                        "config" => raw.config = Some(value(&mut map, known)?),
                        "compositionId" => raw.composition_id = value(&mut map, known)?,
                        "compositions" => raw.compositions = value(&mut map, known)?,
                        "componentSchemas" => raw.component_schemas = value(&mut map, known)?,
                        "primitiveProperties" => {
                            raw.primitive_properties = value(&mut map, known)?;
                        }
                        "propertySchema" => raw.property_schema = value(&mut map, known)?,
                        "dependencies" => raw.dependencies = value(&mut map, known)?,
                        "legacyImports" => raw.legacy_imports = value(&mut map, known)?,
                        "probeMedia" => raw.probe_media = Some(value(&mut map, known)?),
                        "measureText" => raw.measure_text = Some(value(&mut map, known)?),
                        "decodeAudio" => raw.decode_audio = Some(value(&mut map, known)?),
                        "invalidProperties" => {
                            raw.invalid_properties = Some(value(&mut map, known)?);
                        }
                        _ => raw.error = Some(value(&mut map, known)?),
                    }
                }
                Ok(raw)
            }
        }

        deserializer.deserialize_map(RawVisitor)
    }
}

/// The value of `key`, with `key` named in its error.
fn value<'de, A: MapAccess<'de>, T: Deserialize<'de>>(
    map: &mut A,
    key: &str,
) -> Result<T, A::Error> {
    map.next_value()
        .map_err(|error| A::Error::custom(format_args!("`{key}`: {error}")))
}

impl TryFrom<RawReady> for ReadyMessage {
    type Error = &'static str;

    fn try_from(raw: RawReady) -> Result<Self, &'static str> {
        // The order the untagged enum used to try its variants in.
        if let Some(config) = raw.config {
            let mut project_property_order = Vec::new();
            let project_property_schema = raw.property_schema.map(|schema| {
                let mut fields = BTreeMap::new();
                for (key, field) in schema.0 {
                    if fields.insert(key.clone(), field).is_none() {
                        project_property_order.push(key);
                    }
                }
                fields
            });
            Ok(Self::Ready {
                config,
                composition_id: raw.composition_id,
                compositions: raw.compositions,
                component_schemas: raw.component_schemas,
                primitive_properties: raw.primitive_properties,
                project_property_schema,
                project_property_order,
                dependencies: raw.dependencies,
                legacy_imports: raw.legacy_imports,
            })
        } else if let Some(probe_media) = raw.probe_media {
            Ok(Self::ProbeMedia { probe_media })
        } else if let Some(measure_text) = raw.measure_text {
            Ok(Self::MeasureText { measure_text })
        } else if let Some(decode_audio) = raw.decode_audio {
            Ok(Self::DecodeAudio { decode_audio })
        } else if let Some(invalid_properties) = raw.invalid_properties {
            Ok(Self::InvalidProperties { invalid_properties })
        } else if let Some(error) = raw.error {
            Ok(Self::Error { error })
        } else {
            Err(
                "message has none of config, probeMedia, measureText, decodeAudio, invalidProperties or error",
            )
        }
    }
}

/// A JSON object's entries in the order they appear in the text.
struct Ordered<V>(Vec<(String, V)>);

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for OrderedVisitor<V> {
            type Value = Ordered<V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Ordered(entries))
            }
        }

        deserializer.deserialize_map(OrderedVisitor(PhantomData))
    }
}
