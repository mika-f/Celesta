//! A glTF primitive as a protocol `Geometry`: only the vertices its
//! triangles use, the streams Celesta draws, computed normals where the file
//! has none, and MikkTSpace tangents where a normal texture needs them.
//!
//! Accessors are checked before `gltf`'s reader touches them: their types,
//! counts, strides, and byte ranges (which the reader trusts, panicking or
//! misreading on a hostile file), so a broken file is an error and nothing
//! is allocated past what the file's buffers hold.
//!
//! A model's primitives share two budgets: what their accessors decode
//! ([`READ_BUDGET`], each accessor decoded once for every primitive that
//! reads it) and the geometry they make ([`GEOMETRY_BUDGET`], which the
//! model hands each primitive what is left of). So a file of many primitives
//! over one large accessor, a few JSON bytes each, costs neither memory nor
//! time in proportion to their number.

use crate::document::{Document, LoadError};
use celesta_plugin::{Geometry, MorphTarget};
use glam::Vec3;
use gltf::accessor::{Accessor, DataType, Dimensions, Item, Iter};
use gltf::mesh::{Mode, Semantic};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::mem::size_of;
use std::rc::Rc;

/// The tangent a vertex gets when MikkTSpace has none for it.
const DEFAULT_TANGENT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// The most a model's geometry may take, every primitive's streams, morph
/// targets, and indices together: Celesta's whole scene buffer budget
/// (`SCENE_BUFFER_BUDGET`), past which it refuses the model anyway.
pub(crate) const GEOMETRY_BUDGET: u64 = 1 << 30;

/// The most a model's accessors may take once decoded, each counted once.
/// Distinct accessors can all cover the same bytes of a buffer, so the
/// file's size alone does not bound what they decode.
pub(crate) const READ_BUDGET: u64 = 1 << 30;

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

/// The accessors of one model, each decoded once, the first time a
/// primitive or skin reads it, and kept for the next (UniVRM exports every
/// primitive of a mesh over one set of vertex accessors). What they decode
/// counts toward a budget, [`READ_BUDGET`] by default.
// Made by the model (Task 4).
#[allow(dead_code)]
pub(crate) struct Accessors<'d> {
    document: &'d Document,
    budget: u64,
    left: Cell<u64>,
    /// By accessor index and the element type it was decoded to. Looked up,
    /// never iterated.
    decoded: RefCell<HashMap<(usize, TypeId), Rc<dyn Any>>>,
}

// Called by the model (Task 4).
#[allow(dead_code)]
impl<'d> Accessors<'d> {
    pub(crate) fn new(document: &'d Document) -> Self {
        Self::with_budget(document, READ_BUDGET)
    }

    pub(crate) fn with_budget(document: &'d Document, budget: u64) -> Self {
        Self {
            document,
            budget,
            left: Cell::new(budget),
            decoded: RefCell::new(HashMap::new()),
        }
    }

    /// The bytes decoded so far.
    pub(crate) fn decoded(&self) -> u64 {
        self.budget - self.left.get()
    }
}

/// The bytes `geometry` takes: each stream's, its morph targets', and its
/// indices'. A model's geometries together stay within its geometry budget.
// Called by the model (Task 4).
#[allow(dead_code)]
pub(crate) fn geometry_bytes(geometry: &Geometry) -> u64 {
    let bytes = |size: usize| size as u64;
    let mut total = bytes(size_of_val(geometry.positions.as_slice()))
        + bytes(size_of_val(geometry.normals.as_slice()))
        + bytes(size_of_val(geometry.uvs.as_slice()))
        + bytes(size_of_val(geometry.indices.as_slice()));
    if let Some(tangents) = &geometry.tangents {
        total += bytes(size_of_val(tangents.as_slice()));
    }
    if let Some(joints) = &geometry.joints {
        total += bytes(size_of_val(joints.as_slice()));
    }
    if let Some(weights) = &geometry.weights {
        total += bytes(size_of_val(weights.as_slice()));
    }
    for target in &geometry.targets {
        total += bytes(size_of_val(target.positions.as_slice()))
            + bytes(size_of_val(target.normals.as_slice()));
    }
    total
}

