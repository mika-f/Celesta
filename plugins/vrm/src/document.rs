//! Opening a `.vrm` file: the glTF document, its buffers, and the VRM
//! extensions, refused with a message the user can act on when it is not a
//! VRM 1.0 avatar. Images are left encoded; the materials read them.

use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
use vrm_spec::vrmc_materials_mtoon_1_0::{VRMC_MATERIALS_MTOON, VrmcMaterialsMtoonSchema};
use vrm_spec::vrmc_vrm_1_0::{HumanBoneName, VRMC_VRM, VRMCVrmSchema};

/// Why a file did not open.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file is readable but not something the plugin opens.
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Gltf(#[from] gltf::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The bones VRM 1.0 requires of every humanoid.
const REQUIRED_BONES: [(HumanBoneName, &str); 15] = [
    (HumanBoneName::Hips, "hips"),
    (HumanBoneName::Spine, "spine"),
    (HumanBoneName::Head, "head"),
    (HumanBoneName::LeftUpperArm, "leftUpperArm"),
    (HumanBoneName::LeftLowerArm, "leftLowerArm"),
    (HumanBoneName::LeftHand, "leftHand"),
    (HumanBoneName::RightUpperArm, "rightUpperArm"),
    (HumanBoneName::RightLowerArm, "rightLowerArm"),
    (HumanBoneName::RightHand, "rightHand"),
    (HumanBoneName::LeftUpperLeg, "leftUpperLeg"),
    (HumanBoneName::LeftLowerLeg, "leftLowerLeg"),
    (HumanBoneName::LeftFoot, "leftFoot"),
    (HumanBoneName::RightUpperLeg, "rightUpperLeg"),
    (HumanBoneName::RightLowerLeg, "rightLowerLeg"),
    (HumanBoneName::RightFoot, "rightFoot"),
];

/// A VRM 1.0 file, read but not yet turned into a scene.
// The geometry, materials, and model (later tasks) read every field.
#[allow(dead_code)]
pub(crate) struct Document {
    pub gltf: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub vrm: VRMCVrmSchema,
    /// Each material's `VRMC_materials_mtoon`, by glTF material index.
    pub mtoon: Vec<Option<VrmcMaterialsMtoonSchema>>,
    /// The root JSON, for extensions `vrm-spec` has no types for.
    pub raw: Value,
}

// Read by the geometry, materials, and model (later tasks).
#[allow(dead_code)]
impl Document {
    /// Reads a `.vrm` (binary glTF) file, or a glTF JSON file whose buffers
    /// are `data:` URIs.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, LoadError> {
        let (raw, blob): (Value, _) = if bytes.starts_with(b"glTF") {
            let glb = gltf::binary::Glb::from_slice(bytes)?;
            let raw = serde_json::from_slice(&glb.json).map_err(gltf::Error::Deserialize)?;
            (raw, glb.bin.map(Cow::into_owned))
        } else {
            let raw = serde_json::from_slice(bytes).map_err(gltf::Error::Deserialize)?;
            (raw, None)
        };

        // Which VRM this is comes first, before glTF's own validation, so a
        // VRM 0.x file always gets the conversion hint.
        let extensions = &raw["extensions"];
        let vrm = &extensions[VRMC_VRM];
        if vrm.is_null() {
            return Err(LoadError::Invalid(if extensions["VRM"].is_null() {
                "this glTF file has no VRMC_vrm extension: it is not a VRM 1.0 avatar".into()
            } else {
                "this is a VRM 0.x avatar, which the VRM plugin does not open yet; convert it \
                 to VRM 1.0 with UniVRM or the VRM Add-on for Blender"
                    .into()
            }));
        }
        match vrm["specVersion"].as_str() {
            Some(version) if version == "1.0" || version.starts_with("1.0-") => {}
            Some(version) => {
                return Err(LoadError::Invalid(format!(
                    "VRMC_vrm specVersion {version} is not 1.0"
                )));
            }
            None => {
                return Err(LoadError::Invalid("VRMC_vrm has no specVersion".into()));
            }
        }
        let vrm = VRMCVrmSchema::deserialize(vrm)
            .map_err(|error| LoadError::Invalid(format!("VRMC_vrm is malformed: {error}")))?;

        let root = gltf::json::Root::deserialize(&raw).map_err(gltf::Error::Deserialize)?;
        let gltf = gltf::Document::from_json(root)?;
        check_humanoid(&vrm, gltf.nodes().len())?;

        for buffer in gltf.buffers() {
            if let gltf::buffer::Source::Uri(uri) = buffer.source()
                && !uri.starts_with("data:")
            {
                return Err(LoadError::Invalid(format!(
                    "buffer {} is an external file; a .vrm keeps everything in itself",
                    buffer.index()
                )));
            }
        }
        let buffers = gltf::import_buffers(&gltf, None, blob)?;

        let mtoon = (0..gltf.materials().len())
            .map(|index| {
                let value = &raw["materials"][index]["extensions"][VRMC_MATERIALS_MTOON];
                if value.is_null() {
                    return Ok(None);
                }
                VrmcMaterialsMtoonSchema::deserialize(value)
                    .map(Some)
                    .map_err(|error| {
                        LoadError::Invalid(format!(
                            "material {index}: VRMC_materials_mtoon is malformed: {error}"
                        ))
                    })
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            gltf,
            buffers,
            vrm,
            mtoon,
            raw,
        })
    }
}

