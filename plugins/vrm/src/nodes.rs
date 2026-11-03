//! The glTF node tree: who is whose parent, the order to walk it in, each
//! node's rest transform, and world matrices for a pose.

use crate::document::{Document, LoadError};
use glam::{Mat4, Quat, Vec3};
use gltf::scene::Transform;

/// A node's local transform: scale, then rotation, then translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Trs {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Every node of the file, those outside the default scene included (they
/// are kept but never drawn).
#[derive(Clone, Debug)]
pub struct NodeTree {
    pub parents: Vec<Option<usize>>,
    /// Each node's children, in the file's order.
    pub children: Vec<Vec<usize>>,
    /// Every node, each after its parent: the default scene's roots in its
    /// order, then any other root by index, each followed by its subtree.
    pub pre_order: Vec<usize>,
    pub rest: Vec<Trs>,
}

/// Every node's local transform.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub local: Vec<Trs>,
}

impl NodeTree {
    /// The tree of `document`'s nodes; a node with two parents or a cycle
    /// is refused, as is a transform that is not finite in `f32`.
    pub(crate) fn new(document: &Document) -> Result<Self, LoadError> {
        let gltf = &document.gltf;
        let count = gltf.nodes().len();
        let mut parents: Vec<Option<usize>> = vec![None; count];
        let mut children = vec![Vec::new(); count];
        let mut rest = Vec::with_capacity(count);
        for node in gltf.nodes() {
            let index = node.index();
            for child in node.children() {
                let child = child.index();
                let Some(parent) = parents.get_mut(child) else {
                    return Err(LoadError::Invalid(format!(
                        "node {index} names child {child}, but the file has {count} nodes"
                    )));
                };
                if let Some(first) = *parent {
                    return Err(LoadError::Invalid(if first == index {
                        format!("node {index} lists child {child} twice")
                    } else {
                        format!("node {child} is a child of both node {first} and node {index}")
                    }));
                }
                *parent = Some(index);
                children[index].push(child);
            }
            rest.push(rest_transform(&node)?);
        }

        // With one parent each, a walk down from the parentless nodes meets
        // every node once; a node it misses is on a cycle.
        let scene = gltf.default_scene().or_else(|| gltf.scenes().next());
        let scene_roots = scene
            .into_iter()
            .flat_map(|scene| scene.nodes().map(|node| node.index()));
        let mut pre_order = Vec::with_capacity(count);
        let mut visited = vec![false; count];
        let mut stack = Vec::new();
        for root in scene_roots.chain(0..count).filter(|&root| root < count) {
            if visited[root] || parents[root].is_some() {
                continue;
            }
            stack.push(root);
            while let Some(node) = stack.pop() {
                visited[node] = true;
                pre_order.push(node);
                stack.extend(children[node].iter().rev());
            }
        }
        if let Some(missed) = visited.iter().position(|seen| !seen) {
            // A missed node is on a cycle or under one: every missed node
            // has a parent, so climbing from it comes back round, within
            // `count` steps, to a node on the cycle.
            let mut climbed = vec![false; count];
            let mut node = missed;
            while !climbed[node] {
                climbed[node] = true;
                match parents[node] {
                    Some(parent) => node = parent,
                    None => break,
                }
            }
            return Err(LoadError::Invalid(format!(
                "node {node} is its own ancestor: the node hierarchy has a cycle"
            )));
        }

        Ok(Self {
            parents,
            children,
            pre_order,
            rest,
        })
    }

    pub fn rest_pose(&self) -> Pose {
        Pose {
            local: self.rest.clone(),
        }
    }

    /// Each node's model-space matrix for `pose` (one local transform per
    /// node): its parent's world times its local, walked in pre-order.
    ///
    /// # Panics
    ///
    /// When `pose` does not have one local transform per node: poses come
    /// from [`Self::rest_pose`] of the same tree.
    pub fn world(&self, pose: &Pose) -> Vec<Mat4> {
        assert_eq!(
            pose.local.len(),
            self.parents.len(),
            "a pose needs one local transform per node of its tree"
        );
        let mut worlds = vec![Mat4::IDENTITY; self.parents.len()];
        for &node in &self.pre_order {
            let local = pose.local[node].matrix();
            worlds[node] = match self.parents[node] {
                Some(parent) => worlds[parent] * local,
                None => local,
            };
        }
        worlds
    }
}

