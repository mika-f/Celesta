//! The humanoid: which node each VRM bone is.

use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vrm_spec::vrmc_vrm_1_0::HumanBoneName;

/// `HumanBone`, and its conversions with `vrm-spec`'s `HumanBoneName`, from
/// one list of names.
macro_rules! human_bones {
    ($($bone:ident),* $(,)?) => {
        /// A VRM 1.0 humanoid bone. `vrm-spec`'s `HumanBoneName` with an
        /// order (by VRM name, as the variants are listed), so a bone can
        /// key a `BTreeMap`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub enum HumanBone {
            $($bone),*
        }

        impl From<HumanBoneName> for HumanBone {
            fn from(name: HumanBoneName) -> Self {
                match name {
                    $(HumanBoneName::$bone => Self::$bone),*
                }
            }
        }

        impl From<HumanBone> for HumanBoneName {
            fn from(bone: HumanBone) -> Self {
                match bone {
                    $(HumanBone::$bone => Self::$bone),*
                }
            }
        }
    };
}

human_bones!(
    Chest,
    Head,
    Hips,
    Jaw,
    LeftEye,
    LeftFoot,
    LeftHand,
    LeftIndexDistal,
    LeftIndexIntermediate,
    LeftIndexProximal,
    LeftLittleDistal,
    LeftLittleIntermediate,
    LeftLittleProximal,
    LeftLowerArm,
    LeftLowerLeg,
    LeftMiddleDistal,
    LeftMiddleIntermediate,
    LeftMiddleProximal,
    LeftRingDistal,
    LeftRingIntermediate,
    LeftRingProximal,
    LeftShoulder,
    LeftThumbDistal,
    LeftThumbMetacarpal,
    LeftThumbProximal,
    LeftToes,
    LeftUpperArm,
    LeftUpperLeg,
    Neck,
    RightEye,
    RightFoot,
    RightHand,
    RightIndexDistal,
    RightIndexIntermediate,
    RightIndexProximal,
    RightLittleDistal,
    RightLittleIntermediate,
    RightLittleProximal,
    RightLowerArm,
    RightLowerLeg,
    RightMiddleDistal,
    RightMiddleIntermediate,
    RightMiddleProximal,
    RightRingDistal,
    RightRingIntermediate,
    RightRingProximal,
    RightShoulder,
    RightThumbDistal,
    RightThumbMetacarpal,
    RightThumbProximal,
    RightToes,
    RightUpperArm,
    RightUpperLeg,
    Spine,
    UpperChest,
);

/// Which node each of the avatar's bones is.
#[derive(Clone, Debug, PartialEq)]
pub struct Humanoid {
    pub bones: BTreeMap<HumanBone, usize>,
}

impl Humanoid {
    /// The humanoid of a parsed file, whose bones `Document::parse` has
    /// checked: every required bone is there, each on a node of its own
    /// within the file.
    // Called by the model (Task 4).
    #[allow(dead_code)]
    pub(crate) fn new(document: &Document) -> Self {
        let bones = document
            .vrm
            .humanoid
            .human_bones
            .0
            .iter()
            .filter_map(|(name, bone)| Some((HumanBone::from(*name), bone.as_ref()?.node?.value())))
            .collect();
        Self { bones }
    }

    pub fn node(&self, bone: HumanBone) -> Option<usize> {
        self.bones.get(&bone).copied()
    }

    /// The node of a bone VRM 1.0 requires (hips, spine, head, the arms,
    /// hands, legs, and feet), which every opened avatar has.
    ///
    /// # Panics
    ///
    /// For a bone VRM 1.0 does not require that the avatar lacks.
    pub fn required(&self, bone: HumanBone) -> usize {
        self.node(bone)
            .unwrap_or_else(|| panic!("{bone:?} is not a bone VRM 1.0 requires"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::synthetic::{SyntheticVrm, node};

    #[test]
    fn humanoid_maps_bones_to_nodes() {
        let document = Document::parse(&SyntheticVrm::avatar().glb()).unwrap();
        let humanoid = Humanoid::new(&document);
        assert_eq!(humanoid.required(HumanBone::Hips), 1);
        assert_eq!(humanoid.node(HumanBone::LeftEye), Some(6));
        assert_eq!(humanoid.node(HumanBone::Jaw), None);
        assert_eq!(humanoid.bones.len(), 19);
        assert_eq!(
            humanoid.node(HumanBone::LeftUpperArm),
            Some(node::LEFT_UPPER_ARM)
        );
    }

    #[test]
    fn bones_keep_vrm_names() {
        assert_eq!(
            serde_json::to_value(HumanBone::LeftThumbMetacarpal).unwrap(),
            "leftThumbMetacarpal"
        );
        let bone: HumanBone = serde_json::from_value("upperChest".into()).unwrap();
        assert_eq!(bone, HumanBone::UpperChest);
    }
}