/// Every required bone is there, and each bone names a node of its own.
fn check_humanoid(vrm: &VRMCVrmSchema, nodes: usize) -> Result<(), LoadError> {
    let bones = &vrm.humanoid.human_bones.0;
    let node = |bone: &HumanBoneName| Some(bones.get(bone)?.as_ref()?.node?.value());
    for (bone, name) in REQUIRED_BONES {
        if node(&bone).is_none() {
            return Err(LoadError::Invalid(format!(
                "the humanoid has no {name} bone, which VRM 1.0 requires"
            )));
        }
    }

    // In name order, so the message does not depend on the map's order.
    let mut named: Vec<(String, usize)> = bones
        .keys()
        .filter_map(|bone| Some((bone_name(bone), node(bone)?)))
        .collect();
    named.sort();
    for (bone, index) in &named {
        if *index >= nodes {
            return Err(LoadError::Invalid(format!(
                "humanoid bone {bone} names node {index}, but the file has {nodes} nodes"
            )));
        }
    }
    named.sort_by(|(a, a_node), (b, b_node)| a_node.cmp(b_node).then(a.cmp(b)));
    for pair in named.windows(2) {
        let [(first, index), (second, other)] = pair else {
            continue;
        };
        if index == other {
            return Err(LoadError::Invalid(format!(
                "humanoid bone {second} names node {index}, which bone {first} names too"
            )));
        }
    }
    Ok(())
}

/// A bone's name as VRM writes it (`leftUpperArm`).
fn bone_name(bone: &HumanBoneName) -> String {
    match serde_json::to_value(bone) {
        Ok(Value::String(name)) => name,
        _ => format!("{bone:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::SyntheticVrm;
    use serde_json::Value;

    fn refusal(vrm: &SyntheticVrm) -> String {
        match Document::parse(&vrm.glb()) {
            Ok(_) => panic!("the file opened"),
            Err(LoadError::Invalid(message)) => message,
            Err(error) => panic!("not a refusal: {error}"),
        }
    }

    #[test]
    fn synthetic_avatar_parses() {
        let document = Document::parse(&SyntheticVrm::avatar().glb()).unwrap();
        assert_eq!(document.gltf.nodes().count(), 25);
        assert_eq!(document.vrm.humanoid.human_bones.0.len(), 19);
        assert_eq!(document.vrm.meta.name, "Synthetic");
    }

    #[test]
    fn vrm0_is_refused_with_the_conversion_hint() {
        let message = refusal(&SyntheticVrm {
            vrm0: true,
            ..SyntheticVrm::avatar()
        });
        assert!(message.contains("VRM 0.x"), "{message}");
        assert!(message.contains("UniVRM"), "{message}");
        assert!(message.contains("VRM Add-on for Blender"), "{message}");
    }

    #[test]
    fn plain_gltf_is_refused() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.extensions.insert("VRMC_vrm".into(), Value::Null);
        let message = refusal(&vrm);
        assert!(message.contains("no VRMC_vrm extension"), "{message}");
    }

    #[test]
    fn spec_version_other_than_1_0_is_refused() {
        let message = refusal(&SyntheticVrm {
            spec_version: "0.9".into(),
            ..SyntheticVrm::avatar()
        });
        assert!(message.contains("specVersion 0.9"), "{message}");
        Document::parse(
            &SyntheticVrm {
                spec_version: "1.0-beta".into(),
                ..SyntheticVrm::avatar()
            }
            .glb(),
        )
        .unwrap();
    }

    #[test]
    fn missing_required_bone_is_refused() {
        let message = refusal(&SyntheticVrm {
            omit_bone: Some("leftFoot".into()),
            ..SyntheticVrm::avatar()
        });
        assert!(message.contains("no leftFoot bone"), "{message}");
    }
}