/// A node's rest transform. A matrix is decomposed as glTF's crate does; one
/// that scales an axis to 0 has no rotation to find, so it gets none.
fn rest_transform(node: &gltf::Node) -> Result<Trs, LoadError> {
    let transform = node.transform();
    let is_matrix = matches!(transform, Transform::Matrix { .. });
    let (translation, rotation, scale) = transform.decomposed();
    let mut rotation = Quat::from_array(rotation);
    if is_matrix && !rotation.is_finite() {
        rotation = Quat::IDENTITY;
    }
    let trs = Trs {
        translation: Vec3::from(translation),
        rotation,
        scale: Vec3::from(scale),
    };
    if !(trs.translation.is_finite() && trs.rotation.is_finite() && trs.scale.is_finite()) {
        return Err(LoadError::Invalid(format!(
            "node {}'s transform is not finite",
            node.index()
        )));
    }
    // glTF asks for unit quaternions; files round them.
    let length = trs.rotation.length_squared();
    let rotation = if length == 0.0 {
        Quat::IDENTITY
    } else if (length - 1.0).abs() > 1e-5 {
        trs.rotation.normalize()
    } else {
        trs.rotation
    };
    Ok(Trs { rotation, ..trs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::synthetic::{SyntheticVrm, node};
    use glam::{Quat, Vec3};
    use serde_json::{Value, json};

    fn tree(edit: impl FnOnce(&mut Value)) -> Result<NodeTree, LoadError> {
        // The file must open: only `NodeTree::new` may refuse it here.
        let document = match Document::parse(&SyntheticVrm::avatar().glb_with(edit)) {
            Ok(document) => document,
            Err(error) => panic!("the file did not parse: {error}"),
        };
        NodeTree::new(&document)
    }

    fn refusal(edit: impl FnOnce(&mut Value)) -> String {
        match tree(edit) {
            Ok(_) => panic!("the tree was built"),
            Err(error) => error.to_string(),
        }
    }

    fn near(actual: Vec3, expected: Vec3) -> bool {
        (actual - expected).abs().max_element() < 1e-5
    }

    #[test]
    fn rest_world_matches_the_avatar_positions() {
        let tree = tree(|_| {}).unwrap();
        let worlds = tree.world(&tree.rest_pose());
        let hand = worlds[node::LEFT_HAND].w_axis.truncate();
        let head = worlds[node::HEAD].w_axis.truncate();
        assert!(near(hand, Vec3::new(0.62, 1.25, 0.0)), "{hand}");
        assert!(near(head, Vec3::new(0.0, 1.4, 0.0)), "{head}");
        assert_eq!(tree.pre_order, (0..25).collect::<Vec<_>>());
        assert_eq!(tree.parents[node::LEFT_HAND], Some(node::LEFT_LOWER_ARM));
        assert_eq!(tree.parents[node::ROOT], None);
        assert_eq!(
            tree.children[node::HEAD],
            [node::LEFT_EYE, node::RIGHT_EYE, node::HAIR0]
        );
    }

    #[test]
    fn rest_rotations_are_kept() {
        let tree = tree(|_| {}).unwrap();
        let turn = tree.rest[node::LEFT_UPPER_ARM].rotation;
        let expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        assert!(turn.abs_diff_eq(expected, 1e-6), "{turn}");
        // Below the turned arm, the hand's world rotation is the arm's.
        let worlds = tree.world(&tree.rest_pose());
        let (_, rotation, _) = worlds[node::LEFT_HAND].to_scale_rotation_translation();
        assert!(rotation.abs_diff_eq(expected, 1e-5), "{rotation}");
    }

    #[test]
    fn matrices_are_decomposed() {
        let tree = tree(|root| {
            let hips = &mut root["nodes"][node::HIPS];
            hips.as_object_mut().unwrap().remove("translation");
            hips["matrix"] = json!([
                2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.9, 0.0, 1.0
            ]);
        })
        .unwrap();
        let hips = tree.rest[node::HIPS];
        assert!(near(hips.translation, Vec3::new(0.0, 0.9, 0.0)));
        assert!(near(hips.scale, Vec3::splat(2.0)));
        assert!(hips.rotation.abs_diff_eq(Quat::IDENTITY, 1e-6));
    }

    #[test]
    fn a_zero_scale_matrix_keeps_a_finite_rotation() {
        let tree = tree(|root| {
            root["nodes"][node::HAIR0]["matrix"] = json!([
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 0.0, 1.0
            ]);
            let hair = root["nodes"][node::HAIR0].as_object_mut().unwrap();
            hair.remove("translation");
        })
        .unwrap();
        let hair = tree.rest[node::HAIR0];
        assert_eq!(hair.rotation, Quat::IDENTITY);
        assert_eq!(hair.scale, Vec3::ZERO);
        assert!(tree.world(&tree.rest_pose()).iter().all(|m| m.is_finite()));
    }

    #[test]
    fn nodes_outside_the_scene_are_kept() {
        let tree = tree(|root| {
            root["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Loose", "translation": [1.0, 2.0, 3.0] }));
        })
        .unwrap();
        assert_eq!(tree.pre_order, (0..26).collect::<Vec<_>>());
        assert_eq!(tree.parents[25], None);
        assert!(near(
            tree.world(&tree.rest_pose())[25].w_axis.truncate(),
            Vec3::new(1.0, 2.0, 3.0)
        ));
    }

    #[test]
    fn a_deep_chain_does_not_overflow_the_stack() {
        const DEPTH: usize = 200_000;
        let tree = tree(|root| {
            let nodes = root["nodes"].as_array_mut().unwrap();
            nodes[node::HAIR2]["children"] = json!([25]);
            for index in 25..25 + DEPTH {
                let mut chained = json!({ "translation": [0.0, 0.0, 0.0] });
                if index + 1 < 25 + DEPTH {
                    chained["children"] = json!([index + 1]);
                }
                nodes.push(chained);
            }
        })
        .unwrap();
        assert_eq!(tree.pre_order.len(), 25 + DEPTH);
        let worlds = tree.world(&tree.rest_pose());
        let hair = worlds[node::HAIR2].w_axis.truncate();
        assert!(near(worlds[24 + DEPTH].w_axis.truncate(), hair));
    }

    #[test]
    fn a_node_with_two_parents_is_refused() {
        let message = refusal(|root| {
            root["nodes"][node::SPINE]["children"]
                .as_array_mut()
                .unwrap()
                .push(json!(node::HEAD));
        });
        assert!(
            message.contains("node 5 is a child of both node 2 and node 4"),
            "{message}"
        );
    }

    #[test]
    fn a_cycle_is_refused() {
        let message = refusal(|root| root["nodes"][node::BODY]["children"] = json!([node::ROOT]));
        assert!(message.contains("cycle"), "{message}");
    }

    #[test]
    fn a_cycle_is_named_by_a_node_on_it() {
        // Body and FirstPersonOnly parent each other, and the hips (node 1,
        // the first node the walk misses) hang under them.
        let message = refusal(|root| {
            root["nodes"][node::ROOT]["children"] = json!([]);
            root["nodes"][node::BODY]["children"] = json!([node::FIRST_PERSON_ONLY]);
            root["nodes"][node::FIRST_PERSON_ONLY]["children"] = json!([node::BODY, node::HIPS]);
        });
        assert!(message.contains("node 24 is its own ancestor"), "{message}");
    }

    #[test]
    #[should_panic(expected = "a pose needs one local transform per node")]
    fn a_pose_of_another_length_is_a_bug() {
        let tree = tree(|_| {}).unwrap();
        let mut pose = tree.rest_pose();
        pose.local.pop();
        tree.world(&pose);
    }

    #[test]
    fn a_node_that_is_its_own_child_is_refused() {
        let message = refusal(|root| {
            root["nodes"][node::HAIR2]["children"] = json!([node::HAIR2]);
        });
        assert!(message.contains("node 10"), "{message}");
    }

    #[test]
    fn a_transform_past_f32_is_refused() {
        let message = refusal(|root| {
            root["nodes"][node::HAIR0]["translation"] = json!([1e39, 0.0, 0.0]);
        });
        assert!(message.contains("node 8"), "{message}");
    }
}
