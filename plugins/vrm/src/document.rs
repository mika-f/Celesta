//! Opening a `.vrm` file: the glTF document, its buffers, and the VRM
//! extensions, refused with a message the user can act on when it is not a
//! VRM 1.0 avatar. Images are left encoded; the materials read them.

use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::Value;
use std::borrow::Cow;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use vrm_spec::vrmc_materials_mtoon_1_0::{VRMC_MATERIALS_MTOON, VrmcMaterialsMtoonSchema};
use vrm_spec::vrmc_vrm_1_0::{HumanBoneName, VRMCVrmSchema};

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

/// The largest JSON the plugin parses: a VRM's is a few MB. Parsed JSON
/// takes many times its size, and the budgets count only what is read
/// from the buffers, so the JSON is refused past this before it is parsed.
pub(crate) const JSON_LIMIT: usize = 64 << 20;

/// The largest file the plugin reads, far past any avatar's.
pub(crate) const FILE_LIMIT: u64 = 1 << 30;

/// The file at `path`, refused past [`FILE_LIMIT`]: by its size before it
/// is read, and while it is read for a file whose size says nothing (a
/// device such as `/dev/zero`).
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, LoadError> {
    let too_large = || {
        LoadError::Invalid(format!(
            "the file is larger than the {} MiB the VRM plugin reads",
            FILE_LIMIT >> 20
        ))
    };
    let file = File::open(path)?;
    if file.metadata()?.len() > FILE_LIMIT {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(too_large());
    }
    Ok(bytes)
}

/// What the root says about VRM before glTF reads it: the rest of the JSON
/// is skipped, not parsed.
#[derive(Deserialize)]
struct Head {
    #[serde(default)]
    extensions: Option<HeadExtensions>,
}

#[derive(Deserialize)]
struct HeadExtensions {
    #[serde(rename = "VRMC_vrm", default)]
    vrm: Option<Value>,
    #[serde(rename = "VRM", default)]
    vrm0: Option<IgnoredAny>,
}

/// A VRM 1.0 file, read but not yet turned into a scene. Extensions
/// `vrm-spec` has no types for are in gltf-json's `extensions().others`
/// of the object holding them.
pub(crate) struct Document {
    pub gltf: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub vrm: VRMCVrmSchema,
    /// Each material's `VRMC_materials_mtoon`, by glTF material index.
    pub mtoon: Vec<Option<VrmcMaterialsMtoonSchema>>,
}

impl Document {
    /// Reads a `.vrm` (binary glTF) file, or a glTF JSON file whose buffers
    /// are `data:` URIs. JSON past [`JSON_LIMIT`] is refused: a `.vrm`'s JSON
    /// chunk, or a glTF JSON file as a whole, its `data:` buffers included.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, LoadError> {
        let (json, blob) = if bytes.starts_with(b"glTF") {
            let glb = gltf::binary::Glb::from_slice(bytes)?;
            (glb.json, glb.bin.map(Cow::into_owned))
        } else {
            (Cow::Borrowed(bytes), None)
        };
        if json.len() > JSON_LIMIT {
            return Err(LoadError::Invalid(format!(
                "the glTF JSON is {} bytes, more than the {} MiB the VRM plugin reads",
                json.len(),
                JSON_LIMIT >> 20
            )));
        }

        // Which VRM this is comes first, before glTF's own validation, so a
        // VRM 0.x file always gets the conversion hint.
        let head: Head = serde_json::from_slice(&json).map_err(gltf::Error::Deserialize)?;
        let (vrm, vrm0) = head
            .extensions
            .map_or((None, None), |extensions| (extensions.vrm, extensions.vrm0));
        let Some(vrm) = vrm else {
            return Err(LoadError::Invalid(if vrm0.is_none() {
                "this glTF file has no VRMC_vrm extension: it is not a VRM 1.0 avatar".into()
            } else {
                "this is a VRM 0.x avatar, which the VRM plugin does not open yet; convert it \
                 to VRM 1.0 with UniVRM or the VRM Add-on for Blender"
                    .into()
            }));
        };
        // The schema requires a `specVersion`, which `vrm-spec` does not
        // read, but its value is not checked: UniVRM's importer reads the
        // extension whatever its version (`Vrm10Data.Parse`,
        // `Vrm10Data.cs` lines 30-36 at `efd20f62`, a bare `TryGet`), so a
        // file is read as UniVRM reads it and only its fields refuse it.
        if !vrm["specVersion"].is_string() {
            return Err(LoadError::Invalid("VRMC_vrm has no specVersion".into()));
        }
        let vrm = VRMCVrmSchema::deserialize(vrm)
            .map_err(|error| LoadError::Invalid(format!("VRMC_vrm is malformed: {error}")))?;

        let root = gltf::json::Root::from_slice(&json).map_err(gltf::Error::Deserialize)?;
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

