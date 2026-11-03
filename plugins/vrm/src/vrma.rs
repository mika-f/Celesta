// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/VRM10/Runtime/: IO/VrmAnimationImporter.cs,
// ControlRig/InitialRotationPoseProvider.cs, ControlRig/BoneInitialRotation.cs,
// ControlRig/Vrm10Retarget.cs, and Components/Vrm10Runtime/ControlRig/:
// Vrm10ControlBone.cs and Vrm10RuntimeControlRig.cs.
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

//! VRM animations (`.vrma`): a clip of humanoid motion and expressions, and
//! UniVRM's retarget of it onto an avatar.
//!
//! - **The clip** is `VRMC_vrm_animation` over the file's first glTF
//!   animation: the `rotation` of each humanoid bone's node, the
//!   `translation` of the hips' node, and, for each expression, the
//!   `translation` x of its node (`VrmAnimationImporter`). Other channels,
//!   the eye bones (UniVRM's importer leaves them out), the look
//!   expressions (UniVRM's clip has no field for them), and the lookAt are
//!   not read.
//! - **Sampling.** Times clamp to a channel's first and last key. `STEP`
//!   holds a key; `LINEAR` lerps translations and weights and slerps
//!   rotations along the shorter arc, as glTF asks (UniVRM's Unity curves
//!   lerp a rotation's components and normalise, a difference far below a
//!   degree at a clip's key rate); `CUBICSPLINE` is glTF's Hermite spline
//!   with tangents scaled by the key interval, a rotation normalised.
//! - **Normalized rotations** (`BoneInitialRotation`): a bone whose node
//!   rests at local rotation `L₀` and world rotation `G₀` turns by
//!   `q = G₀ · L₀⁻¹ · L(t) · G₀⁻¹`, its turn from rest in the clip's root
//!   axes, whatever its rest. The raw hips are the hips' node's position at
//!   `t` in the clip's root space.
//! - **The retarget** (`Vrm10ControlBone`, `Vrm10Retarget`): on an avatar
//!   bone resting at `L₀'` and `G₀'`, `local = L₀' · (G₀'⁻¹ · q · G₀')`; a
//!   bone the clip does not key takes `q = 1`, its rest. The hips go to
//!   `H₀' + (raw − H₀) · (H₀'.y / H₀.y)`, the clip's travel scaled by the
//!   avatar's hips height over the clip's, written as the hips' local
//!   translation under their parent's current world.
//! - **Hostile files.** A `.vrma` is read through the model's limits (the
//!   file and its JSON); its key times must be finite and rise, its values
//!   finite, its rotations of non-zero length; it keys at most
//!   [`MAX_CHANNELS`] bones and expressions, and its channels hold at most
//!   [`CLIP_BUDGET`] bytes of keys.

use crate::document::{LoadError, embedded_buffers, json_and_blob};
use crate::expressions::{ExpressionKey, Preset};
use crate::geometry::{Accessors, At, floats};
use crate::humanoid::{HumanBone, Humanoid};
use crate::nodes::{NodeTree, Pose};
use glam::{Mat4, Quat, Vec3, Vec4};
use gltf::accessor::Dimensions;
use gltf::animation::{Interpolation, Property};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Add, Mul};
use vrm_spec::vrmc_vrm_1_0::ExpressionPresetName;
use vrm_spec::vrmc_vrm_animation_1_0::{VRMC_VRM_ANIMATION, VRMCVrmAnimationSchema};

/// The most bones and expressions a clip may key: far past a humanoid's 55
/// bones, its hips, and any face's expressions, so a file of a few JSON
/// bytes per expression cannot make each frame walk millions of channels.
pub(crate) const MAX_CHANNELS: usize = 1024;

/// The most bytes of keys a clip's channels may hold, times and values as
/// glTF stores them, a channel counted however many share its accessors:
/// far past any motion (ten minutes at 60 fps on every bone is under
/// 40 MiB).
pub(crate) const CLIP_BUDGET: u64 = 256 << 20;

/// The lowest the clip's hips may rest when it moves them: their travel is
/// scaled by the avatar's hips height over this one.
const MIN_HIPS_HEIGHT: f32 = 1e-3;

/// A VRM animation, read and ready to sample.
#[derive(Clone, Debug)]
pub struct Clip {
    /// The last key time over every channel read, in seconds.
    pub duration: f64,
    /// By bone order.
    bones: Vec<BoneChannel>,
    hips: Option<HipsChannel>,
    /// The hips' node's rest position in the clip's root space (`H₀`); the
    /// origin without a humanoid.
    hips_rest: Vec3,
    /// By key order.
    expressions: Vec<(ExpressionKey, Channel<f32>)>,
}

/// What a clip says at one time.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ClipPose {
    /// Each keyed bone's normalized local rotation: its turn from rest in
    /// the clip's root axes.
    pub rotations: BTreeMap<HumanBone, Quat>,
    /// The hips' position in the clip's root space, when the clip moves
    /// them.
    pub hips: Option<Vec3>,
    /// Each keyed expression's weight.
    pub expressions: BTreeMap<ExpressionKey, f32>,
}

/// A bone's rotation keys and its node's rest.
#[derive(Clone, Debug)]
struct BoneChannel {
    bone: HumanBone,
    /// `G₀ · L₀⁻¹`.
    before: Quat,
    /// `G₀⁻¹`.
    after: Quat,
    rotation: Channel<Quat>,
}

/// The hips' translation keys and where they move from.
#[derive(Clone, Debug)]
struct HipsChannel {
    /// The rest world of the hips' parent (the identity at the root).
    parent: Mat4,
    translation: Channel<Vec3>,
}

