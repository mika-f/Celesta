//! A glTF primitive as a protocol `Geometry`: only the vertices its
//! triangles use, the streams Celesta draws, computed normals where the file
//! has none, and MikkTSpace tangents where a normal texture needs them.
//!
//! Accessors are checked before `gltf`'s reader touches them: their types,
//! counts, strides, and byte ranges (which the reader trusts, panicking or
//! misreading on a hostile file), so a broken file is an error and nothing
//! is allocated past what the file's buffers hold.

use crate::document::{Document, LoadError};
use celesta_plugin::{Geometry, MorphTarget};
use glam::Vec3;
use gltf::accessor::{Accessor, DataType, Dimensions, Item, Iter};
use gltf::mesh::{Mode, Semantic};
use std::fmt;

/// The tangent a vertex gets when MikkTSpace has none for it.
const DEFAULT_TANGENT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// The most one primitive's morph targets may take, dense: Celesta's whole
/// scene buffer budget (`SCENE_BUFFER_BUDGET`), past which it refuses the
/// model anyway.
const TARGET_BUDGET: u64 = 1 << 30;

/// The most morph targets one primitive may have: far past any avatar's
/// (VRoid's faces have about 60), so a file of empty targets cannot make
/// one primitive hold millions.
const MAX_TARGETS: usize = 1 << 16;

/// One glTF primitive's geometry.
// Read by the model (Task 4).
#[allow(dead_code)]
pub(crate) struct PrimitiveGeometry {
    pub geometry: Geometry,
    pub mesh: usize,
    pub primitive: usize,
}

/// `primitive` of `mesh` as a triangle list, or `None` for points and
/// lines. `skin`, the skin of the node drawing the mesh, makes the
/// geometry skinned when the primitive has `JOINTS_0` and `WEIGHTS_0`;
/// `needs_tangents` (the material has a `normalTexture`) adds tangents.
// Called by the model (Task 4).
#[allow(dead_code)]
pub(crate) fn primitive_geometry(
    document: &Document,
    mesh: gltf::Mesh,
    primitive: gltf::Primitive,
    skin: Option<gltf::Skin>,
    needs_tangents: bool,
) -> Result<Option<PrimitiveGeometry>, LoadError> {
    read_primitive(
        document,
        mesh,
        primitive,
        skin,
        needs_tangents,
        TARGET_BUDGET,
    )
}

/// [`primitive_geometry`] with the morph targets' budget in bytes.
fn read_primitive(
    document: &Document,
    mesh: gltf::Mesh,
    primitive: gltf::Primitive,
    skin: Option<gltf::Skin>,
    needs_tangents: bool,
    target_budget: u64,
) -> Result<Option<PrimitiveGeometry>, LoadError> {
    let mode = primitive.mode();
    if !matches!(
        mode,
        Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan
    ) {
        return Ok(None);
    }
    let streams = Streams {
        document,
        at: At {
            mesh: &mesh,
            primitive: primitive.index(),
        },
    };
    let at = &streams.at;

    let Some(position) = primitive.get(&Semantic::Positions) else {
        return Err(at.invalid(format_args!("has no POSITION")));
    };
    if position.view().is_none() {
        // Its count is then bounded by nothing in the file.
        return Err(at.invalid(format_args!("has a POSITION with no bufferView")));
    }
    let vertices = position.count();
    let positions: Vec<[f32; 3]> = streams.floats3(&position, "POSITION", vertices)?;

    // Triangles, three indices each, over the file's vertices.
    let drawn: Vec<u32> = match primitive.indices() {
        Some(indices) => streams.indices(&indices)?,
        None => {
            let count = u32::try_from(vertices)
                .map_err(|_| at.invalid(format_args!("has {vertices} vertices")))?;
            (0..count).collect()
        }
    };
    if let Some(&past) = drawn.iter().find(|&&index| index as usize >= vertices) {
        return Err(at.invalid(format_args!(
            "uses vertex {past}, past its {vertices} vertices"
        )));
    }
    let triangles = triangle_list(mode, drawn, at)?;

    // The vertices the triangles use, in their order in the file.
    let mut remap = vec![u32::MAX; vertices];
    for &index in &triangles {
        remap[index as usize] = 0;
    }
    let mut kept: Vec<usize> = Vec::new();
    for (vertex, slot) in remap.iter_mut().enumerate() {
        if *slot != u32::MAX {
            *slot = kept.len() as u32;
            kept.push(vertex);
        }
    }
    let indices: Vec<u32> = triangles
        .iter()
        .map(|&index| remap[index as usize])
        .collect();
    let compact = |stream: Vec<[f32; 3]>| -> Vec<[f32; 3]> {
        kept.iter().map(|&vertex| stream[vertex]).collect()
    };

    let positions = compact(positions);
    let normals = match primitive.get(&Semantic::Normals) {
        Some(normals) => compact(streams.floats3(&normals, "NORMAL", vertices)?),
        None => computed_normals(&positions, &indices),
    };
    let uvs: Vec<[f32; 2]> = match primitive.get(&Semantic::TexCoords(0)) {
        Some(uvs) => {
            let uvs = streams.uvs(&uvs, vertices)?;
            kept.iter().map(|&vertex| uvs[vertex]).collect()
        }
        None => vec![[0.0; 2]; kept.len()],
    };

    let (joints, weights) = match skin {
        Some(skin) => streams.skinning(&primitive, &skin, vertices, &kept)?,
        None => (None, None),
    };

    // An empty target (`{}`), or a sparse one with no bufferView, costs the
    // file a few bytes and a dense copy here, so the targets are counted and
    // their total capped before any is read: every target's dense positions,
    // and its normals where it has them.
    let target_count = primitive.morph_targets().len();
    if target_count > MAX_TARGETS {
        return Err(at.invalid(format_args!(
            "has {target_count} morph targets, more than {MAX_TARGETS}"
        )));
    }
    let streams_per_vertex: u64 = primitive
        .morph_targets()
        .map(|target| 1 + u64::from(target.normals().is_some()))
        .sum();
    if let Some(bytes) = past_budget(streams_per_vertex, kept.len(), target_budget) {
        return Err(at.invalid(format_args!(
            "has morph targets of {} MiB, more than {} MiB",
            bytes >> 20,
            target_budget >> 20
        )));
    }
    let mut targets = Vec::new();
    for target in primitive.morph_targets() {
        let positions = match target.positions() {
            Some(deltas) => compact(streams.floats3(&deltas, "morph target POSITION", vertices)?),
            None => vec![[0.0; 3]; kept.len()],
        };
        let normals = match target.normals() {
            Some(deltas) => compact(streams.floats3(&deltas, "morph target NORMAL", vertices)?),
            None => Vec::new(),
        };
        targets.push(MorphTarget { positions, normals });
    }

    let tangents = needs_tangents.then(|| tangents(&positions, &normals, &uvs, &indices));
    Ok(Some(PrimitiveGeometry {
        geometry: Geometry {
            positions,
            normals,
            uvs,
            tangents,
            joints,
            weights,
            targets,
            indices,
        },
        mesh: mesh.index(),
        primitive: primitive.index(),
    }))
}

