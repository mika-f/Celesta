//! Listing and describing media components, for the CLI's
//! `--list-components` and `--describe` and for MCP.

use crate::error::ExportError;
use celesta_media::{
    ComponentDescription, ComponentEntry, ComponentKind, ComponentRef, MediaComponentInfo,
    MediaRegistry, PluginDescription,
};
use celesta_plugin_host::{MediaOrigin, PluginKillSwitch};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// A component from its plugin's manifest, and with `check` whether its
/// plugin describes it as available.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedComponent {
    pub info: MediaComponentInfo,
    pub available: Option<bool>,
    pub reason: Option<String>,
}

/// Every installed component, from the manifests. With `check`, each
/// plugin is also asked to describe itself (once per listing, even when
/// that fails, which the registry does not cache), and every component
/// says whether it is available.
pub fn list_components(
    registry: &MediaRegistry,
    check: bool,
    kill: &PluginKillSwitch,
) -> Vec<ListedComponent> {
    let mut plugins: HashMap<usize, Arc<PluginDescription>> = HashMap::new();
    registry
        .components()
        .into_iter()
        .map(|info| {
            let described = check.then(|| {
                let plugin = plugins
                    .entry(info.plugin)
                    .or_insert_with(|| registry.describe(info.plugin, kill));
                description_in(plugin, &info)
            });
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
        reason: crate::resolve::not_installed(registry, &reference.plugin),
    })?;
    let description = description_in(&registry.describe(info.plugin, kill), &info);
    Ok((info, description))
}

/// What `plugin`, the description of `info`'s plugin, says of `info`.
fn description_in(plugin: &PluginDescription, info: &MediaComponentInfo) -> ComponentDescription {
    plugin
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
        MediaOrigin::BuiltIn => "built-in",
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
        registry_counting(Arc::default())
    }

    /// [`registry`], counting in `starts` the times its plugin is started.
    fn registry_counting(starts: Arc<std::sync::atomic::AtomicUsize>) -> MediaRegistry {
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
            name: "plug".into(),
            version: "1.2.3".into(),
            origin: MediaOrigin::User,
            directory: PathBuf::from("plug"),
            executable: None,
            media,
            factory: Arc::new(move |_, _| {
                starts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(celesta_media::MediaError::CodecUnavailable(
                    "no process".into(),
                ))
            }),
        }])
    }

    #[test]
    fn built_ins_are_listed_without_a_folder() {
        let registry = crate::with_builtin(Vec::new());
        let listed = list_components(&registry, false, &PluginKillSwitch::new());
        let json = listing_json(&listed);
        let entries = json["components"].as_array().unwrap();
        let ids: Vec<&str> = entries
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect();
        for id in [
            "builtin/png",
            "builtin/jpeg",
            "builtin/webp",
            "builtin/image",
            "builtin/imageSequence",
        ] {
            assert!(ids.contains(&id), "{ids:?}");
        }
        assert!(
            entries
                .iter()
                .all(|entry| entry["origin"] == "built-in" && entry.get("directory").is_none())
        );
        assert!(!json.to_string().contains("directory"));

        let webp = "builtin/webp".parse().unwrap();
        let (info, description) =
            describe_component(&registry, &webp, &PluginKillSwitch::new()).unwrap();
        assert!(description.available, "{description:?}");
        let settings = description_json(&info, &description)["settings"].clone();
        assert_eq!(settings.as_array().unwrap().len(), 3, "{settings}");
    }

    #[test]
    fn the_built_in_origin_is_labelled() {
        assert_eq!(origin_label(MediaOrigin::BuiltIn), "built-in");
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

    /// A failed `describe` is not cached in the registry, but one listing
    /// still starts each plugin once, not once per component.
    #[test]
    fn a_check_starts_a_failing_plugin_once() {
        let starts = Arc::<std::sync::atomic::AtomicUsize>::default();
        let registry = registry_counting(starts.clone());
        let listed = list_components(&registry, true, &PluginKillSwitch::new());
        assert_eq!(listed.len(), 2);
        assert_eq!(starts.load(std::sync::atomic::Ordering::Relaxed), 1);
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

    #[test]
    fn a_component_of_a_skipped_program_says_why() {
        let registry = registry().with_skipped(vec![celesta_plugin_host::SkippedMedia {
            id: "gone".into(),
            program: "/bin/gone".into(),
            reason: "it could not be started".into(),
        }]);
        let reference = "gone/video".parse().unwrap();
        let error = describe_component(&registry, &reference, &PluginKillSwitch::new())
            .expect_err("its program was skipped");
        assert_eq!(
            error.to_string(),
            "gone/video is unavailable: skipped at discovery (/bin/gone): it could not be started"
        );
    }
}