/// `primitive` of `mesh` as a triangle list, or `None` when it draws no
/// triangles: points, lines, or a strip or fan of fewer than 3 vertices
/// (skipped, like points and lines). `skin`, the skin of the node drawing
/// the mesh, makes the geometry skinned when the primitive has `JOINTS_0`
/// and `WEIGHTS_0`; `needs_tangents` (the material has a `normalTexture`)
/// adds tangents. The geometry takes at most `budget` bytes
/// ([`geometry_bytes`]): a primitive that would take more is refused before
/// its streams are made.
// Called by the model (Task 4).
#[allow(dead_code)]
pub(crate) fn primitive_geometry<'d>(
    accessors: &Accessors<'d>,
    mesh: gltf::Mesh<'d>,
    primitive: gltf::Primitive<'d>,
    skin: Option<gltf::Skin<'d>>,
    needs_tangents: bool,
    budget: u64,
) -> Result<Option<PrimitiveGeometry>, LoadError> {
    let mode = primitive.mode();
    if !matches!(
        mode,
        Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan
    ) {
        return Ok(None);
    }
    let streams = Streams {
        accessors,
        at: At::Primitive {
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
        return Err(at.invalid(format_args!("has POSITION with no bufferView")));
    }
    let vertices = position.count();
    let positions = streams.floats3(&position, "POSITION", vertices)?;

    // Triangles, three indices each, over the file's vertices.
    let triangles = match primitive.indices() {
        Some(indices) => {
            let drawn = streams.indices(&indices)?;
            check_drawn(&drawn, vertices, at)?;
            triangle_list(mode, &drawn, at)?
        }
        None => {
            let count = u32::try_from(vertices)
                .map_err(|_| at.invalid(format_args!("has {vertices} vertices")))?;
            triangle_list(mode, &(0..count).collect::<Vec<u32>>(), at)?
        }
    };
    if triangles.is_empty() {
        // A strip or fan of fewer than 3 vertices: nothing to draw. (A list
        // cannot get here: its indices are at least one, in threes.)
        return Ok(None);
    }

    // The vertices the triangles use, in their order in the file. Sorted
    // rather than marked in a table of every vertex, so a primitive costs
    // its own triangles, not the size of an accessor it shares.
    let mut kept: Vec<usize> = triangles.iter().map(|&index| index as usize).collect();
    kept.sort_unstable();
    kept.dedup();
    let indices: Vec<u32> = triangles
        .iter()
        .map(|&index| kept.partition_point(|&vertex| vertex < index as usize) as u32)
        .collect();
    drop(triangles);

    // An empty target (`{}`), or a sparse one with no bufferView, costs the
    // file a few bytes and a dense copy here, so the targets are counted and
    // their total capped before any is read: every target's dense positions,
    // and its normals where it has them. Then the whole geometry is.
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
    if let Some(bytes) = past_budget(streams_per_vertex, kept.len(), budget) {
        return Err(at.invalid(format_args!(
            "has morph targets of {} MiB, more than {} MiB",
            bytes >> 20,
            budget >> 20
        )));
    }
    let skinned = skin.is_some()
        && (primitive.get(&Semantic::Joints(0)).is_some()
            || primitive.get(&Semantic::Weights(0)).is_some());
    let per_vertex = 2 * size_of::<[f32; 3]>()
        + size_of::<[f32; 2]>()
        + if needs_tangents {
            size_of::<[f32; 4]>()
        } else {
            0
        }
        + if skinned {
            size_of::<[u16; 4]>() + size_of::<[f32; 4]>()
        } else {
            0
        };
    let bytes = (kept.len() as u64)
        .saturating_mul(per_vertex as u64)
        .saturating_add(streams_per_vertex.saturating_mul(kept.len() as u64 * 12))
        .saturating_add(indices.len() as u64 * 4);
    if bytes > budget {
        return Err(at.invalid(format_args!(
            "makes {bytes} bytes of geometry, but the model has {budget} left of its {} MiB",
            GEOMETRY_BUDGET >> 20
        )));
    }

    // The kept vertices' values, each finite: a NaN or infinity the file
    // holds would otherwise reach Celesta and draw as NaN.
    let compact = |stream: &[[f32; 3]], what: &str| -> Result<Vec<[f32; 3]>, LoadError> {
        let kept: Vec<[f32; 3]> = kept.iter().map(|&vertex| stream[vertex]).collect();
        finite(at, what, &kept)?;
        Ok(kept)
    };

    let positions = compact(&positions, "POSITION")?;
    let normals = match primitive.get(&Semantic::Normals) {
        Some(normals) => compact(&streams.floats3(&normals, "NORMAL", vertices)?, "NORMAL")?,
        None => computed_normals(&positions, &indices),
    };
    let uvs: Vec<[f32; 2]> = match primitive.get(&Semantic::TexCoords(0)) {
        Some(uvs) => {
            let uvs = streams.uvs(&uvs, vertices)?;
            let uvs: Vec<[f32; 2]> = kept.iter().map(|&vertex| uvs[vertex]).collect();
            finite(at, "TEXCOORD_0", &uvs)?;
            uvs
        }
        None => vec![[0.0; 2]; kept.len()],
    };

    let (joints, weights) = match skin {
        Some(skin) => streams.skinning(&primitive, &skin, vertices, &kept)?,
        None => (None, None),
    };

    let mut targets = Vec::new();
    for target in primitive.morph_targets() {
        let positions = match target.positions() {
            Some(deltas) => {
                let what = "morph target POSITION";
                compact(&streams.floats3(&deltas, what, vertices)?, what)?
            }
            None => vec![[0.0; 3]; kept.len()],
        };
        let normals = match target.normals() {
            Some(deltas) => {
                let what = "morph target NORMAL";
                compact(&streams.floats3(&deltas, what, vertices)?, what)?
            }
            None => Vec::new(),
        };
        targets.push(MorphTarget { positions, normals });
    }

    let tangents = needs_tangents.then(|| tangents(&positions, &normals, &uvs, &indices));
    let geometry = Geometry {
        positions,
        normals,
        uvs,
        tangents,
        joints,
        weights,
        targets,
        indices,
    };
    debug_assert_eq!(geometry_bytes(&geometry), bytes);
    Ok(Some(PrimitiveGeometry {
        geometry,
        mesh: mesh.index(),
        primitive: primitive.index(),
    }))
}

/// Every index `drawn` names is one of the primitive's `vertices`.
fn check_drawn(drawn: &[u32], vertices: usize, at: &At) -> Result<(), LoadError> {
    match drawn.iter().find(|&&index| index as usize >= vertices) {
        Some(past) => Err(at.invalid(format_args!(
            "uses vertex {past}, past its {vertices} vertices"
        ))),
        None => Ok(()),
    }
}

/// Where in the file a primitive is, for messages: `mesh "Body" primitive 0`.
enum At<'a, 'd> {
    Primitive {
        mesh: &'a gltf::Mesh<'d>,
        primitive: usize,
    },
}

impl fmt::Display for At<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            At::Primitive { mesh, primitive } => match mesh.name() {
                Some(name) => write!(f, "mesh {name:?} primitive {primitive}"),
                None => write!(f, "mesh {} primitive {primitive}", mesh.index()),
            },
        }
    }
}