/// Where in the file a primitive is, for messages:
/// `mesh "Body" primitive 0`.
struct At<'a, 'd> {
    mesh: &'a gltf::Mesh<'d>,
    primitive: usize,
}

impl fmt::Display for At<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.mesh.name() {
            Some(name) => write!(f, "mesh {name:?} primitive {}", self.primitive),
            None => write!(f, "mesh {} primitive {}", self.mesh.index(), self.primitive),
        }
    }
}

impl At<'_, '_> {
    fn invalid(&self, reason: fmt::Arguments) -> LoadError {
        LoadError::Invalid(format!("{self} {reason}"))
    }
}

/// Reads one primitive's accessors, each checked first.
struct Streams<'a, 'd> {
    document: &'d Document,
    at: At<'a, 'd>,
}

impl<'d> Streams<'_, 'd> {
    fn buffer(&self, buffer: gltf::Buffer<'d>) -> Option<&'d [u8]> {
        self.document
            .buffers
            .get(buffer.index())
            .map(|data| data.0.as_slice())
    }

    /// `accessor`'s elements, after [`Self::check`].
    fn read<T: Item>(&self, accessor: &Accessor<'d>, what: &str) -> Result<Vec<T>, LoadError> {
        Iter::<T>::new(accessor.clone(), |buffer| self.buffer(buffer))
            .map(Iterator::collect)
            .ok_or_else(|| self.at.invalid(format_args!("has a {what} it cannot read")))
    }

    /// `VEC3` floats: positions, normals, and their morph deltas.
    fn floats3(
        &self,
        accessor: &Accessor<'d>,
        what: &str,
        count: usize,
    ) -> Result<Vec<[f32; 3]>, LoadError> {
        self.expect(accessor, what, Dimensions::Vec3, &[DataType::F32])?;
        self.check(accessor, what, Some(count))?;
        self.read(accessor, what)
    }

    /// `TEXCOORD_0` as floats, normalised integers widened.
    fn uvs(&self, accessor: &Accessor<'d>, count: usize) -> Result<Vec<[f32; 2]>, LoadError> {
        let what = "TEXCOORD_0";
        self.expect(
            accessor,
            what,
            Dimensions::Vec2,
            &[DataType::F32, DataType::U8, DataType::U16],
        )?;
        self.check(accessor, what, Some(count))?;
        Ok(match accessor.data_type() {
            DataType::U8 => widen(self.read::<[u8; 2]>(accessor, what)?, 255.0),
            DataType::U16 => widen(self.read::<[u16; 2]>(accessor, what)?, 65535.0),
            _ => self.read(accessor, what)?,
        })
    }

    /// The triangles' vertex indices as the file lists them.
    fn indices(&self, accessor: &Accessor<'d>) -> Result<Vec<u32>, LoadError> {
        let what = "indices";
        self.expect(
            accessor,
            what,
            Dimensions::Scalar,
            &[DataType::U8, DataType::U16, DataType::U32],
        )?;
        if accessor.view().is_none() {
            return Err(self
                .at
                .invalid(format_args!("has indices with no bufferView")));
        }
        self.check(accessor, what, None)?;
        Ok(match accessor.data_type() {
            DataType::U8 => self
                .read::<u8>(accessor, what)?
                .into_iter()
                .map(u32::from)
                .collect(),
            DataType::U16 => self
                .read::<u16>(accessor, what)?
                .into_iter()
                .map(u32::from)
                .collect(),
            _ => self.read(accessor, what)?,
        })
    }

    /// `JOINTS_0` and `WEIGHTS_0` of the kept vertices, for `skin`. Neither
    /// gives an unskinned geometry; one without the other is refused.
    #[allow(clippy::type_complexity)]
    fn skinning(
        &self,
        primitive: &gltf::Primitive<'d>,
        skin: &gltf::Skin<'d>,
        count: usize,
        kept: &[usize],
    ) -> Result<(Option<Vec<[u16; 4]>>, Option<Vec<[f32; 4]>>), LoadError> {
        let (joints, weights) = match (
            primitive.get(&Semantic::Joints(0)),
            primitive.get(&Semantic::Weights(0)),
        ) {
            (None, None) => return Ok((None, None)),
            (Some(joints), Some(weights)) => (joints, weights),
            (Some(_), None) => {
                return Err(self
                    .at
                    .invalid(format_args!("has JOINTS_0 but no WEIGHTS_0")));
            }
            (None, Some(_)) => {
                return Err(self
                    .at
                    .invalid(format_args!("has WEIGHTS_0 but no JOINTS_0")));
            }
        };

        self.expect(
            &joints,
            "JOINTS_0",
            Dimensions::Vec4,
            &[DataType::U8, DataType::U16],
        )?;
        self.check(&joints, "JOINTS_0", Some(count))?;
        let joints: Vec<[u16; 4]> = match joints.data_type() {
            DataType::U8 => self
                .read::<[u8; 4]>(&joints, "JOINTS_0")?
                .into_iter()
                .map(|joint| joint.map(u16::from))
                .collect(),
            _ => self.read(&joints, "JOINTS_0")?,
        };
        self.expect(
            &weights,
            "WEIGHTS_0",
            Dimensions::Vec4,
            &[DataType::F32, DataType::U8, DataType::U16],
        )?;
        self.check(&weights, "WEIGHTS_0", Some(count))?;
        let weights: Vec<[f32; 4]> = match weights.data_type() {
            DataType::U8 => widen(self.read::<[u8; 4]>(&weights, "WEIGHTS_0")?, 255.0),
            DataType::U16 => widen(self.read::<[u16; 4]>(&weights, "WEIGHTS_0")?, 65535.0),
            _ => self.read(&weights, "WEIGHTS_0")?,
        };

        let joint_count = skin.joints().len();
        let mut kept_joints = Vec::with_capacity(kept.len());
        let mut kept_weights = Vec::with_capacity(kept.len());
        for &vertex in kept {
            let weight = weights[vertex];
            if let Some(bad) = weight.iter().find(|weight| !weight.is_finite()) {
                return Err(self.at.invalid(format_args!("has a joint weight of {bad}")));
            }
            let (joint, weight) =
                skin_slots(joints[vertex], weight, joint_count).map_err(|joint| {
                    self.at.invalid(format_args!(
                        "names joint {joint} of skin {}, which has {joint_count}",
                        skin.index()
                    ))
                })?;
            kept_joints.push(joint);
            kept_weights.push(weight);
        }
        Ok((Some(kept_joints), Some(kept_weights)))
    }

    /// `accessor` has the type `what` needs.
    fn expect(
        &self,
        accessor: &Accessor<'d>,
        what: &str,
        dimensions: Dimensions,
        types: &[DataType],
    ) -> Result<(), LoadError> {
        if accessor.dimensions() == dimensions && types.contains(&accessor.data_type()) {
            return Ok(());
        }
        Err(self.at.invalid(format_args!(
            "has a {what} of {:?} {:?}, not {dimensions:?} of {types:?}",
            accessor.dimensions(),
            accessor.data_type()
        )))
    }

    /// Everything `gltf`'s reader assumes of `accessor`: at least one
    /// element (`count` of them for a vertex stream), each view's stride at
    /// least an element, and every element inside its view and buffer. A
    /// sparse accessor's indices must also rise and stay below its count.
    fn check(
        &self,
        accessor: &Accessor<'d>,
        what: &str,
        count: Option<usize>,
    ) -> Result<(), LoadError> {
        let elements = accessor.count();
        if elements == 0 {
            return Err(self
                .at
                .invalid(format_args!("has a {what} with no elements")));
        }
        if let Some(count) = count
            && elements != count
        {
            return Err(self
                .at
                .invalid(format_args!("has {elements} {what} for {count} vertices")));
        }
        let size = accessor.size();
        match accessor.view() {
            Some(view) => self.check_view(&view, accessor.offset(), size, elements, what)?,
            None if accessor.sparse().is_none() => {
                return Err(self.at.invalid(format_args!(
                    "has a {what} with neither a bufferView nor sparse values"
                )));
            }
            None => {}
        }
        let Some(sparse) = accessor.sparse() else {
            return Ok(());
        };
        let values = sparse.count();
        if values == 0 || values > elements {
            return Err(self.at.invalid(format_args!(
                "has a {what} with {values} sparse values for {elements} elements"
            )));
        }
        let indices = sparse.indices();
        let index_size = indices.index_type().size();
        let view = indices.view();
        self.check_view(&view, indices.offset(), index_size, values, what)?;
        self.check_view(
            &sparse.values().view(),
            sparse.values().offset(),
            size,
            values,
            what,
        )?;

        // The reader matches indices to elements in order and skips the
        // rest: indices out of order would silently drop values.
        let stride = view.stride().unwrap_or(index_size);
        let start = view.offset() + indices.offset();
        let bytes = self.buffer(view.buffer()).unwrap_or_default();
        let mut previous = None;
        for value in 0..values {
            let at = start + value * stride;
            let index = bytes.get(at..at + index_size).map(|bytes| match bytes {
                [a] => usize::from(*a),
                [a, b] => usize::from(u16::from_le_bytes([*a, *b])),
                [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]) as usize,
                _ => usize::MAX,
            });
            match index {
                Some(index) if index < elements && previous.is_none_or(|p| index > p) => {
                    previous = Some(index);
                }
                _ => {
                    return Err(self.at.invalid(format_args!(
                        "has a {what} whose sparse indices are not rising below {elements}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// `count` elements of `size` bytes from `offset` into `view` lie inside
    /// the view, and the view inside its buffer.
    fn check_view(
        &self,
        view: &gltf::buffer::View<'d>,
        offset: usize,
        size: usize,
        count: usize,
        what: &str,
    ) -> Result<(), LoadError> {
        let stride = view.stride().unwrap_or(size);
        if stride < size {
            return Err(self.at.invalid(format_args!(
                "has a {what} in buffer view {} whose stride, {stride}, is less than its {size}-byte elements",
                view.index()
            )));
        }
        let buffer = self.buffer(view.buffer()).map_or(0, <[u8]>::len);
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > buffer)
        {
            return Err(self.at.invalid(format_args!(
                "has a {what} in buffer view {}, which runs past its buffer's {buffer} bytes",
                view.index()
            )));
        }
        let end = stride
            .checked_mul(count - 1)
            .and_then(|bytes| bytes.checked_add(size))
            .and_then(|bytes| bytes.checked_add(offset));
        if end.is_none_or(|end| end > view.length()) {
            return Err(self.at.invalid(format_args!(
                "has a {what} of {count} elements, which runs past buffer view {}'s {} bytes",
                view.index(),
                view.length()
            )));
        }
        Ok(())
    }
}

/// The bytes of `streams` dense `VEC3` float streams over `vertices`
/// vertices, when they are more than `budget`.
fn past_budget(streams: u64, vertices: usize, budget: u64) -> Option<u64> {
    let bytes = streams.saturating_mul((vertices as u64).saturating_mul(12));
    (bytes > budget).then_some(bytes)
}

/// Normalised integers as floats in 0..1.
fn widen<T: Copy + Into<f32>, const N: usize>(items: Vec<[T; N]>, max: f32) -> Vec<[f32; N]> {
    items
        .into_iter()
        .map(|item| item.map(|value| value.into() / max))
        .collect()
}

/// `TRIANGLE_STRIP` and `TRIANGLE_FAN` as a list, in glTF's winding
/// (glTF 2.0, 3.7.2.1): strip triangle `i` is `v[i], v[i + 1 + i % 2],
/// v[i + 2 − i % 2]`, fan triangle `i` is `v[i + 1], v[i + 2], v[0]`.
fn triangle_list(mode: Mode, drawn: Vec<u32>, at: &At) -> Result<Vec<u32>, LoadError> {
    let triangles = drawn.len().saturating_sub(2);
    match mode {
        Mode::TriangleStrip => Ok((0..triangles)
            .flat_map(|i| {
                let odd = i % 2;
                [drawn[i], drawn[i + 1 + odd], drawn[i + 2 - odd]]
            })
            .collect()),
        Mode::TriangleFan => Ok((0..triangles)
            .flat_map(|i| [drawn[i + 1], drawn[i + 2], drawn[0]])
            .collect()),
        _ if !drawn.len().is_multiple_of(3) => Err(at.invalid(format_args!(
            "has {} indices, which is not a whole number of triangles",
            drawn.len()
        ))),
        _ => Ok(drawn),
    }
}

/// One vertex's four joint slots for a skin of `joint_count` joints: a slot
/// of weight 0 (or less) names joint 0 with weight 0, and the weights are
/// scaled to sum to 1 where they sum to more than 0. `Err` is the joint a
/// weighted slot names that the skin lacks.
fn skin_slots(
    joints: [u16; 4],
    weights: [f32; 4],
    joint_count: usize,
) -> Result<([u16; 4], [f32; 4]), u16> {
    let mut joints = joints;
    let mut weights = weights;
    for (joint, weight) in joints.iter_mut().zip(&mut weights) {
        if *weight > 0.0 {
            if usize::from(*joint) >= joint_count {
                return Err(*joint);
            }
        } else {
            *joint = 0;
            *weight = 0.0;
        }
    }
    let sum: f32 = weights.iter().sum();
    if sum > 0.0 {
        weights = weights.map(|weight| weight / sum);
    }
    Ok((joints, weights))
}

/// Area-weighted vertex normals over `indices`' triangles, as Unity's
/// `RecalculateNormals` makes them for a mesh whose vertices are not
/// welded: a vertex in no triangle with area gets (0, 0, 0).
fn computed_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut sums = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|index| index as usize);
        let [pa, pb, pc] = [a, b, c].map(|vertex| Vec3::from(positions[vertex]));
        // Twice the triangle's area along its normal.
        let face = (pb - pa).cross(pc - pa);
        for vertex in [a, b, c] {
            sums[vertex] += face;
        }
    }
    sums.into_iter()
        .map(|sum| sum.normalize_or_zero().to_array())
        .collect()
}

/// MikkTSpace tangents for the triangles of `indices`, as Unity imports
/// them for UniVRM: a vertex takes the last tangent written for it. A
/// primitive whose UVs span no area, or with a stream that is not finite,
/// and any vertex left without a finite unit tangent, gets
/// [`DEFAULT_TANGENT`].
fn tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let fallback = vec![DEFAULT_TANGENT; positions.len()];
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    if !(finite(positions.as_flattened())
        && finite(normals.as_flattened())
        && finite(uvs.as_flattened()))
    {
        return fallback;
    }
    let flat = indices.as_chunks::<3>().0.iter().all(|triangle| {
        let [a, b, c] = triangle.map(|index| uvs[index as usize]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
        u[0] * v[1] - u[1] * v[0] == 0.0
    });
    if flat {
        return fallback;
    }

    let mut mikk = Mikk {
        positions,
        normals,
        uvs,
        indices,
        tangents: vec![[f32::NAN; 4]; positions.len()],
    };
    if !mikktspace::generate_tangents(&mut mikk) {
        return fallback;
    }
    mikk.tangents
        .into_iter()
        .map(|[x, y, z, w]| {
            let tangent = Vec3::new(x, y, z);
            match tangent.try_normalize() {
                Some(unit) if w.is_finite() => {
                    let [x, y, z] = unit.to_array();
                    [x, y, z, if w < 0.0 { -1.0 } else { 1.0 }]
                }
                _ => DEFAULT_TANGENT,
            }
        })
        .collect()
}

/// A primitive's triangles as MikkTSpace reads them. glTF's `v` counts
/// down from the top of the texture, MikkTSpace's up from the bottom, so
/// `v` is flipped: the bitangent then points up the texture, as a normal
/// texture's green does.
struct Mikk<'a> {
    positions: &'a [[f32; 3]],
    normals: &'a [[f32; 3]],
    uvs: &'a [[f32; 2]],
    indices: &'a [u32],
    tangents: Vec<[f32; 4]>,
}

impl Mikk<'_> {
    fn vertex(&self, face: usize, vert: usize) -> usize {
        self.indices[face * 3 + vert] as usize
    }
}