/// One sampler's keys: rising times, and a value per time (three for
/// `CUBICSPLINE`: in-tangent, value, out-tangent).
#[derive(Clone, Debug)]
struct Channel<T> {
    interpolation: Interpolation,
    times: Vec<f32>,
    values: Vec<T>,
}

/// A value a channel interpolates.
trait Key: Copy + Add<Output = Self> + Mul<f32, Output = Self> {
    /// `LINEAR` from `self` to `other`, `s` of the way.
    fn linear(self, other: Self, s: f32) -> Self;
    /// A `CUBICSPLINE` result as a value (a rotation normalised), else
    /// `fallback`.
    fn settle(self, _fallback: Self) -> Self {
        self
    }
}

impl Key for f32 {
    fn linear(self, other: Self, s: f32) -> Self {
        self + (other - self) * s
    }
}

impl Key for Vec3 {
    fn linear(self, other: Self, s: f32) -> Self {
        self.lerp(other, s)
    }
}

impl Key for Quat {
    fn linear(self, other: Self, s: f32) -> Self {
        self.slerp(other, s)
    }

    fn settle(self, fallback: Self) -> Self {
        Vec4::from(self)
            .try_normalize()
            .map_or(fallback, Quat::from_vec4)
    }
}

impl<T: Key> Channel<T> {
    fn last_time(&self) -> f64 {
        self.times.last().map_or(0.0, |&time| f64::from(time))
    }

    /// Key `k`'s value.
    fn value(&self, k: usize) -> T {
        match self.interpolation {
            Interpolation::CubicSpline => self.values[3 * k + 1],
            _ => self.values[k],
        }
    }

    fn sample(&self, seconds: f64) -> T {
        let times = &self.times;
        let last = times.len() - 1;
        // Before the first key (or at no time at all), the first value;
        // from the last, the last: the keys themselves, never a blend.
        if seconds.is_nan() || seconds <= f64::from(times[0]) {
            return self.value(0);
        }
        if seconds >= f64::from(times[last]) {
            return self.value(last);
        }
        // times[k] ≤ seconds < times[k + 1], times rising.
        let k = times.partition_point(|&time| f64::from(time) <= seconds) - 1;
        let (t0, t1) = (f64::from(times[k]), f64::from(times[k + 1]));
        let s = ((seconds - t0) / (t1 - t0)) as f32;
        match self.interpolation {
            Interpolation::Step => self.value(k),
            Interpolation::Linear => self.value(k).linear(self.value(k + 1), s),
            Interpolation::CubicSpline => {
                let interval = (t1 - t0) as f32;
                let (p0, out_tangent) = (self.values[3 * k + 1], self.values[3 * k + 2]);
                let (in_tangent, p1) = (self.values[3 * k + 3], self.values[3 * k + 4]);
                let (s2, s3) = (s * s, s * s * s);
                let spline = p0 * (2.0 * s3 - 3.0 * s2 + 1.0)
                    + out_tangent * (interval * (s3 - 2.0 * s2 + s))
                    + p1 * (-2.0 * s3 + 3.0 * s2)
                    + in_tangent * (interval * (s3 - s2));
                spline.settle(p0)
            }
        }
    }
}

/// What the root says about VRM animation before glTF reads it: the rest
/// of the JSON is skipped, not parsed.
#[derive(Deserialize)]
struct Head {
    #[serde(default)]
    extensions: Option<HeadExtensions>,
}

#[derive(Deserialize)]
struct HeadExtensions {
    #[serde(rename = "VRMC_vrm_animation", default)]
    animation: Option<Value>,
}

/// What a channel of the clip drives.
#[derive(Clone, Copy)]
enum Target {
    Bone(usize),
    Hips,
    Expression(usize),
}

impl Clip {
    /// Reads a `.vrma` (binary glTF), or a glTF JSON file whose buffers are
    /// `data:` URIs, through the model's JSON limit.
    pub fn load(bytes: &[u8]) -> Result<Self, LoadError> {
        Self::load_within(bytes, CLIP_BUDGET)
    }

