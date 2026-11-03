//! VRM 1.0's `firstPerson` mesh annotations, as a third-person camera sees
//! them: Celesta's cameras are never the avatar's own eyes, so a mesh
//! annotated `firstPersonOnly` (a head-less body for the wearer's view, for
//! one) is hidden, and `thirdPersonOnly`, `both`, `auto`, and unannotated
//! meshes are drawn. That is what UniVRM's third-person camera layer shows.

use crate::document::{Document, LoadError};
use vrm_spec::vrmc_vrm_1_0::FirstPersonType;

/// By node: whether its mesh is hidden from a third-person camera. A node
/// annotated more than once takes its last annotation: this plugin's
/// choice, not checked against what UniVRM does with such a file. An
/// annotation without a node is ignored; one naming a node the file lacks
/// is refused.
pub(crate) fn hidden_nodes(document: &Document) -> Result<Vec<bool>, LoadError> {
    let nodes = document.gltf.nodes().len();
    let mut hidden = vec![false; nodes];
    let annotations = document
        .vrm
        .first_person
        .as_ref()
        .and_then(|first_person| first_person.mesh_annotations.as_deref())
        .unwrap_or_default();
    for (index, annotation) in annotations.iter().enumerate() {
        let Some(node) = annotation.node.map(|node| node.value()) else {
            continue;
        };
        let Some(slot) = hidden.get_mut(node) else {
            return Err(LoadError::Invalid(format!(
                "firstPerson annotation {index} names node {node}, but the file has {nodes} nodes"
            )));
        };
        *slot = matches!(
            annotation.mesh_annotation_type,
            FirstPersonType::FirstPersonOnly
        );
    }
    Ok(hidden)
}