        let mtoon = gltf
            .materials()
            .enumerate()
            .map(|(index, material)| {
                let Some(value) = material
                    .extension_value(VRMC_MATERIALS_MTOON)
                    .filter(|value| !value.is_null())
                else {
                    return Ok(None);
                };
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
    fn any_spec_version_is_read_as_univrm_reads_it() {
        for version in ["1.0-beta", "2.0", "0.9"] {
            let document = Document::parse(
                &SyntheticVrm {
                    spec_version: version.into(),
                    ..SyntheticVrm::avatar()
                }
                .glb(),
            )
            .unwrap_or_else(|error| panic!("{version}: {error}"));
            assert_eq!(document.vrm.meta.name, "Synthetic");
        }
        // The schema still requires one.
        let glb = SyntheticVrm::avatar().glb_with(|root| {
            let vrm = root["extensions"]["VRMC_vrm"].as_object_mut().unwrap();
            vrm.remove("specVersion");
        });
        let message = match Document::parse(&glb) {
            Ok(_) => panic!("the file opened"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("VRMC_vrm has no specVersion"), "{message}");
    }

    #[test]
    fn missing_required_bone_is_refused() {
        let message = refusal(&SyntheticVrm {
            omit_bone: Some("leftFoot".into()),
            ..SyntheticVrm::avatar()
        });
        assert!(message.contains("no leftFoot bone"), "{message}");
    }

    #[test]
    #[should_panic(expected = "omit_bone names leftFot")]
    fn misspelled_omitted_bone_fails_the_fixture() {
        SyntheticVrm {
            omit_bone: Some("leftFot".into()),
            ..SyntheticVrm::avatar()
        }
        .glb();
    }

    /// The reason `bytes` did not open, whatever kind it is.
    fn error(bytes: &[u8]) -> String {
        match Document::parse(bytes) {
            Ok(_) => panic!("the file opened"),
            Err(error) => error.to_string(),
        }
    }

    fn edited(edit: impl FnOnce(&mut Value)) -> String {
        error(&SyntheticVrm::avatar().glb_with(edit))
    }

    #[test]
    fn external_buffers_are_refused() {
        let message = edited(|root| root["buffers"][0]["uri"] = "avatar.bin".into());
        assert!(
            message.contains("buffer 0 is an external file"),
            "{message}"
        );
    }

    #[test]
    fn bone_past_the_nodes_is_refused() {
        let message = edited(|root| {
            root["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["hips"]["node"] = 999.into();
        });
        assert!(
            message.contains("humanoid bone hips names node 999, but the file has 25 nodes"),
            "{message}"
        );
    }

    #[test]
    fn two_bones_on_one_node_are_refused() {
        let message = edited(|root| {
            root["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["leftEye"]["node"] = 5.into();
        });
        assert!(
            message.contains("humanoid bone leftEye names node 5, which bone head names too"),
            "{message}"
        );
    }

    #[test]
    fn truncated_files_are_errors() {
        let glb = SyntheticVrm::avatar().glb();
        for length in [0, 4, 11, 12, 19, 20, 100, glb.len() / 2, glb.len() - 1] {
            error(&glb[..length]);
        }
        // A header and chunk headers that claim more than the file holds.
        let mut lying = glb[..20].to_vec();
        lying[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        lying[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        error(&lying);
        let mut short_bin = glb.clone();
        let json = u32::from_le_bytes([glb[12], glb[13], glb[14], glb[15]]) as usize;
        short_bin[20 + json..24 + json].copy_from_slice(&u32::MAX.to_le_bytes());
        error(&short_bin);
    }

    /// `glb` with its JSON chunk padded with spaces to at least `length`.
    fn padded_json(glb: &[u8], length: usize) -> Vec<u8> {
        let parsed = gltf::binary::Glb::from_slice(glb).unwrap();
        let mut json = parsed.json.into_owned();
        json.resize(length.next_multiple_of(4), b' ');
        let bin = parsed.bin.unwrap();
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }

    #[test]
    fn json_past_its_limit_is_refused_before_it_is_parsed() {
        let glb = SyntheticVrm::avatar().glb();
        Document::parse(&padded_json(&glb, JSON_LIMIT)).unwrap();
        let message = error(&padded_json(&glb, JSON_LIMIT + 1));
        let expected = format!(
            "the glTF JSON is {} bytes, more than the 64 MiB the VRM plugin reads",
            JSON_LIMIT + 4
        );
        assert!(message.contains(&expected), "{message}");
        // A glTF JSON file is its JSON.
        let mut json = br#"{"asset": {"version": "2.0"}}"#.to_vec();
        json.resize(JSON_LIMIT + 1, b' ');
        let message = error(&json);
        assert!(
            message.contains("the 64 MiB the VRM plugin reads"),
            "{message}"
        );
    }

    #[test]
    fn files_past_their_limit_are_refused_before_they_are_read() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("huge.vrm");
        let file = std::fs::File::create(&path).unwrap();
        // Sparse: nothing is written.
        file.set_len(FILE_LIMIT + 1).unwrap();
        let message = match read_file(&path) {
            Ok(_) => panic!("the file was read"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("the file is larger than the 1024 MiB the VRM plugin reads"),
            "{message}"
        );
        let glb = SyntheticVrm::avatar().glb();
        std::fs::write(&path, &glb).unwrap();
        assert_eq!(read_file(&path).unwrap(), glb);
    }

    #[test]
    fn garbage_is_an_error() {
        error(b"\x00\x01\x02 not a model at all");
        error(b"glTF\x02\x00\x00\x00garbage garbage garbage");
        error(b"{\"asset\": ");
        error(&[0xff; 64]);
    }
}