    /// [`Self::load`] with `budget` bytes of keys.
    fn load_within(bytes: &[u8], budget: u64) -> Result<Self, LoadError> {
        let (json, blob) = json_and_blob(bytes)?;
        let head: Head = serde_json::from_slice(&json).map_err(gltf::Error::Deserialize)?;
        let Some(extension) = head.extensions.and_then(|extensions| extensions.animation) else {
            return Err(invalid(format_args!(
                "this glTF file has no {VRMC_VRM_ANIMATION} extension: it is not a VRM animation"
            )));
        };
        match extension["specVersion"].as_str() {
            Some("1.0") => {}
            Some(version) => {
                return Err(invalid(format_args!(
                    "{VRMC_VRM_ANIMATION} specVersion {version} is not 1.0"
                )));
            }
            None => {
                return Err(invalid(format_args!(
                    "{VRMC_VRM_ANIMATION} has no specVersion"
                )));
            }
        }
        let vrma = VRMCVrmAnimationSchema::deserialize(extension)
            .map_err(|error| invalid(format_args!("{VRMC_VRM_ANIMATION} is malformed: {error}")))?;

        let root = gltf::json::Root::from_slice(&json).map_err(gltf::Error::Deserialize)?;
        let gltf = gltf::Document::from_json(root)?;
        let tree = NodeTree::from_gltf(&gltf)?;
        let count = tree.parents.len();
        let bones = clip_bones(&vrma, count)?;
        let expressions = clip_expressions(&vrma, count)?;
        let buffers = embedded_buffers(&gltf, blob, ".vrma")?;

        let rest_worlds = tree.world(&tree.rest_pose());
        let rest_rotations = rest_world_rotations(&tree);
        let hips_node = bones.get(&HumanBone::Hips).copied();
        let hips_rest = hips_node.map_or(Vec3::ZERO, |hips| rest_worlds[hips].w_axis.truncate());

        let mut targets = vec![Vec::new(); count];
        for (&bone, &node) in &bones {
            targets[node].push((Property::Rotation, Target::Bone(node)));
            if bone == HumanBone::Hips {
                targets[node].push((Property::Translation, Target::Hips));
            }
        }
        for (index, (_, node)) in expressions.iter().enumerate() {
            targets[*node].push((Property::Translation, Target::Expression(index)));
        }
        let bone_at: BTreeMap<usize, HumanBone> =
            bones.iter().map(|(&bone, &node)| (node, bone)).collect();

        let mut clip = Self {
            duration: 0.0,
            bones: Vec::new(),
            hips: None,
            hips_rest,
            expressions: Vec::new(),
        };
        let mut expression_channels = BTreeMap::new();
        if let Some(animation) = gltf.animations().next() {
            let accessors = Accessors::of_buffers(&buffers, budget);
            let mut left = budget;
            let mut seen = BTreeSet::new();
            let mut kept = 0;
            for channel in animation.channels() {
                let node = channel.target().node().index();
                let property = channel.target().property();
                let Some(&(_, target)) =
                    targets[node].iter().find(|(wanted, _)| *wanted == property)
                else {
                    continue;
                };
                let path = match property {
                    Property::Rotation => "rotation",
                    _ => "translation",
                };
                if !seen.insert((node, path)) {
                    return Err(invalid(format_args!(
                        "animation {} has two {path} channels for node {node}",
                        animation.index()
                    )));
                }
                kept += 1;
                if kept > MAX_CHANNELS {
                    return Err(invalid(format_args!(
                        "animation {} keys more than the {MAX_CHANNELS} bones and expressions a VRM animation may",
                        animation.index()
                    )));
                }
                let mut reader = Reader {
                    accessors: &accessors,
                    sampler: channel.sampler(),
                    left: &mut left,
                };
                match target {
                    Target::Bone(node) => {
                        let rotation = reader.read(path, Dimensions::Vec4, |[x, y, z, w]| {
                            Quat::from_xyzw(x, y, z, w)
                        })?;
                        let rotation = reader.rotations(rotation)?;
                        let (local, world) = (tree.rest[node].rotation, rest_rotations[node]);
                        clip.bones.push(BoneChannel {
                            bone: bone_at[&node],
                            before: world * local.inverse(),
                            after: world.inverse(),
                            rotation,
                        });
                    }
                    Target::Hips => {
                        let translation =
                            reader.read(path, Dimensions::Vec3, |[x, y, z]| Vec3::new(x, y, z))?;
                        let parent = tree.parents[node].map_or(Mat4::IDENTITY, |p| rest_worlds[p]);
                        clip.hips = Some(HipsChannel {
                            parent,
                            translation,
                        });
                    }
                    Target::Expression(index) => {
                        let weight =
                            reader.read(path, Dimensions::Vec3, |[x, _, _]: [f32; 3]| x)?;
                        expression_channels.insert(index, weight);
                    }
                }
            }
        }
        clip.bones.sort_by_key(|channel| channel.bone);
        clip.expressions = expressions
            .into_iter()
            .enumerate()
            .filter_map(|(index, (key, _))| Some((key, expression_channels.remove(&index)?)))
            .collect();
        // Each rest transform is finite, but their product can overflow.
        if clip.hips.is_some() && (!hips_rest.is_finite() || hips_rest.y < MIN_HIPS_HEIGHT) {
            return Err(invalid(format_args!(
                "the clip's hips rest {} m up; a clip that moves the hips needs them at least {} m up, to scale their travel to the avatar's",
                hips_rest.y, MIN_HIPS_HEIGHT
            )));
        }
        clip.duration = clip
            .bones
            .iter()
            .map(|bone| bone.rotation.last_time())
            .chain(clip.hips.iter().map(|hips| hips.translation.last_time()))
            .chain(
                clip.expressions
                    .iter()
                    .map(|(_, weight)| weight.last_time()),
            )
            .fold(0.0, f64::max);
        Ok(clip)
    }

    /// The clip at `seconds`, each channel clamped to its keys.
    pub fn sample(&self, seconds: f64) -> ClipPose {
        ClipPose {
            rotations: self
                .bones
                .iter()
                .map(|channel| {
                    let local = channel.rotation.sample(seconds);
                    (channel.bone, channel.before * local * channel.after)
                })
                .collect(),
            hips: self.hips.as_ref().map(|hips| {
                hips.parent
                    .transform_point3(hips.translation.sample(seconds))
            }),
            expressions: self
                .expressions
                .iter()
                .map(|(key, weight)| (key.clone(), weight.sample(seconds)))
                .collect(),
        }
    }

    /// The hips' rest position in the clip's root space (`H₀`).
    pub fn hips_rest(&self) -> Vec3 {
        self.hips_rest
    }
}

fn invalid(message: std::fmt::Arguments) -> LoadError {
    LoadError::Invalid(message.to_string())
}

