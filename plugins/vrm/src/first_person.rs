//! VRM 1.0's `firstPerson` mesh annotations, as a third-person camera sees
//! them: Celesta's cameras are never the avatar's own eyes, so a mesh
//! annotated `firstPersonOnly` (a head-less body for the wearer's view, for
//! one) is hidden, and `thirdPersonOnly`, `both`, `auto`, and unannotated
//! meshes are drawn. That is what UniVRM's third-person camera layer shows.

use crate::document::{Document, LoadError};
use crate::notices::Warnings;
use celesta_plugin::Notice;
use vrm_spec::vrmc_vrm_1_0::FirstPersonType;

/// By node: whether its mesh is hidden from a third-person camera. A node
/// annotated more than once takes its first annotation, as UniVRM's
/// `VRM10ObjectFirstPerson` does, and each later one adds a warning to
/// `notices` (see `notices`). An annotation without a node is ignored; one
/// naming a node the file lacks is refused.
pub(crate) fn hidden_nodes(
    document: &Document,
    notices: &mut Vec<Notice>,
) -> Result<Vec<bool>, LoadError> {
    let nodes = document.gltf.nodes().len();
    let mut hidden = vec![false; nodes];
    let mut annotated = vec![false; nodes];
    let mut repeated = Warnings::new("repeated firstPerson annotations");
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
        let Some(seen) = annotated.get_mut(node) else {
            return Err(LoadError::Invalid(format!(
                "firstPerson annotation {index} names node {node}, but the file has {nodes} nodes"
            )));
        };
        if std::mem::replace(seen, true) {
            repeated.push(|| {
                format!(
                    "firstPerson annotation {index} names node {node} again; only its first annotation counts"
                )
            });
            continue;
        }
        hidden[node] = matches!(
            annotation.mesh_annotation_type,
            FirstPersonType::FirstPersonOnly
        );
    }
    repeated.finish(notices);
    Ok(hidden)
}