impl At<'_, '_> {
    fn invalid(&self, reason: fmt::Arguments) -> LoadError {
        LoadError::Invalid(format!("{self} {reason}"))
    }
}

/// Reads one primitive's (or skin's) accessors, each checked first.
struct Streams<'a, 'd> {
    accessors: &'a Accessors<'d>,
    at: At<'a, 'd>,
}

impl<'d> Streams<'_, 'd> {
    fn buffer(&self, buffer: gltf::Buffer<'d>) -> Option<&'d [u8]> {
        self.accessors
            .document
            .buffers
            .get(buffer.index())
            .map(|data| data.0.as_slice())
    }

    /// `accessor`'s elements, after [`Self::check`].
    fn read<T: Item>(&self, accessor: &Accessor<'d>, what: &str) -> Result<Vec<T>, LoadError> {
        Iter::<T>::new(accessor.clone(), |buffer| self.buffer(buffer))
            .map(Iterator::collect)
            .ok_or_else(|| self.at.invalid(format_args!("has {what} it cannot read")))
    }

    /// `accessor` as `decode` decodes it to `T`, from an earlier read when
    /// there was one. `count` is how many elements this use needs, when it
    /// needs a number; it is checked on every use. The first decode is
    /// checked ([`Self::check`]) and counted toward the model's budget
    /// before it runs; an accessor decodes to `T` the same way wherever it
    /// is used, since `decode` follows its own type.
    fn decoded<T: 'static>(
        &self,
        accessor: &Accessor<'d>,
        what: &str,
        count: Option<usize>,
        decode: impl FnOnce() -> Result<Vec<T>, LoadError>,
    ) -> Result<Rc<Vec<T>>, LoadError> {
        self.check_count(accessor, what, count)?;
        let key = (accessor.index(), TypeId::of::<T>());
        let earlier = self.accessors.decoded.borrow().get(&key).cloned();
        if let Some(earlier) = earlier
            && let Ok(earlier) = earlier.downcast::<Vec<T>>()
        {
            return Ok(earlier);
        }
        self.check(accessor, what)?;
        // The sparse indices are walked by `check_sparse_order`, so they
        // count too.
        let sparse = accessor.sparse().map_or(0, |sparse| {
            (sparse.count() as u64).saturating_mul(sparse.indices().index_type().size() as u64)
        });
        let bytes = (accessor.count() as u64)
            .saturating_mul(size_of::<T>() as u64)
            .saturating_add(sparse);
        let left = self.accessors.left.get();
        if bytes > left {
            return Err(self.at.invalid(format_args!(
                "has {what} of {bytes} bytes to decode, past the {left} bytes left of the {} MiB a model's accessors may decode",
                self.accessors.budget >> 20
            )));
        }
        self.accessors.left.set(left - bytes);
        self.check_sparse_order(accessor, what)?;
        let decoded = Rc::new(decode()?);
        self.accessors
            .decoded
            .borrow_mut()
            .insert(key, decoded.clone());
        Ok(decoded)
    }

    /// `VEC3` floats: positions, normals, and their morph deltas.
    fn floats3(
        &self,
        accessor: &Accessor<'d>,
        what: &str,
        count: usize,
    ) -> Result<Rc<Vec<[f32; 3]>>, LoadError> {
        self.expect(accessor, what, Dimensions::Vec3, &[DataType::F32])?;
        self.decoded(accessor, what, Some(count), || self.read(accessor, what))
    }

    /// `TEXCOORD_0` as floats, normalised integers widened.
    fn uvs(&self, accessor: &Accessor<'d>, count: usize) -> Result<Rc<Vec<[f32; 2]>>, LoadError> {
        let what = "TEXCOORD_0";
        self.expect(
            accessor,
            what,
            Dimensions::Vec2,
            &[DataType::F32, DataType::U8, DataType::U16],
        )?;
        self.decoded(accessor, what, Some(count), || {
            Ok(match accessor.data_type() {
                DataType::U8 => widen(self.read::<[u8; 2]>(accessor, what)?, 255.0),
                DataType::U16 => widen(self.read::<[u16; 2]>(accessor, what)?, 65535.0),
                _ => self.read(accessor, what)?,
            })
        })
    }

    /// The triangles' vertex indices as the file lists them.
    fn indices(&self, accessor: &Accessor<'d>) -> Result<Rc<Vec<u32>>, LoadError> {
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
        self.decoded(accessor, what, None, || {
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
        let joints: Rc<Vec<[u16; 4]>> = self.decoded(&joints, "JOINTS_0", Some(count), || {
            Ok(match joints.data_type() {
                DataType::U8 => self
                    .read::<[u8; 4]>(&joints, "JOINTS_0")?
                    .into_iter()
                    .map(|joint| joint.map(u16::from))
                    .collect(),
                _ => self.read(&joints, "JOINTS_0")?,
            })
        })?;
        self.expect(
            &weights,
            "WEIGHTS_0",
            Dimensions::Vec4,
            &[DataType::F32, DataType::U8, DataType::U16],
        )?;
        let weights: Rc<Vec<[f32; 4]>> =
            self.decoded(&weights, "WEIGHTS_0", Some(count), || {
                Ok(match weights.data_type() {
                    DataType::U8 => widen(self.read::<[u8; 4]>(&weights, "WEIGHTS_0")?, 255.0),
                    DataType::U16 => widen(self.read::<[u16; 4]>(&weights, "WEIGHTS_0")?, 65535.0),
                    _ => self.read(&weights, "WEIGHTS_0")?,
                })
            })?;

        let joint_count = skin.joints().len();
        let mut kept_joints = Vec::with_capacity(kept.len());
        let mut kept_weights = Vec::with_capacity(kept.len());
        for &vertex in kept {
            let weight = weights[vertex];
            finite(&self.at, "WEIGHTS_0", &[weight])?;
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
            "has {what} of {:?} {:?}, not {dimensions:?} of {types:?}",
            accessor.dimensions(),
            accessor.data_type()
        )))
    }

    /// `accessor` has at least one element (the reader computes
    /// `count − 1`), and `count` of them when this use needs a number.
    fn check_count(
        &self,
        accessor: &Accessor<'d>,
        what: &str,
        count: Option<usize>,
    ) -> Result<(), LoadError> {
        let elements = accessor.count();
        if elements == 0 {
            return Err(self.at.invalid(format_args!("has {what} with no elements")));
        }
        if let Some(count) = count
            && elements != count
        {
            return Err(self
                .at
                .invalid(format_args!("has {elements} {what} for {count} vertices")));
        }
        Ok(())
    }

    /// Everything else `gltf`'s reader assumes of `accessor` (after
    /// [`Self::check_count`]): each view's stride at least an element, and
    /// every element inside its view and buffer; a sparse accessor's values
    /// no more than its elements.
    fn check(&self, accessor: &Accessor<'d>, what: &str) -> Result<(), LoadError> {
        let elements = accessor.count();
        let size = accessor.size();
        match accessor.view() {
            Some(view) => self.check_view(&view, accessor.offset(), size, elements, what)?,
            None if accessor.sparse().is_none() => {
                return Err(self.at.invalid(format_args!(
                    "has {what} with neither a bufferView nor sparse values"
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
                "has {what} with {values} sparse values for {elements} elements"
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
        )
    }

    /// A sparse accessor's indices rise and stay below its count (after
    /// [`Self::check`]): the reader matches indices to elements in order
    /// and skips the rest, so indices out of order would silently drop
    /// values.
    fn check_sparse_order(&self, accessor: &Accessor<'d>, what: &str) -> Result<(), LoadError> {
        let Some(sparse) = accessor.sparse() else {
            return Ok(());
        };
        let elements = accessor.count();
        let values = sparse.count();
        let indices = sparse.indices();
        let index_size = indices.index_type().size();
        let view = indices.view();
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
                        "has {what} whose sparse indices are not rising below {elements}"
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
                "has {what} in buffer view {} whose stride, {stride}, is less than its {size}-byte elements",
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
                "has {what} in buffer view {}, which runs past its buffer's {buffer} bytes",
                view.index()
            )));
        }
        let end = stride
            .checked_mul(count - 1)
            .and_then(|bytes| bytes.checked_add(size))
            .and_then(|bytes| bytes.checked_add(offset));
        if end.is_none_or(|end| end > view.length()) {
            return Err(self.at.invalid(format_args!(
                "has {what} of {count} elements, which runs past buffer view {}'s {} bytes",
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

/// Refuses `values` of stream `what` when one is NaN or infinite.
fn finite<const N: usize>(at: &At, what: &str, values: &[[f32; N]]) -> Result<(), LoadError> {
    match values
        .iter()
        .find(|value| value.iter().any(|v| !v.is_finite()))
    {
        Some(value) => Err(at.invalid(format_args!("has {what} that is not finite: {value:?}"))),
        None => Ok(()),
    }
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
fn triangle_list(mode: Mode, drawn: &[u32], at: &At) -> Result<Vec<u32>, LoadError> {
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
        _ => Ok(drawn.to_vec()),
    }
}

/// One vertex's four joint slots for a skin of `joint_count` joints: a slot
/// of weight 0 names joint 0 with weight 0, and the weights are scaled to
/// sum to 1 where they sum to more than 0. A negative weight, which glTF
/// forbids, is zeroed the same way rather than refused: Unity ignores it,
/// so UniVRM draws such a file. `Err` is the joint a weighted slot names
/// that the skin lacks.
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
        // The file must open: only `primitive_geometry` may refuse it here.
        let document = match Document::parse(&vrm.glb_with(edit)) {
            Ok(document) => document,
            Err(error) => panic!("the file did not parse: {error}"),
        };
        let accessors = Accessors::new(&document);
        read_mesh(&accessors, mesh, tangents, GEOMETRY_BUDGET)
    }

    /// Mesh `mesh`'s primitives through `accessors`, each with `budget`
    /// bytes of geometry to make.
    fn read_mesh(
        accessors: &Accessors,
        mesh: usize,
        tangents: bool,
        budget: u64,
    ) -> Result<Vec<Option<Geometry>>, LoadError> {
        let document = accessors.document;
        let mesh = document.gltf.meshes().nth(mesh).unwrap();
        let skin = (mesh.index() == 0).then(|| document.gltf.skins().next().unwrap());
        mesh.primitives()
            .map(|primitive| {
                let needs = tangents || primitive.material().normal_texture().is_some();
                let read = primitive_geometry(
                    accessors,
                    mesh.clone(),
                    primitive,
                    skin.clone(),
                    needs,
                    budget,
                )?;
                Ok(read.map(|read| read.geometry))
            })
            .collect()
    }

    fn parse(glb: &[u8]) -> Document {
        match Document::parse(glb) {
            Ok(document) => document,
            Err(error) => panic!("the file did not parse: {error}"),
        }
    }

    /// The avatar with `copies` copies of Body primitive 0 after its two,
    /// each sharing primitive 0's accessors, then `edit`.
    fn copied_box(copies: usize, edit: impl FnOnce(&mut Value)) -> Document {
        parse(&SyntheticVrm::avatar().glb_with(|root| {
            let primitives = root["meshes"][0]["primitives"].as_array_mut().unwrap();
            let first = primitives[0].clone();
            primitives.extend(std::iter::repeat_n(first, copies));
            edit(root);
        }))
    }

    #[test]
    fn shared_accessors_are_decoded_once() {
        // Body's own two primitives.
        let alone = copied_box(0, |_| {});
        let accessors = Accessors::new(&alone);
        read_mesh(&accessors, 0, false, GEOMETRY_BUDGET).unwrap();
        let once = accessors.decoded();
        assert!(once > 0);

        // 500 more primitives over the same accessors decode nothing more.
        let copied = copied_box(500, |_| {});
        let accessors = Accessors::new(&copied);
        let read = read_mesh(&accessors, 0, false, GEOMETRY_BUDGET).unwrap();
        assert_eq!(read.len(), 502);
        assert_eq!(read[501], read[0]);
        assert_eq!(accessors.decoded(), once);
    }

    #[test]
    fn aliased_accessors_share_one_read_budget() {
        // Each copy has its own POSITION accessor, all over the same bytes,
        // so the file's size bounds nothing a cache would.
        let document = copied_box(500, |root| {
            let position = accessor(root, "POSITION");
            let copy = root["accessors"][position].clone();
            let accessors = root["accessors"].as_array_mut().unwrap();
            let first = accessors.len();
            accessors.extend(std::iter::repeat_n(copy, 500));
            let primitives = root["meshes"][0]["primitives"].as_array_mut().unwrap();
            for (index, primitive) in primitives[2..].iter_mut().enumerate() {
                primitive["attributes"]["POSITION"] = json!(first + index);
            }
        });
        let alone = Accessors::new(&document);
        let mesh = document.gltf.meshes().next().unwrap();
        let skin = document.gltf.skins().next();
        for primitive in mesh.primitives().take(2) {
            primitive_geometry(
                &alone,
                mesh.clone(),
                primitive,
                skin.clone(),
                false,
                GEOMETRY_BUDGET,
            )
            .unwrap();
        }
        // Room for Body's own primitives and ten aliases of 12 positions.
        let budget = alone.decoded() + 10 * 12 * 12;
        let accessors = Accessors::with_budget(&document, budget);
        let message = match read_mesh(&accessors, 0, false, GEOMETRY_BUDGET) {
            Ok(_) => panic!("every alias was decoded"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains(
                "mesh \"Body\" primitive 12 has POSITION of 144 bytes to decode, past the 0 bytes left"
            ),
            "{message}"
        );
        assert_eq!(accessors.decoded(), budget);
    }

    #[test]
    fn many_primitives_over_one_large_accessor_are_read_once() {
        // Mesh 1 as 5,000 one-triangle primitives over one accessor of
        // 100,000 positions: read for each, they would decode 6 GB.
        const VERTICES: usize = 100_000;
        const PRIMITIVES: usize = 5_000;
        use base64::Engine as _;
        let mut bytes = vec![0u8; VERTICES * 12];
        for index in [0u16, 50_000, 65_535] {
            bytes.extend(index.to_le_bytes());
        }
        let uri = format!(
            "data:application/octet-stream;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        let document = parse(&SyntheticVrm::avatar().glb_with(|root| {
            root["buffers"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "byteLength": bytes.len(), "uri": uri }));
            let buffer = root["buffers"].as_array().unwrap().len() - 1;
            let views = root["bufferViews"].as_array_mut().unwrap();
            views.push(json!({ "buffer": buffer, "byteLength": VERTICES * 12 }));
            views.push(json!({ "buffer": buffer, "byteOffset": VERTICES * 12, "byteLength": 6 }));
            let positions = views.len() - 2;
            let accessors = root["accessors"].as_array_mut().unwrap();
            accessors.push(json!({
                "bufferView": positions, "componentType": 5126, "count": VERTICES,
                "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [0.0, 0.0, 0.0],
            }));
            accessors.push(json!({
                "bufferView": positions + 1, "componentType": 5123, "count": 3, "type": "SCALAR",
            }));
            let primitive = json!({
                "attributes": { "POSITION": accessors.len() - 2 },
                "indices": accessors.len() - 1,
            });
            root["meshes"][1]["primitives"] = json!(vec![primitive; PRIMITIVES]);
        }));
        let accessors = Accessors::new(&document);
        let read = read_mesh(&accessors, 1, false, GEOMETRY_BUDGET).unwrap();
        assert_eq!(read.len(), PRIMITIVES);
        for geometry in &read {
            let geometry = geometry.as_ref().unwrap();
            assert_eq!(geometry.positions.len(), 3);
            assert_eq!(geometry.indices, [0, 1, 2]);
        }
        assert_eq!(accessors.decoded(), (VERTICES * 12 + 3 * 4) as u64);
    }

    #[test]
    fn a_primitive_past_its_geometry_budget_is_refused_before_it_is_made() {
        let document = parse(&SyntheticVrm::avatar().glb());
        let read = |budget| {
            let accessors = Accessors::new(&document);
            let mesh = document.gltf.meshes().next().unwrap();
            let primitive = mesh.primitives().next().unwrap();
            let skin = document.gltf.skins().next();
            primitive_geometry(&accessors, mesh.clone(), primitive, skin, true, budget)
                .map(|read| read.unwrap().geometry)
        };
        let cube = read(GEOMETRY_BUDGET).unwrap();
        // 8 vertices: positions, normals 96 bytes each; UVs and joints 64;
        // weights and tangents 128; 3 targets 288; 36 indices 144.
        assert_eq!(
            geometry_bytes(&cube),
            96 + 96 + 64 + 128 + 64 + 128 + 288 + 144
        );
        assert_eq!(read(1008).unwrap(), cube);
        let message = match read(1007) {
            Ok(_) => panic!("the primitive was made"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("mesh \"Body\" primitive 0 makes 1008 bytes of geometry")
                && message.contains("1007 left"),
            "{message}"
        );
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
            past_budget(u64::MAX, usize::MAX, GEOMETRY_BUDGET),
            Some(u64::MAX)
        );
        assert_eq!(past_budget(0, usize::MAX, GEOMETRY_BUDGET), None);
    }

    /// Body primitive 0 with `extra` empty targets (`{}`) after its three,
    /// read with a geometry budget of `budget` bytes.
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
        let accessors = Accessors::new(&document);
        let read = primitive_geometry(&accessors, mesh.clone(), primitive, skin, false, budget)?;
        Ok(read.map(|read| read.geometry))
    }

    #[test]
    fn empty_targets_count_toward_the_budget() {
        // 13 targets, each with dense positions over 8 vertices: 1248 bytes,
        // and 592 for the rest of the box.
        let cube = empty_targets(10, 1248 + 592).unwrap().unwrap();
        assert_eq!(cube.targets.len(), 13);
        for target in &cube.targets[3..] {
            assert_eq!(target.positions, vec![[0.0; 3]; 8]);
            assert!(target.normals.is_empty());
        }
        let message = match empty_targets(10, 1248 + 591) {
            Ok(_) => panic!("the box was read"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("bytes of geometry"), "{message}");
        let message = match empty_targets(10, 1247) {
            Ok(_) => panic!("the targets were read"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("morph targets of"), "{message}");
    }

    #[test]
    fn too_many_targets_are_refused() {
        let message = match empty_targets(MAX_TARGETS - 2, GEOMETRY_BUDGET) {
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
    fn strips_and_fans_without_a_triangle_are_skipped() {
        for mode in [5, 6] {
            for count in [1, 2] {
                let read = body_with(|root| {
                    root["meshes"][0]["primitives"][1]["mode"] = json!(mode);
                    let indices = root["meshes"][0]["primitives"][1]["indices"]
                        .as_u64()
                        .unwrap() as usize;
                    root["accessors"][indices]["count"] = json!(count);
                })
                .unwrap();
                assert_eq!(read[1], None, "mode {mode}, {count} indices");
            }
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
        for (semantic, field, value, expected) in [
            (
                "POSITION",
                "type",
                json!("VEC2"),
                "has POSITION of Vec2 F32, not Vec3",
            ),
            (
                "JOINTS_0",
                "componentType",
                json!(5126),
                "has JOINTS_0 of Vec4 F32, not Vec4",
            ),
            (
                "WEIGHTS_0",
                "componentType",
                json!(5125),
                "has WEIGHTS_0 of Vec4 U32, not Vec4",
            ),
            (
                "TEXCOORD_0",
                "type",
                json!("SCALAR"),
                "has TEXCOORD_0 of Scalar F32, not Vec2",
            ),
            (
                "indices",
                "type",
                json!("VEC3"),
                "has indices of Vec3 U16, not Scalar",
            ),
            (
                "indices",
                "componentType",
                json!(5126),
                "has indices of Scalar F32, not Scalar",
            ),
            (
                "target0",
                "componentType",
                json!(5123),
                "has morph target POSITION of Vec3 U16, not Vec3",
            ),
        ] {
            let message = refusal(|root| {
                let accessor = accessor(root, semantic);
                root["accessors"][accessor][field] = value;
            });
            assert!(message.contains(expected), "{semantic}: {message}");
        }
    }

    #[test]
    fn non_finite_streams_are_refused() {
        // Six NaN floats in a second, `data:` buffer, swapped in for vertex
        // 2 (the box's second index) by a sparse value.
        const NANS: &str = "data:application/octet-stream;base64,\
                            AADAfwAAwH8AAMB/AADAfwAAwH8AAMB/";
        for semantic in ["POSITION", "NORMAL", "TEXCOORD_0", "WEIGHTS_0", "target0"] {
            let message = refusal(|root| {
                let indices = view(root, "indices");
                root["buffers"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({ "byteLength": 24, "uri": NANS }));
                let views = root["bufferViews"].as_array_mut().unwrap();
                views.push(json!({ "buffer": 1, "byteLength": 24 }));
                let nans = views.len() - 1;
                let accessor = accessor(root, semantic);
                root["accessors"][accessor]["sparse"] = json!({
                    "count": 1,
                    "indices": { "bufferView": indices, "byteOffset": 2, "componentType": 5123 },
                    "values": { "bufferView": nans },
                });
            });
            let name = if semantic == "target0" {
                "morph target POSITION"
            } else {
                semantic
            };
            assert!(
                message.contains(&format!(
                    "mesh \"Body\" primitive 0 has {name} that is not finite"
                )),
                "{semantic}: {message}"
            );
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
        for (semantic, field, value, expected) in [
            (
                "POSITION",
                "count",
                json!(0),
                "has POSITION with no elements",
            ),
            (
                "POSITION",
                "count",
                json!(1u64 << 40),
                "has POSITION of 1099511627776 elements, which runs past buffer view",
            ),
            (
                "POSITION",
                "byteOffset",
                json!(1_000_000),
                "has POSITION of 12 elements, which runs past buffer view",
            ),
            (
                "indices",
                "count",
                json!(1u64 << 40),
                "has indices of 1099511627776 elements, which runs past buffer view",
            ),
            ("indices", "count", json!(0), "has indices with no elements"),
            (
                "target0",
                "count",
                json!(1u64 << 40),
                "has 1099511627776 morph target POSITION for 12 vertices",
            ),
        ] {
            let message = refusal(|root| {
                let accessor = accessor(root, semantic);
                root["accessors"][accessor][field] = value;
            });
            assert!(message.contains(expected), "{semantic} {field}: {message}");
        }
        for (field, value) in [
            ("byteLength", json!(1u64 << 40)),
            ("byteOffset", json!(u64::MAX - 8)),
        ] {
            let message = refusal(|root| {
                let positions = view(root, "POSITION");
                root["bufferViews"][positions][field] = value;
            });
            assert!(
                message.contains("has POSITION in buffer view")
                    && message.contains("which runs past its buffer's"),
                "{field}: {message}"
            );
        }
    }

    #[test]
    fn broken_sparse_accessors_are_refused() {
        // View 0 is the POSITION data: 144 bytes of floats.
        for (sparse, expected) in [
            (
                json!({ "count": 0, "indices": { "bufferView": 0, "componentType": 5123 }, "values": { "bufferView": 0 } }),
                "with 0 sparse values for 12 elements",
            ),
            (
                json!({ "count": 13, "indices": { "bufferView": 0, "componentType": 5123 }, "values": { "bufferView": 0 } }),
                "with 13 sparse values for 12 elements",
            ),
            (
                json!({ "count": 1, "indices": { "bufferView": 0, "byteOffset": 1_000_000, "componentType": 5123 }, "values": { "bufferView": 0 } }),
                "has morph target POSITION of 1 elements, which runs past buffer view 0",
            ),
            (
                json!({ "count": 1, "indices": { "bufferView": 0, "componentType": 5125 }, "values": { "bufferView": 0, "byteOffset": 1_000_000 } }),
                "has morph target POSITION of 1 elements, which runs past buffer view 0",
            ),
            (
                json!({ "count": 12, "indices": { "bufferView": 0, "componentType": 5125 }, "values": { "bufferView": 0 } }),
                "whose sparse indices are not rising below 12",
            ),
        ] {
            let message = refusal(|root| {
                let target = accessor(root, "target0");
                let accessor = root["accessors"][target].as_object_mut().unwrap();
                accessor.remove("bufferView");
                accessor.insert("sparse".into(), sparse);
            });
            assert!(message.contains(expected), "{message}");
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