/// The clip's humanoid bones by node, refused when a bone names a node past
/// the file's or another bone's, or (with any bone) there is no hips. The
/// eyes are left out, as UniVRM's importer leaves them.
fn clip_bones(
    vrma: &VRMCVrmAnimationSchema,
    count: usize,
) -> Result<BTreeMap<HumanBone, usize>, LoadError> {
    let Some(humanoid) = &vrma.humanoid else {
        return Ok(BTreeMap::new());
    };
    let bones: BTreeMap<HumanBone, usize> = humanoid
        .human_bones
        .0
        .iter()
        .filter_map(|(name, bone)| Some((HumanBone::from(*name), bone.as_ref()?.node?.value())))
        .collect();
    let name = |bone: HumanBone| match serde_json::to_value(bone) {
        Ok(Value::String(name)) => name,
        _ => format!("{bone:?}"),
    };
    // By name (the bones' order), so the message does not depend on the
    // file's order.
    for (&bone, &node) in &bones {
        if node >= count {
            return Err(invalid(format_args!(
                "{VRMC_VRM_ANIMATION} humanoid bone {} names node {node}, but the file has {count} nodes",
                name(bone)
            )));
        }
    }
    let mut by_node: Vec<(usize, String)> = bones
        .iter()
        .map(|(&bone, &node)| (node, name(bone)))
        .collect();
    by_node.sort();
    for pair in by_node.windows(2) {
        if let [(node, first), (other, second)] = pair
            && node == other
        {
            return Err(invalid(format_args!(
                "{VRMC_VRM_ANIMATION} humanoid bone {second} names node {node}, which bone {first} names too"
            )));
        }
    }
    if !bones.is_empty() && !bones.contains_key(&HumanBone::Hips) {
        return Err(invalid(format_args!(
            "the {VRMC_VRM_ANIMATION} humanoid has no hips bone, which a VRM animation needs"
        )));
    }
    let eyes = [HumanBone::LeftEye, HumanBone::RightEye];
    Ok(bones
        .into_iter()
        .filter(|(bone, _)| !eyes.contains(bone))
        .collect())
}

/// The clip's expressions and their nodes, presets then custom names, each
/// in key order; a node past the file's or another expression's is refused
/// (an expression is its node's channel). The look expressions
/// are left out: UniVRM's clip has no field for them.
fn clip_expressions(
    vrma: &VRMCVrmAnimationSchema,
    count: usize,
) -> Result<Vec<(ExpressionKey, usize)>, LoadError> {
    let Some(expressions) = &vrma.expressions else {
        return Ok(Vec::new());
    };
    let looks = [
        ExpressionPresetName::LookUp,
        ExpressionPresetName::LookDown,
        ExpressionPresetName::LookLeft,
        ExpressionPresetName::LookRight,
    ];
    let mut named = BTreeMap::new();
    for (preset, expression) in expressions.preset.iter().flat_map(|preset| &preset.0) {
        if looks.contains(preset) {
            continue;
        }
        if let Some(node) = expression.node {
            named.insert(
                ExpressionKey::Preset(Preset::from_spec(*preset)),
                node.value(),
            );
        }
    }
    for (name, expression) in expressions.custom.iter().flatten() {
        if let Some(node) = expression.node {
            named
                .entry(ExpressionKey::parse(name))
                .or_insert(node.value());
        }
    }
    let mut first_on = BTreeMap::new();
    for (key, &node) in &named {
        if node >= count {
            return Err(invalid(format_args!(
                "{VRMC_VRM_ANIMATION} expression {} names node {node}, but the file has {count} nodes",
                key.name()
            )));
        }
        if let Some(first) = first_on.insert(node, key) {
            return Err(invalid(format_args!(
                "{VRMC_VRM_ANIMATION} expression {} names node {node}, which expression {} names too",
                key.name(),
                first.name()
            )));
        }
    }
    Ok(named.into_iter().collect())
}

/// Each node's rest rotation in the root's axes: its parent's times its own,
/// scale left out, as Unity's `Transform.rotation` is.
fn rest_world_rotations(tree: &NodeTree) -> Vec<Quat> {
    let mut rotations = vec![Quat::IDENTITY; tree.parents.len()];
    for &node in &tree.pre_order {
        let local = tree.rest[node].rotation;
        rotations[node] = match tree.parents[node] {
            Some(parent) => rotations[parent] * local,
            None => local,
        };
    }
    rotations
}

/// Reads one channel's sampler, its keys counted toward what is left of the
/// clip's budget.
struct Reader<'a, 'd> {
    accessors: &'a Accessors<'d>,
    sampler: gltf::animation::Sampler<'d>,
    left: &'a mut u64,
}

