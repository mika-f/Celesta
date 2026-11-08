//! Listing and describing media components, for the CLI's
//! `--list-components` and `--describe` and for MCP.

use crate::error::ExportError;
use celesta_media::{
    ComponentDescription, ComponentEntry, ComponentKind, ComponentRef, MediaComponentInfo,
    MediaRegistry,
};
use celesta_plugin_host::{MediaOrigin, PluginKillSwitch};
use serde_json::{Map, Value, json};

/// A component from its plugin's manifest, and with `check` whether its
/// plugin describes it as available.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedComponent {
    pub info: MediaComponentInfo,
    pub available: Option<bool>,
    pub reason: Option<String>,
}

/// Every installed component, from the manifests. With `check`, each
/// plugin is also asked to describe itself (once), and every component
/// says whether it is available.
pub fn list_components(
    registry: &MediaRegistry,
    check: bool,
    kill: &PluginKillSwitch,
) -> Vec<ListedComponent> {
    registry
        .components()
        .into_iter()
        .map(|info| {
            let described = check.then(|| description_of(registry, &info, kill));
            ListedComponent {
                available: described.as_ref().map(|described| described.available),
                reason: described.and_then(|described| described.reason),
                info,
            }
        })
        .collect()
}

/// The component `reference` names, whichever of the four kinds it is,
/// and what its plugin says of it.
pub fn describe_component(
    registry: &MediaRegistry,
    reference: &ComponentRef,
    kill: &PluginKillSwitch,
) -> Result<(MediaComponentInfo, ComponentDescription), ExportError> {
    let info = [
        ComponentKind::Muxer,
        ComponentKind::VideoEncoder,
        ComponentKind::AudioEncoder,
        ComponentKind::Decoder,
    ]
    .into_iter()
    .find_map(|kind| registry.component(reference, kind))
    .ok_or_else(|| ExportError::ComponentUnavailable {
        component: reference.clone(),
        reason: "not installed".to_owned(),
    })?;
    let description = description_of(registry, &info, kill);
    Ok((info, description))
}

fn description_of(
    registry: &MediaRegistry,
    info: &MediaComponentInfo,
    kill: &PluginKillSwitch,
) -> ComponentDescription {
    registry
        .describe(info.plugin, kill)
        .components
        .iter()
        .find(|described| described.kind == info.kind && described.id == info.reference.component)
        .cloned()
        .unwrap_or_else(|| {
            ComponentDescription::unavailable(
                info.kind,
                &info.reference.component,
                "the plugin did not describe it",
            )
        })
}

/// How a component's origin is named.
pub fn origin_label(origin: MediaOrigin) -> &'static str {
    match origin {
        MediaOrigin::User => "user",
        MediaOrigin::Bundled => "bundled",
        MediaOrigin::Project => "project",
    }
}

/// How a component's kind is named, as on the wire.
pub fn kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Decoder => "decoder",
        ComponentKind::VideoEncoder => "videoEncoder",
        ComponentKind::AudioEncoder => "audioEncoder",
        ComponentKind::Muxer => "muxer",
    }
}

/// `{"components": [...]}`: each entry's reference, kind, name, plugin,
/// origin, plugin version, its manifest fields, and with a check whether
/// it is available and why not.
pub fn listing_json(components: &[ListedComponent]) -> Value {
    let entries: Vec<Value> = components
        .iter()
        .map(|listed| {
            let mut entry = entry_json(&listed.info);
            if let Some(available) = listed.available {
                entry.insert("available".into(), available.into());
            }
            if let Some(reason) = &listed.reason {
                entry.insert("reason".into(), reason.clone().into());
            }
            Value::Object(entry)
        })
        .collect();
    json!({ "components": entries })
}