impl mikktspace::Geometry for Mikk<'_> {
    fn num_faces(&self) -> usize {
        self.indices.len() / 3
    }

    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        self.positions[self.vertex(face, vert)]
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        self.normals[self.vertex(face, vert)]
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        let [u, v] = self.uvs[self.vertex(face, vert)];
        [u, 1.0 - v]
    }

    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, vert: usize) {
        let vertex = self.vertex(face, vert);
        self.tangents[vertex] = tangent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::SyntheticVrm;
    use serde_json::{Value, json};

    /// Mesh `mesh`'s primitives as the avatar's model would read them: skinned
    /// by the 3-joint skin when the mesh is Body, with tangents where the
    /// material has a normal texture (or everywhere, with `tangents`).
    fn read(
        vrm: &SyntheticVrm,
        mesh: usize,
        tangents: bool,
        edit: impl FnOnce(&mut Value),
    ) -> Result<Vec<Option<Geometry>>, LoadError> {
        let document = Document::parse(&vrm.glb_with(edit))?;
        let mesh = document.gltf.meshes().nth(mesh).unwrap();
        let skin = (mesh.index() == 0).then(|| document.gltf.skins().next().unwrap());
        mesh.primitives()
            .map(|primitive| {
                let needs = tangents || primitive.material().normal_texture().is_some();
                let read =
                    primitive_geometry(&document, mesh.clone(), primitive, skin.clone(), needs)?;
                Ok(read.map(|read| read.geometry))
            })
            .collect()
    }

    fn body(vrm: &SyntheticVrm) -> Vec<Geometry> {
        read(vrm, 0, false, |_| {})
            .unwrap()
            .into_iter()
            .map(Option::unwrap)
            .collect()
    }

    fn body_with(edit: impl FnOnce(&mut Value)) -> Result<Vec<Option<Geometry>>, LoadError> {
        read(&SyntheticVrm::avatar(), 0, false, edit)
    }

    fn refusal(edit: impl FnOnce(&mut Value)) -> String {
        match body_with(edit) {
            Ok(_) => panic!("the primitives were read"),
            Err(error) => error.to_string(),
        }
    }

    /// The accessor Body primitive 0 has for `semantic`.
    fn accessor(root: &Value, semantic: &str) -> usize {
        let primitive = &root["meshes"][0]["primitives"][0];
        let index = match semantic {
            "indices" => &primitive["indices"],
            "target0" => &primitive["targets"][0]["POSITION"],
            _ => &primitive["attributes"][semantic],
        };
        index.as_u64().unwrap() as usize
    }

    fn view(root: &Value, semantic: &str) -> usize {
        root["accessors"][accessor(root, semantic)]["bufferView"]
            .as_u64()
            .unwrap() as usize
    }

    #[test]
    fn shared_accessors_are_compacted() {
        let [cube, hair] = &body(&SyntheticVrm::avatar())[..] else {
            panic!("Body has two primitives");
        };
        assert_eq!(cube.positions.len(), 8);
        assert_eq!(hair.positions.len(), 4);
        assert!(hair.indices.iter().all(|&index| index < 4));
        assert_eq!(hair.indices, [0, 2, 3, 0, 3, 1]);
        assert_eq!(hair.positions[0], [-0.15, 1.3, -0.1]);
        for geometry in [cube, hair] {
            let vertices = geometry.positions.len();
            assert_eq!(geometry.targets.len(), 3);
            for target in &geometry.targets {
                assert_eq!(target.positions.len(), vertices);
                assert!(target.normals.is_empty());
            }
            assert_eq!(geometry.normals.len(), vertices);
            assert_eq!(geometry.uvs.len(), vertices);
            assert_eq!(geometry.joints.as_ref().map(Vec::len), Some(vertices));
            assert_eq!(geometry.weights.as_ref().map(Vec::len), Some(vertices));
        }
        // The box's top corners move with target 0; the hair does not.
        assert_eq!(cube.targets[0].positions[2], [0.0, 0.1, 0.0]);
        assert_eq!(cube.targets[0].positions[0], [0.0, 0.0, 0.0]);
        assert!(hair.targets[0].positions.iter().all(|&d| d == [0.0; 3]));
    }

    #[test]
    fn zero_weight_joint_slots_are_zeroed() {
        let cube = &body(&SyntheticVrm::avatar())[0];
        let joints = cube.joints.as_ref().unwrap();
        let weights = cube.weights.as_ref().unwrap();
        for (joint, weight) in joints.iter().zip(weights) {
            assert_eq!(joint[1], 0);
            assert_eq!(weight[1], 0.0);
            assert_eq!(*weight, [1.0, 0.0, 0.0, 0.0]);
        }
        let message = match read(
            &SyntheticVrm {
                slot1_weight: 0.5,
                ..SyntheticVrm::avatar()
            },
            0,
            false,
            |_| {},
        ) {
            Ok(_) => panic!("joint 99 was read"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("names joint 99 of skin 0, which has 3"),
            "{message}"
        );
        assert!(message.contains("mesh \"Body\" primitive 0"), "{message}");
    }

    #[test]
    fn weights_are_normalised() {
        let skin = |joints, weights| skin_slots(joints, weights, 3);
        assert_eq!(
            skin([0, 1, 2, 0], [2.0, 1.0, 1.0, 0.0]),
            Ok(([0, 1, 2, 0], [0.5, 0.25, 0.25, 0.0]))
        );
        assert_eq!(
            skin([2, 7, 1, 9], [1.0, -0.5, 0.0, 0.0]),
            Ok(([2, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]))
        );
        assert_eq!(skin([5, 5, 5, 5], [0.0; 4]), Ok(([0; 4], [0.0; 4])));
        assert_eq!(skin([0, 3, 0, 0], [0.5, 0.5, 0.0, 0.0]), Err(3));
    }

    #[test]
    fn morph_targets_have_a_budget() {
        // Six target streams of 8 vertices: 576 bytes.
        assert_eq!(past_budget(6, 8, 576), None);
        assert_eq!(past_budget(6, 8, 575), Some(576));
        assert_eq!(
            past_budget(u64::MAX, usize::MAX, TARGET_BUDGET),
            Some(u64::MAX)
        );
        assert_eq!(past_budget(0, usize::MAX, TARGET_BUDGET), None);
    }

    /// Body primitive 0 with `extra` empty targets (`{}`) after its three,
    /// read with a morph target budget of `budget` bytes.
    fn empty_targets(extra: usize, budget: u64) -> Result<Option<Geometry>, LoadError> {
        let glb = SyntheticVrm::avatar().glb_with(|root| {
            let targets = root["meshes"][0]["primitives"][0]["targets"]
                .as_array_mut()
                .unwrap();
            targets.extend(std::iter::repeat_n(json!({}), extra));
        });
        let document = Document::parse(&glb)?;
        let mesh = document.gltf.meshes().next().unwrap();
        let primitive = mesh.primitives().next().unwrap();
        let skin = document.gltf.skins().next();
        let read = read_primitive(&document, mesh.clone(), primitive, skin, false, budget)?;
        Ok(read.map(|read| read.geometry))
    }

    #[test]
    fn empty_targets_count_toward_the_budget() {
        // 13 targets, each with dense positions over 8 vertices: 1248 bytes.
        let cube = empty_targets(10, 1248).unwrap().unwrap();
        assert_eq!(cube.targets.len(), 13);
        for target in &cube.targets[3..] {
            assert_eq!(target.positions, vec![[0.0; 3]; 8]);
            assert!(target.normals.is_empty());
        }
        let message = match empty_targets(10, 1247) {
            Ok(_) => panic!("the targets were read"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("morph targets of"), "{message}");
    }

    #[test]
    fn too_many_targets_are_refused() {
        let message = match empty_targets(MAX_TARGETS - 2, TARGET_BUDGET) {
            Ok(_) => panic!("the targets were read"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("has 65537 morph targets, more than 65536"),
            "{message}"
        );
    }

    #[test]
    fn tangents_only_where_a_normal_texture_is_used() {
        let [cube, hair] = &body(&SyntheticVrm::avatar())[..] else {
            panic!("Body has two primitives");
        };
        let tangents = cube.tangents.as_ref().expect("the box has tangents");
        assert_eq!(tangents.len(), 8);
        for &[x, y, z, w] in tangents {
            assert!([x, y, z, w].iter().all(|v| v.is_finite()));
            let length = (x * x + y * y + z * z).sqrt();
            assert!((length - 1.0).abs() < 1e-4, "{length}");
            assert!(w == 1.0 || w == -1.0, "{w}");
        }
        assert_eq!(hair.tangents, None);
    }

    #[test]
    fn tangents_follow_u_with_the_bitangent_up() {
        // The quad faces +Z, `u` grows to +X and `v` downwards, so the
        // tangent is +X and the bitangent, `w × (N × T)`, points up: w = 1.
        let quad = read(&SyntheticVrm::avatar(), 1, true, |_| {}).unwrap();
        let tangents = quad[0].as_ref().unwrap().tangents.clone().unwrap();
        for tangent in tangents {
            let error = tangent
                .iter()
                .zip([1.0, 0.0, 0.0, 1.0])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(error < 1e-5, "{tangent:?}");
        }
    }

    #[test]
    fn flat_uvs_give_finite_tangents() {
        let cube = &body(&SyntheticVrm {
            flat_uvs: true,
            ..SyntheticVrm::avatar()
        })[0];
        let tangents = cube.tangents.as_ref().unwrap();
        assert_eq!(tangents.len(), 8);
        assert!(
            tangents
                .iter()
                .all(|&tangent| tangent == [1.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn missing_normals_are_computed() {
        let [cube, hair] = &body(&SyntheticVrm {
            normals: false,
            ..SyntheticVrm::avatar()
        })[..] else {
            panic!("Body has two primitives");
        };
        for normal in &hair.normals {
            let error = normal
                .iter()
                .zip([0.0, 0.0, -1.0])
                .map(|(a, b): (&f32, f32)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(error < 1e-6, "{normal:?}");
        }
        // A box corner's normal leans out of all three faces it touches.
        let [x, y, z] = cube.normals[0];
        assert!(x < 0.0 && y < 0.0 && z < 0.0, "{:?}", cube.normals[0]);
        assert!(((x * x + y * y + z * z).sqrt() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn missing_uvs_are_zero() {
        let read = body_with(|root| {
            for primitive in root["meshes"][0]["primitives"].as_array_mut().unwrap() {
                primitive["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove("TEXCOORD_0");
            }
        })
        .unwrap();
        let cube = read[0].as_ref().unwrap();
        assert!(cube.uvs.iter().all(|&uv| uv == [0.0, 0.0]));
        // Flat, so the tangents are the default.
        assert!(
            cube.tangents
                .as_ref()
                .unwrap()
                .iter()
                .all(|&tangent| tangent == [1.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn unskinned_meshes_have_no_joints() {
        let quad = read(&SyntheticVrm::avatar(), 1, false, |_| {}).unwrap();
        let quad = quad[0].as_ref().unwrap();
        assert_eq!(quad.joints, None);
        assert_eq!(quad.weights, None);
        assert_eq!(quad.positions.len(), 4);
        assert!(quad.targets.is_empty());
    }

    fn hair_mode(mode: u32) -> Option<Geometry> {
        body_with(|root| root["meshes"][0]["primitives"][1]["mode"] = json!(mode))
            .unwrap()
            .remove(1)
    }

    #[test]
    fn strips_and_fans_become_lists() {
        // Hair indices 8 10 11 8 11 9, compacted to 0 2 3 0 3 1.
        assert_eq!(
            hair_mode(5).unwrap().indices,
            [0, 2, 3, 2, 0, 3, 3, 0, 3, 0, 1, 3]
        );
        assert_eq!(
            hair_mode(6).unwrap().indices,
            [2, 3, 0, 3, 0, 0, 0, 3, 0, 3, 1, 0]
        );
    }

    #[test]
    fn points_and_lines_are_not_read() {
        for mode in [0, 1, 2, 3] {
            assert_eq!(hair_mode(mode), None, "mode {mode}");
        }
    }

    #[test]
    fn a_primitive_without_indices_draws_every_vertex() {
        let read = body_with(|root| {
            let hair = root["meshes"][0]["primitives"][1].as_object_mut().unwrap();
            hair.remove("indices");
        })
        .unwrap();
        let hair = read[1].as_ref().unwrap();
        assert_eq!(hair.positions.len(), 12);
        assert_eq!(hair.indices, (0..12).collect::<Vec<u32>>());
    }

    #[test]
    fn sparse_morph_targets_are_read() {
        // Target 0 as zeros with one sparse value: vertex 2 (the box's
        // second index) gets the value stored for vertex 2.
        let read = body_with(|root| {
            let target = accessor(root, "target0");
            let indices = view(root, "indices");
            let values = view(root, "target0");
            let accessor = root["accessors"][target].as_object_mut().unwrap();
            accessor.remove("bufferView");
            accessor.insert(
                "sparse".into(),
                json!({
                    "count": 1,
                    "indices": { "bufferView": indices, "byteOffset": 2, "componentType": 5123 },
                    "values": { "bufferView": values, "byteOffset": 24 },
                }),
            );
        })
        .unwrap();
        let cube = read[0].as_ref().unwrap();
        let mut expected = vec![[0.0; 3]; 8];
        expected[2] = [0.0, 0.1, 0.0];
        assert_eq!(cube.targets[0].positions, expected);
    }

    #[test]
    fn indices_past_the_vertices_are_refused() {
        let message = refusal(|root| {
            for semantic in [
                "POSITION",
                "NORMAL",
                "TEXCOORD_0",
                "JOINTS_0",
                "WEIGHTS_0",
                "target0",
            ] {
                let accessor = accessor(root, semantic);
                root["accessors"][accessor]["count"] = json!(4);
            }
            for target in 1..3 {
                let accessor = root["meshes"][0]["primitives"][0]["targets"][target]["POSITION"]
                    .as_u64()
                    .unwrap() as usize;
                root["accessors"][accessor]["count"] = json!(4);
            }
        });
        assert!(message.contains("vertex 4"), "{message}");
        assert!(message.contains("4 vertices"), "{message}");
    }

    #[test]
    fn streams_of_another_length_are_refused() {
        let message = refusal(|root| {
            let normals = accessor(root, "NORMAL");
            root["accessors"][normals]["count"] = json!(11);
        });
        assert!(message.contains("NORMAL"), "{message}");
        assert!(message.contains("11"), "{message}");
    }

    #[test]
    fn streams_of_the_wrong_type_are_refused() {
        for (semantic, field, value) in [
            ("POSITION", "type", json!("VEC2")),
            ("JOINTS_0", "componentType", json!(5126)),
            ("WEIGHTS_0", "componentType", json!(5125)),
            ("TEXCOORD_0", "type", json!("SCALAR")),
            ("indices", "type", json!("VEC3")),
            ("indices", "componentType", json!(5126)),
            ("target0", "componentType", json!(5123)),
        ] {
            let message = refusal(|root| {
                let accessor = accessor(root, semantic);
                root["accessors"][accessor][field] = value;
            });
            assert!(!message.is_empty(), "{semantic}");
        }
    }

    #[test]
    fn strides_shorter_than_an_element_are_refused() {
        let message = refusal(|root| {
            let positions = view(root, "POSITION");
            root["bufferViews"][positions]["byteStride"] = json!(4);
        });
        assert!(message.contains("stride"), "{message}");
    }

    #[test]
    fn accessors_past_their_bytes_are_refused() {
        for (semantic, field, value) in [
            ("POSITION", "count", json!(0)),
            ("POSITION", "count", json!(1u64 << 40)),
            ("POSITION", "byteOffset", json!(1_000_000)),
            ("indices", "count", json!(1u64 << 40)),
            ("indices", "count", json!(0)),
            ("target0", "count", json!(1u64 << 40)),
        ] {
            let message = refusal(|root| {
                let accessor = accessor(root, semantic);
                root["accessors"][accessor][field] = value;
            });
            assert!(!message.is_empty(), "{semantic} {field}");
        }
        let message = refusal(|root| {
            let positions = view(root, "POSITION");
            root["bufferViews"][positions]["byteLength"] = json!(1u64 << 40);
        });
        assert!(message.contains("buffer"), "{message}");
        let message = refusal(|root| {
            let positions = view(root, "POSITION");
            root["bufferViews"][positions]["byteOffset"] = json!(u64::MAX - 8);
        });
        assert!(!message.is_empty());
    }

    #[test]
    fn broken_sparse_accessors_are_refused() {
        for sparse in [
            json!({ "count": 0, "indices": { "bufferView": 0, "componentType": 5123 }, "values": { "bufferView": 0 } }),
            json!({ "count": 13, "indices": { "bufferView": 0, "componentType": 5123 }, "values": { "bufferView": 0 } }),
            json!({ "count": 1, "indices": { "bufferView": 0, "byteOffset": 1_000_000, "componentType": 5123 }, "values": { "bufferView": 0 } }),
            json!({ "count": 1, "indices": { "bufferView": 0, "componentType": 5125 }, "values": { "bufferView": 0, "byteOffset": 1_000_000 } }),
            json!({ "count": 12, "indices": { "bufferView": 0, "componentType": 5125 }, "values": { "bufferView": 0 } }),
        ] {
            let message = refusal(|root| {
                let target = accessor(root, "target0");
                let accessor = root["accessors"][target].as_object_mut().unwrap();
                accessor.remove("bufferView");
                accessor.insert("sparse".into(), sparse);
            });
            assert!(!message.is_empty());
        }
    }

    #[test]
    fn a_skinned_primitive_needs_both_joints_and_weights() {
        let message = refusal(|root| {
            for primitive in root["meshes"][0]["primitives"].as_array_mut().unwrap() {
                primitive["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove("WEIGHTS_0");
            }
        });
        assert!(message.contains("WEIGHTS_0"), "{message}");
    }

    #[test]
    fn a_triangle_list_of_loose_indices_is_refused() {
        let message = refusal(|root| {
            let indices = accessor(root, "indices");
            root["accessors"][indices]["count"] = json!(35);
        });
        assert!(message.contains("35"), "{message}");
    }
}