impl<'d> Reader<'_, 'd> {
    fn at(&self) -> At<'d, 'd> {
        At::Sampler {
            animation: self.sampler.animation().index(),
            sampler: self.sampler.index(),
        }
    }

    /// The sampler's keys: times finite and rising, `N`-float values
    /// (`what`, of `dimensions`) finite, one per time (three for
    /// `CUBICSPLINE`), each made a `T` by `value`.
    fn read<const N: usize, T>(
        &mut self,
        what: &str,
        dimensions: Dimensions,
        value: impl Fn([f32; N]) -> T,
    ) -> Result<Channel<T>, LoadError>
    where
        [f32; N]: gltf::accessor::Item,
    {
        let at = self.at();
        let interpolation = self.sampler.interpolation();
        let (input, output) = (self.sampler.input(), self.sampler.output());
        let keys = input.count();
        let per_key = if interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        };
        if keys.checked_mul(per_key) != Some(output.count()) {
            let each = if per_key == 3 { ", three each" } else { "" };
            return Err(invalid(format_args!(
                "{at} has {} {what} values for {keys} key times{each}",
                output.count()
            )));
        }
        let bytes = (keys as u64)
            .saturating_mul(4)
            .saturating_add((output.count() as u64).saturating_mul(4 * N as u64));
        if bytes > *self.left {
            return Err(invalid(format_args!(
                "{at} has {bytes} bytes of keys, past the {} bytes left of the {} MiB a clip's channels may hold",
                *self.left,
                CLIP_BUDGET >> 20
            )));
        }
        *self.left -= bytes;

        let times = floats::<f32>(
            self.accessors,
            self.at(),
            &input,
            "key times",
            Dimensions::Scalar,
        )?;
        if times.iter().any(|time| !time.is_finite()) {
            return Err(invalid(format_args!(
                "{at} has a key time that is not finite"
            )));
        }
        if let Some(pair) = times.windows(2).find(|pair| pair[1] <= pair[0]) {
            return Err(invalid(format_args!(
                "{at} has key time {} after {}: key times must rise",
                pair[1], pair[0]
            )));
        }
        let values = floats::<[f32; N]>(self.accessors, self.at(), &output, what, dimensions)?;
        if values.iter().flatten().any(|v| !v.is_finite()) {
            return Err(invalid(format_args!(
                "{at} has a {what} that is not finite"
            )));
        }
        Ok(Channel {
            interpolation,
            times: times.to_vec(),
            values: values.iter().copied().map(value).collect(),
        })
    }

    /// `channel`'s rotation keys made unit length (glTF asks for unit
    /// quaternions; files round them); `CUBICSPLINE`'s tangents are left as
    /// they are. A key of length 0 is refused.
    fn rotations(&self, mut channel: Channel<Quat>) -> Result<Channel<Quat>, LoadError> {
        let (start, step) = match channel.interpolation {
            Interpolation::CubicSpline => (1, 3),
            _ => (0, 1),
        };
        for rotation in channel.values.iter_mut().skip(start).step_by(step) {
            let length = rotation.length_squared();
            if length == 0.0 {
                return Err(invalid(format_args!(
                    "{} has a rotation of length 0",
                    self.at()
                )));
            }
            if (length - 1.0).abs() > 1e-5 {
                *rotation = rotation.normalize();
            }
        }
        Ok(channel)
    }
}

/// An avatar's humanoid at rest, which [`Clip`]s are retargeted onto
/// (UniVRM's control rig).
// Only tests use it until the model plays clips.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct Retarget {
    /// By bone order.
    bones: Vec<RetargetBone>,
    hips: usize,
    /// The hips' rest position in model space (`H₀'`).
    hips_rest: Vec3,
}

/// An avatar bone's node and its rest rotations, local (`L₀'`) and world
/// (`G₀'`).
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct RetargetBone {
    bone: HumanBone,
    node: usize,
    rest_local: Quat,
    rest_world: Quat,
}

#[allow(dead_code)]
impl Retarget {
    pub(crate) fn new(tree: &NodeTree, humanoid: &Humanoid) -> Self {
        let rotations = rest_world_rotations(tree);
        let hips = humanoid.required(HumanBone::Hips);
        Self {
            bones: humanoid
                .bones
                .iter()
                .map(|(&bone, &node)| RetargetBone {
                    bone,
                    node,
                    rest_local: tree.rest[node].rotation,
                    rest_world: rotations[node],
                })
                .collect(),
            hips,
            hips_rest: tree.world(&tree.rest_pose())[hips].w_axis.truncate(),
        }
    }