/// [`listing_json`]'s entry for the component, with what its plugin
/// describes: `available` and `reason`, its `settings`, and the input
/// formats, transports, sample rates, and channels an encoder takes.
pub fn description_json(info: &MediaComponentInfo, description: &ComponentDescription) -> Value {
    let mut entry = entry_json(info);
    entry.insert("available".into(), description.available.into());
    if let Some(reason) = &description.reason {
        entry.insert("reason".into(), reason.clone().into());
    }
    // Serializing the description leaves out what is empty, but every
    // description has settings.
    let Value::Object(described) = serde_json::to_value(description).unwrap_or_default() else {
        unreachable!("a description serializes as an object");
    };
    entry.insert(
        "settings".into(),
        described
            .get("settings")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    for key in ["inputFormats", "transports", "sampleRates", "channels"] {
        if let Some(value) = described.get(key) {
            entry.insert(key.into(), value.clone());
        }
    }
    Value::Object(entry)
}

fn entry_json(info: &MediaComponentInfo) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("id".into(), info.reference.to_string().into());
    entry.insert("kind".into(), kind_label(info.kind).into());
    entry.insert("name".into(), info.name.clone().into());
    entry.insert("plugin".into(), info.reference.plugin.clone().into());
    entry.insert("origin".into(), origin_label(info.origin).into());
    entry.insert("version".into(), info.plugin_version.clone().into());
    let manifest = match &info.entry {
        ComponentEntry::Decoder(entry) => serde_json::to_value(entry),
        ComponentEntry::VideoEncoder(entry) => serde_json::to_value(entry),
        ComponentEntry::AudioEncoder(entry) => serde_json::to_value(entry),
        ComponentEntry::Muxer(entry) => serde_json::to_value(entry),
    };
    if let Ok(Value::Object(fields)) = manifest {
        for (key, value) in fields {
            // The reference and name above stand for the entry's own.
            if key != "id" && key != "name" {
                entry.insert(key, value);
            }
        }
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_media::{MediaRegistry, RegisteredPlugin};
    use celesta_plugin_host::{
        MediaManifest, MediaMuxerEntry, MediaVideoEncoderEntry, PluginKillSwitch,
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    fn registry() -> MediaRegistry {
        let media = MediaManifest {
            video_encoders: vec![MediaVideoEncoderEntry {
                id: "video".into(),
                name: "Video".into(),
                codecs: vec!["avc1".into()],
                input_formats: None,
            }],
            muxers: vec![MediaMuxerEntry {
                id: "mux".into(),
                name: "Mux".into(),
                extensions: vec!["mp4".into()],
                video: vec!["avc1".into()],
                audio: None,
                default_video: None,
                default_audio: None,
                trims_leading_audio: true,
                sequence: false,
            }],
            ..MediaManifest::default()
        };
        MediaRegistry::new(vec![RegisteredPlugin {
            id: "plug".into(),
            version: "1.2.3".into(),
            origin: MediaOrigin::User,
            directory: PathBuf::from("plug"),
            executable: None,
            media,
            factory: Arc::new(|_, _| {
                Err(celesta_media::MediaError::CodecUnavailable(
                    "no process".into(),
                ))
            }),
        }])
    }

    #[test]
    fn a_listing_omits_unset_fields_and_unchecked_availability() {
        let registry = registry();
        let listed = list_components(&registry, false, &PluginKillSwitch::new());
        let json = listing_json(&listed);
        let entries = json["components"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0],
            json!({
                "id": "plug/video", "kind": "videoEncoder", "name": "Video",
                "plugin": "plug", "origin": "user", "version": "1.2.3",
                "codecs": ["avc1"],
            })
        );
        assert_eq!(entries[1]["trimsLeadingAudio"], true);
        assert!(entries[1].get("audio").is_none());
        assert!(entries[1].get("available").is_none());
    }

    #[test]
    fn a_check_reports_a_plugin_that_cannot_start() {
        let registry = registry();
        let listed = list_components(&registry, true, &PluginKillSwitch::new());
        assert!(listed.iter().all(|listed| listed.available == Some(false)));
        assert!(listed[0].reason.as_deref().unwrap().contains("no process"));
    }

    #[test]
    fn an_unknown_component_is_not_installed() {
        let registry = registry();
        let reference = "plug/missing".parse().unwrap();
        let error = describe_component(&registry, &reference, &PluginKillSwitch::new())
            .expect_err("nothing is named that");
        assert_eq!(
            error.to_string(),
            "plug/missing is unavailable: not installed"
        );
    }
}