    /// Writes `pose` onto `out`: every humanoid bone's local rotation, and
    /// the hips' local translation for their travel from `source_hips_rest`
    /// (the clip's `H₀`). Other nodes keep `out`'s values, which place the
    /// hips: their parent's current world is read from `out`.
    pub(crate) fn apply(
        &self,
        pose: &ClipPose,
        source_hips_rest: Vec3,
        tree: &NodeTree,
        out: &mut Pose,
    ) {
        for bone in &self.bones {
            let q = pose
                .rotations
                .get(&bone.bone)
                .copied()
                .unwrap_or(Quat::IDENTITY);
            out.local[bone.node].rotation =
                bone.rest_local * (bone.rest_world.inverse() * q * bone.rest_world);
        }

        let world = match pose.hips {
            Some(raw) => {
                self.hips_rest + (raw - source_hips_rest) * (self.hips_rest.y / source_hips_rest.y)
            }
            None => self.hips_rest,
        };
        // The hips' parent's current world, walked down from the root.
        let mut chain = Vec::new();
        let mut node = tree.parents[self.hips];
        while let Some(parent) = node {
            chain.push(parent);
            node = tree.parents[parent];
        }
        let parent = chain.iter().rev().fold(Mat4::IDENTITY, |world, &node| {
            world * out.local[node].matrix()
        });
        let local = parent.inverse().transform_point3(world);
        // A parent scaled to nothing has no inverse: the hips stay put.
        if local.is_finite() {
            out.local[self.hips].translation = local;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::humanoid::Humanoid;
    use crate::nodes::NodeTree;
    use crate::synthetic::{SyntheticVrm, SyntheticVrma, node, vrma_node};
    use serde_json::{Value, json};
    use std::f32::consts::FRAC_PI_2;

    fn clip(vrma: &SyntheticVrma) -> Clip {
        match Clip::load(&vrma.glb()) {
            Ok(clip) => clip,
            Err(error) => panic!("the clip did not load: {error}"),
        }
    }

    fn refusal(bytes: &[u8]) -> String {
        match Clip::load(bytes) {
            Ok(_) => panic!("the clip loaded"),
            Err(error) => error.to_string(),
        }
    }

    fn near(actual: Vec3, expected: Vec3) -> bool {
        (actual - expected).abs().max_element() < 1e-4
    }

    fn same_rotation(actual: Quat, expected: Quat) -> bool {
        actual.dot(expected).abs() > 1.0 - 1e-6
    }

    /// The synthetic avatar's tree and humanoid, without a texture cache.
    fn avatar() -> (NodeTree, Humanoid) {
        let document = Document::parse(&SyntheticVrm::avatar().glb()).unwrap();
        (NodeTree::new(&document).unwrap(), Humanoid::new(&document))
    }

    /// The avatar's world matrices with `clip` retargeted onto it at `seconds`.
    fn posed(clip: &Clip, seconds: f64) -> Vec<Mat4> {
        let (tree, humanoid) = avatar();
        let retarget = Retarget::new(&tree, &humanoid);
        let mut pose = tree.rest_pose();
        retarget.apply(&clip.sample(seconds), clip.hips_rest(), &tree, &mut pose);
        tree.world(&pose)
    }

    fn position(worlds: &[Mat4], node: usize) -> Vec3 {
        worlds[node].w_axis.truncate()
    }

    #[test]
    fn clip_reads_bones_hips_and_expressions() {
        let clip = clip(&SyntheticVrma::clip());
        assert_eq!(clip.duration, 1.0);
        assert!(near(clip.hips_rest(), Vec3::new(0.0, 1.0, 0.0)));
        let half = clip.sample(0.5);
        let arm = half.rotations[&HumanBone::LeftUpperArm];
        assert!(
            same_rotation(arm, Quat::from_rotation_z(-FRAC_PI_2 / 2.0)),
            "{arm}"
        );
        // Bones the clip does not key are not in the pose.
        assert_eq!(half.rotations.len(), 1);
        let hips = half.hips.unwrap();
        assert!(near(hips, Vec3::new(0.0, 1.0, 0.25)), "{hips}");
        let happy = half.expressions[&ExpressionKey::parse("happy")];
        assert!((happy - 0.5).abs() < 1e-6, "{happy}");
        assert_eq!(clip.sample(2.0), clip.sample(1.0));
        assert_eq!(clip.sample(-1.0), clip.sample(0.0));

        // A preset of a camelCase name is that preset, not a custom name.
        let blink = Clip::load(&SyntheticVrma::clip().glb_with(|root| {
            root["extensions"]["VRMC_vrm_animation"]["expressions"]["preset"] =
                json!({ "blinkLeft": { "node": vrma_node::HAPPY } });
        }))
        .unwrap();
        let expressions = blink.sample(1.0).expressions;
        assert_eq!(
            expressions.keys().collect::<Vec<_>>(),
            [&ExpressionKey::Preset(Preset::BlinkLeft)]
        );
    }

    #[test]
    fn retarget_uses_normalized_rotations() {
        // The avatar's upper arm rests at Rz(+90°); the clip's at identity.
        let clip = clip(&SyntheticVrma::clip());
        let lower = position(&posed(&clip, 1.0), node::LEFT_LOWER_ARM);
        // Copying the clip's local rotation would put it near (−0.10, 1.25, 0.45).
        assert!(near(lower, Vec3::new(0.15, 1.0, 0.45)), "{lower}");
        let lower = position(&posed(&clip, 0.5), node::LEFT_LOWER_ARM);
        assert!(
            near(lower, Vec3::new(0.3267767, 1.0732233, 0.225)),
            "{lower}"
        );
    }

    #[test]
    fn retarget_leaves_unkeyed_bones_and_other_nodes_alone() {
        let clip = clip(&SyntheticVrma::clip());
        let (tree, humanoid) = avatar();
        let retarget = Retarget::new(&tree, &humanoid);
        let mut pose = tree.rest_pose();
        // A bone the clip lacks goes to rest; a node no bone is keeps its value.
        pose.local[node::HEAD].rotation = Quat::from_rotation_y(1.0);
        pose.local[node::HAIR0].rotation = Quat::from_rotation_y(1.0);
        retarget.apply(&clip.sample(0.25), clip.hips_rest(), &tree, &mut pose);
        assert!(same_rotation(
            pose.local[node::HEAD].rotation,
            tree.rest[node::HEAD].rotation
        ));
        assert_eq!(pose.local[node::HAIR0].rotation, Quat::from_rotation_y(1.0));
        // The upper arm's rest turn is kept under the clip's.
        let rest = tree.rest[node::LEFT_UPPER_ARM].rotation;
        assert!(!same_rotation(
            pose.local[node::LEFT_UPPER_ARM].rotation,
            rest
        ));
    }

    #[test]
    fn clip_rest_rotations_are_normalized_away() {
        // The clip's upper arm rests at R = Rx(90°), as exporters' bones do;
        // a key of R is its rest, and R·Rz(−90°) turns it −90° about R·ẑ,
        // which is −Y: Ry(+90°).
        let rest = Quat::from_rotation_x(FRAC_PI_2);
        let vrma = SyntheticVrma {
            arm: vec![(0.0, rest), (1.0, rest * Quat::from_rotation_z(-FRAC_PI_2))],
            hips: Vec::new(),
            ..SyntheticVrma::clip()
        };
        let clip = Clip::load(&vrma.glb_with(|root| {
            root["nodes"][vrma_node::LEFT_UPPER_ARM]["rotation"] = json!(rest.to_array());
        }))
        .unwrap();
        let start = clip.sample(0.0).rotations[&HumanBone::LeftUpperArm];
        assert!(same_rotation(start, Quat::IDENTITY), "{start}");
        let end = clip.sample(1.0).rotations[&HumanBone::LeftUpperArm];
        assert!(
            same_rotation(end, Quat::from_rotation_y(FRAC_PI_2)),
            "{end}"
        );
        // On the avatar: at rest at 0 s; at 1 s, Ry(90°) after its own rest
        // turn swings the arm from +X to −Z.
        let lower = position(&posed(&clip, 0.0), node::LEFT_LOWER_ARM);
        assert!(near(lower, Vec3::new(0.40, 1.25, 0.0)), "{lower}");
        let lower = position(&posed(&clip, 1.0), node::LEFT_LOWER_ARM);
        assert!(near(lower, Vec3::new(0.15, 1.25, -0.25)), "{lower}");
    }

    #[test]
    fn hips_travel_scales_by_hips_height() {
        let clip_at = |vrma: &SyntheticVrma| position(&posed(&clip(vrma), 1.0), node::HIPS);
        let hips = clip_at(&SyntheticVrma::clip());
        assert!(near(hips, Vec3::new(0.0, 0.9, 0.45)), "{hips}");
        let h = 0.45;
        let half = SyntheticVrma {
            hips_rest_y: h,
            hips: vec![(0.0, Vec3::new(0.0, h, 0.0)), (1.0, Vec3::new(0.0, h, 0.5))],
            ..SyntheticVrma::clip()
        };
        let hips = clip_at(&half);
        assert!(near(hips, Vec3::new(0.0, 0.9, 1.0)), "{hips}");
        // Without hips keys, the hips rest.
        let still = SyntheticVrma {
            hips: Vec::new(),
            ..SyntheticVrma::clip()
        };
        assert_eq!(clip(&still).sample(0.5).hips, None);
        let hips = clip_at(&still);
        assert!(near(hips, Vec3::new(0.0, 0.9, 0.0)), "{hips}");
    }

    #[test]
    fn step_and_cubic_interpolation() {
        let step = clip(&SyntheticVrma {
            interpolation: "STEP",
            ..SyntheticVrma::clip()
        });
        let arm = step.sample(0.5).rotations[&HumanBone::LeftUpperArm];
        assert!(same_rotation(arm, Quat::IDENTITY), "{arm}");
        let cubic = clip(&SyntheticVrma {
            interpolation: "CUBICSPLINE",
            ..SyntheticVrma::clip()
        });
        let half = cubic.sample(0.5);
        let arm = half.rotations[&HumanBone::LeftUpperArm];
        assert!(
            same_rotation(arm, Quat::from_rotation_z(-FRAC_PI_2 / 2.0)),
            "{arm}"
        );
        // Zero tangents ease in and out: halfway in time is halfway there.
        assert!(near(half.hips.unwrap(), Vec3::new(0.0, 1.0, 0.25)));
        let quarter = cubic.sample(0.25).hips.unwrap();
        assert!(near(quarter, Vec3::new(0.0, 1.0, 0.078125)), "{quarter}");
        assert_eq!(cubic.sample(1.0), cubic.sample(5.0));
    }

    #[test]
    fn not_a_vrma_is_refused() {
        let message = refusal(&SyntheticVrm::avatar().glb());
        assert!(message.contains("not a VRM animation"), "{message}");
        assert!(
            message.contains("no VRMC_vrm_animation extension"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            root["extensions"]["VRMC_vrm_animation"]["specVersion"] = "1.1".into();
        }));
        assert!(
            message.contains("VRMC_vrm_animation specVersion 1.1 is not 1.0"),
            "{message}"
        );
    }

    #[test]
    fn non_finite_and_unordered_key_times_are_refused() {
        let arm = |times: [f32; 2]| SyntheticVrma {
            arm: vec![
                (times[0], Quat::IDENTITY),
                (times[1], Quat::from_rotation_z(-FRAC_PI_2)),
            ],
            ..SyntheticVrma::clip()
        };
        let message = refusal(&arm([0.0, f32::NAN]).glb());
        assert!(
            message.contains("animation 0 sampler 0 has a key time that is not finite"),
            "{message}"
        );
        let message = refusal(&arm([0.0, f32::INFINITY]).glb());
        assert!(message.contains("not finite"), "{message}");
        let message = refusal(&arm([1.0, 0.0]).glb());
        assert!(
            message.contains("animation 0 sampler 0 has key time 0 after 1"),
            "{message}"
        );
        let message = refusal(&arm([0.5, 0.5]).glb());
        assert!(message.contains("key time 0.5 after 0.5"), "{message}");
    }

    #[test]
    fn non_finite_values_are_refused() {
        let message = refusal(
            &SyntheticVrma {
                hips: vec![(0.0, Vec3::new(0.0, 1.0, f32::NAN))],
                ..SyntheticVrma::clip()
            }
            .glb(),
        );
        assert!(
            message.contains("animation 0 sampler 1 has a translation that is not finite"),
            "{message}"
        );
        let message = refusal(
            &SyntheticVrma {
                happy: vec![(0.0, f32::INFINITY)],
                ..SyntheticVrma::clip()
            }
            .glb(),
        );
        assert!(
            message.contains("animation 0 sampler 2 has a translation that is not finite"),
            "{message}"
        );
        let message = refusal(
            &SyntheticVrma {
                arm: vec![(0.0, Quat::from_xyzw(0.0, 0.0, 0.0, 0.0))],
                ..SyntheticVrma::clip()
            }
            .glb(),
        );
        assert!(
            message.contains("animation 0 sampler 0 has a rotation of length 0"),
            "{message}"
        );
    }

    #[test]
    fn samplers_of_the_wrong_shape_are_refused() {
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            // The arm's output, two rotations, read as one.
            let output = root["animations"][0]["samplers"][0]["output"].clone();
            root["accessors"][output.as_u64().unwrap() as usize]["count"] = 1.into();
        }));
        assert!(
            message.contains("animation 0 sampler 0 has 1 rotation values for 2 key times"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            root["animations"][0]["samplers"][0]["interpolation"] = "CUBICSPLINE".into();
        }));
        assert!(
            message.contains("has 2 rotation values for 2 key times, three each"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            // The hips' translation output, VEC3, read for the arm.
            root["animations"][0]["samplers"][0]["output"] =
                root["animations"][0]["samplers"][1]["output"].clone();
        }));
        assert!(
            message.contains("animation 0 sampler 0 has rotation of Vec3"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            let channels = root["animations"][0]["channels"].as_array_mut().unwrap();
            channels.push(channels[0].clone());
        }));
        assert!(
            message.contains("animation 0 has two rotation channels for node 2"),
            "{message}"
        );
    }

    #[test]
    fn humanoids_naming_missing_nodes_are_refused() {
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            root["extensions"]["VRMC_vrm_animation"]["humanoid"]["humanBones"]["spine"]["node"] =
                99.into();
        }));
        assert!(
            message.contains("humanoid bone spine names node 99, but the file has 5 nodes"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            root["extensions"]["VRMC_vrm_animation"]["humanoid"]["humanBones"]["chest"] =
                json!({ "node": vrma_node::SPINE });
        }));
        assert!(
            message.contains("humanoid bone spine names node 1, which bone chest names too"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            root["extensions"]["VRMC_vrm_animation"]["expressions"]["preset"]["happy"]["node"] =
                99.into();
        }));
        assert!(
            message.contains("expression happy names node 99, but the file has 5 nodes"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            root["extensions"]["VRMC_vrm_animation"]["expressions"]["custom"] =
                json!({ "wink": { "node": vrma_node::HAPPY } });
        }));
        assert!(
            message.contains("expression wink names node 4, which expression happy names too"),
            "{message}"
        );
        let message = refusal(&SyntheticVrma::clip().glb_with(|root| {
            let bones = &mut root["extensions"]["VRMC_vrm_animation"]["humanoid"]["humanBones"];
            bones.as_object_mut().unwrap().remove("hips");
        }));
        assert!(message.contains("humanoid has no hips bone"), "{message}");
    }

    #[test]
    fn hips_too_low_to_scale_by_are_refused() {
        let flat = SyntheticVrma {
            hips_rest_y: 0.0,
            ..SyntheticVrma::clip()
        };
        let message = refusal(&flat.glb());
        assert!(
            message.contains("the clip's hips rest 0 m up; a clip that moves the hips"),
            "{message}"
        );
        // Without hips keys there is nothing to scale.
        clip(&SyntheticVrma {
            hips: Vec::new(),
            ..flat
        });
    }

    #[test]
    fn channels_past_their_limit_are_refused() {
        // `extra` custom expressions on nodes of their own, each keyed by
        // happy's sampler: a few JSON bytes each.
        let customs = |extra: usize| {
            SyntheticVrma::clip().glb_with(|root| {
                let first = root["nodes"].as_array().unwrap().len();
                let mut custom = serde_json::Map::new();
                for index in 0..extra {
                    root["nodes"].as_array_mut().unwrap().push(json!({}));
                    custom.insert(format!("custom{index}"), json!({ "node": first + index }));
                    root["animations"][0]["channels"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({
                            "sampler": 2,
                            "target": { "node": first + index, "path": "translation" },
                        }));
                }
                root["extensions"]["VRMC_vrm_animation"]["expressions"]["custom"] =
                    Value::Object(custom);
            })
        };
        // The arm, the hips, and happy, then customs up to the limit.
        let full = Clip::load(&customs(MAX_CHANNELS - 3)).unwrap();
        assert_eq!(full.sample(0.5).expressions.len(), MAX_CHANNELS - 2);
        let message = refusal(&customs(MAX_CHANNELS - 2));
        assert!(
            message.contains(&format!(
                "animation 0 keys more than the {MAX_CHANNELS} bones and expressions"
            )),
            "{message}"
        );
    }

    #[test]
    fn keys_past_the_budget_are_refused() {
        let glb = SyntheticVrma::clip().glb();
        // Two keys each: the arm's 2 × (4 + 16) bytes, the hips' and happy's
        // 2 × (4 + 12) each.
        let needed = 40 + 32 + 32;
        Clip::load_within(&glb, needed).unwrap();
        let message = match Clip::load_within(&glb, needed - 1) {
            Ok(_) => panic!("the clip loaded"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("animation 0 sampler 2 has 32 bytes of keys, past the 31 bytes left"),
            "{message}"
        );
    }

    #[test]
    fn sample_clip_loads() {
        let Some(directory) = std::env::var_os("CELESTA_VRM_SAMPLES_DIR") else {
            println!("skipped: CELESTA_VRM_SAMPLES_DIR is not set");
            return;
        };
        let bytes = std::fs::read(std::path::Path::new(&directory).join("test.vrma")).unwrap();
        let clip = Clip::load(&bytes).unwrap();
        assert_eq!(clip.duration, 3.0);
        // Its bones rest turned, so its rest keys normalise to identity.
        let pose = clip.sample(0.0);
        assert_eq!(pose.hips, None);
        assert!(
            pose.expressions
                .contains_key(&ExpressionKey::parse("happy"))
        );
        for (bone, rotation) in &pose.rotations {
            assert!(rotation.is_finite(), "{bone:?}");
        }
        let worlds = posed(&clip, 1.5);
        assert!(worlds.iter().all(Mat4::is_finite));
    }
}
