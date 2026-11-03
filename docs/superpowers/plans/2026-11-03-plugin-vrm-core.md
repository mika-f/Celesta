# Plugin VRM core (phase 3, part 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A VRM plugin, built apart from Celesta's workspace, opens a VRM 1.0 avatar (`.vrm`) and answers each frame with a scene Celesta draws: the avatar in its humanoid rest pose with MToon and unlit materials, expressions with UniVRM's override rules, eyes following a gaze target, a camera framed from the humanoid, the plugin's reference light, and the model's usage terms as notices. `<Vrm>` puts it in a composition, and the editor's Add menu offers it with a `.vrm` file.

**Architecture:** `plugins/vrm` is the crate `celesta-plugin-vrm` with a `[workspace]` of its own (excluded from the root one), depending on `crates/plugin` by path. `document.rs` reads the glb with `gltf` and the VRM extensions with `vrm-spec`; `nodes.rs` keeps the node tree, the rest pose, and world matrices; `geometry.rs` turns each glTF primitive into a compacted `Geometry` with MikkTSpace tangents; `materials.rs` maps materials and textures; `mtoon.rs` and `mtoon.wgsl` are the plugin's MToon material (UniVRM's MToon10 written against Celesta's material interface); `expressions.rs`, `look_at.rs`, `camera.rs`, `meta.rs` hold UniVRM's runtime rules; `model.rs` runs the per-frame pipeline (pose, constraints, lookAt, expressions, springs: constraints and springs are no-op stages the motion plan fills); `provider.rs` serves it as plugin `vrm`. `js/src/index.ts` is `<Vrm>`. The plugin host's manifest and the editor's Add menu gain a model-file component kind.

**Tech Stack:** Rust 2024 (`gltf` 1.4.1, `vrm-spec` =0.1.1, `glam` 0.34.2, `mikktspace` 0.3.0, `image` 0.25.10, `sha2` 0.11.1, serde, thiserror), TypeScript (React, `@celesta/react`, `@celesta/character`, Vitest), Node build script.

**Spec:** `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md` (Where the pieces live, Libraries for the VRM plugin, The VRM plugin → Loading, Per frame steps 3–4, `<Vrm>`, Usage terms; Checks → Plugin; Phases → 3). The protocol the plugin speaks: `docs/handoff/2026-11-03-plugin-scene-content.md` and `2026-11-03-plugin-scene-materials.md` (its "For plugins" list). Humanoid animation, constraints, and spring bones: `2026-11-03-plugin-vrm-motion.md`, which builds on this plan's types.

## Global Constraints

- The plugin lives at `plugins/vrm`, package `celesta-plugin-vrm`, binary `celesta-plugin-vrm`, library `celesta_plugin_vrm`, plugin id `vrm`; licence `MIT OR Apache-2.0`; edition 2024; `rust-version = "1.89"`; `publish = false`.
- `plugins/vrm/Cargo.toml` has an empty `[workspace]` table and its own committed `Cargo.lock`; the root `Cargo.toml` gains `exclude = ["plugins/vrm"]`; `cargo build --workspace` at the root never builds it.
- Dependencies: `celesta-plugin = { path = "../../crates/plugin" }`; dev: `celesta-gpu-renderer`, `celesta-plugin-host`, `celesta-composition` by path, `tempfile = "3.27.0"`, and the crate itself with feature `synthetic`.
- Every file ported from UniVRM starts with `// Ported from UniVRM (https://github.com/vrm-c/UniVRM), <path>. MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka SUMI for MToon.` naming the source path.
- Scene conventions are the protocol's: right-handed, +Y up, metres; the avatar faces +Z, its left is +X; matrices column-major `f32`; UVs from the top-left; colours linear unless stated.
- UniVRM decides wherever the VRM specification leaves room. Its Unity space is glTF's with X negated; every rule below is already written in glTF's space.
- Model space is the glTF default scene's root space (`document.default_scene()`, else scene 0).
- A frame depends only on the model, `time`, and `data`; nothing iterates a `HashMap` into output (expressions in preset order then custom names in byte order, nodes in pre-order).
- The plugin sends only the protocol's vertex streams: position, normal, `TEXCOORD_0`, tangents where a material has a `normalTexture`, `JOINTS_0`/`WEIGHTS_0` (the skinning pass is at its 8 storage bindings).
- MToon is the plugin's: `src/mtoon.wgsl`, recovered from `crates/gpu-renderer/src/scene_mtoon.wgsl` at the parent of Celesta's commit `refactor(plugins): MToon leaves Celesta for the VRM plugin` (`git show <that commit>^:crates/gpu-renderer/src/scene_mtoon.wgsl`), and `src/mtoon.rs` (`Mtoon` with UniVRM's defaults from that parent's `crates/plugin/src/scene.rs`; `params` from its `scene.rs`'s `mtoon_params` and `uv_scroll`); both carry the UniVRM notice. Its GPU tests are `tests/mtoon.rs`.
- Required humanoid bones (VRM 1.0): `hips spine head leftUpperArm leftLowerArm leftHand rightUpperArm rightLowerArm rightHand leftUpperLeg leftLowerLeg leftFoot rightUpperLeg rightLowerLeg rightFoot`.
- Canvas: the layer's size in px is a resolution the composition sets — `<Vrm width height>` (`aspect` fills whichever one is missing), default 1920×1080 — and travels as `data.canvas`; the plugin answers it in the frame's `canvas` (Task 13), which overrides the model's `canvas` (`CANVAS` = `[1920.0, 1080.0]`, the default). Sharpness does not depend on it: Celesta draws scene canvases at the layer's density.
- Decoded textures (width × height × 4 over distinct images) at most 1 GiB (`TEXTURE_BUDGET = 1 << 30`).
- Reference lighting (Unity: directional light rotation (50°, 150°, 0°), colour sRGB (255, 244, 214), intensity 1, ambient mode Color sRGB (54, 58, 66), shadows off), in Celesta's terms: `direction [-0.3213938, -0.7660444, -0.5566704]`, `color [1.0, 0.9046612, 0.6724432]`, `intensity 1.0`, `ambient[3] = [0.0368895, 0.0423114, 0.0544803]`, other coefficients 0.
- Camera presets: perspective `yfov` 0.5235988 (30°), `near` 0.01, `far` 100.
- Plugin checks run with `cargo test --manifest-path plugins/vrm/Cargo.toml`; GPU tests return early without an adapter unless `CELESTA_REQUIRE_GPU=1`, so red/green and full runs of GPU tests use `env CELESTA_REQUIRE_GPU=1`.
- Sample models are never committed. Tests that need them read `CELESTA_VRM_SAMPLES_DIR` (a folder holding `Seed-san.vrm`, `VRM1_Constraint_Twist_Sample.vrm`, `VRMC_materials_mtoon_UV_Animation_Test.vrm`, `test.vrma`) and print `skipped: CELESTA_VRM_SAMPLES_DIR is not set` and pass without it. Every behaviour also has a committed test on a synthetic VRM built in code.
- Each task ends with its tests, `cargo clippy --manifest-path plugins/vrm/Cargo.toml --all-targets --locked -- -D warnings`, `cargo fmt --manifest-path plugins/vrm/Cargo.toml --check`, and, when it touches root crates, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`; then its own commit. Never amend; tests and commits are separate commands.

## Review Focus

- **A VRM 0.x file** (most avatars in the wild): refused on open with the message naming VRM 0.x and the two converters, never a parse error about a missing extension. Task 1.
- **Primitives sharing one vertex accessor and zero-weight joint slots naming joints the skin lacks** (UniVRM and VRoid exports do both): each geometry carries only its own vertices, and those slots are zeroed, so Celesta does not refuse the model or upload copies. Task 2.
- **A smile closing the eyes twice**: `happy` with `overrideBlink: block` turns the blink expression off; `blend` scales it by `1 − w`; a binary expression snaps at 0.5 as UniVRM's `Mathf.Round` does. Task 5.
- **Transparent hair sorting against the face**: every primitive of one glTF mesh gets the same `center`, following the pose (the skin's skeleton node, or the mesh node), and draws keep primitive order. Task 4.
- **Usage terms**: an avatar whose terms forbid commercial use or require credit opens with a `warning` notice, never a refusal; every other meta field is an `info` notice. Task 8.
- **MToon parity survives the move:** every MToon test Celesta had passes in `tests/mtoon.rs` with the same expected pixels. Task 3b.

---

### Task 1: The crate, its build wiring, the synthetic VRM, and loading

**Files:**
- Create: `plugins/vrm/Cargo.toml`, `plugins/vrm/Cargo.lock`, `plugins/vrm/src/lib.rs`, `src/main.rs`, `src/provider.rs` (stub), `src/document.rs`, `src/synthetic.rs`, `plugins/vrm/README.md` (what it is, how to build, the samples variable)
- Modify: `Cargo.toml` (root: `exclude`), `.gitignore` (`/plugins/vrm/target`, `/plugins/vrm/dist`, `/plugins/vrm/assets`), `.github/workflows/ci.yml` (job `vrm-plugin`)

**Interfaces — Produces:**

```rust
pub const PLUGIN: &str = "vrm";
#[derive(Debug, thiserror::Error)] pub enum LoadError { #[error("{0}")] Invalid(String), #[error(transparent)] Gltf(#[from] gltf::Error), #[error(transparent)] Io(#[from] std::io::Error) }
pub(crate) struct Document {
    pub gltf: gltf::Document, pub buffers: Vec<gltf::buffer::Data>,
    pub vrm: vrm_spec::vrmc_vrm_1_0::VRMCVrmSchema,
    pub mtoon: Vec<Option<vrm_spec::vrmc_materials_mtoon_1_0::VrmcMaterialsMtoonSchema>>, // by material
    pub raw: serde_json::Value,   // the root JSON, for extensions vrm-spec lacks (motion plan)
}
impl Document { pub(crate) fn parse(bytes: &[u8]) -> Result<Self, LoadError> }
// synthetic.rs, #[cfg(any(test, feature = "synthetic"))]
pub struct SyntheticVrm { pub vrm0: bool, pub spec_version: String, pub omit_bone: Option<String>,
    pub normals: bool, pub flat_uvs: bool, pub slot1_weight: f32 /* Body box vertices' JOINTS_0 slot 1 (joint 99) weight, default 0 */, pub meta: Value, pub expressions: Value, pub look_at: Value,
    pub images: Vec<(String /* mime */, Vec<u8>)>, pub extensions: Map<String, Value>,
    pub node_extensions: Vec<(usize, Value)> }
impl SyntheticVrm { pub fn avatar() -> Self; pub fn glb(&self) -> Vec<u8> }
pub mod node { pub const ROOT: usize = 0; pub const HIPS: usize = 1; /* … */ pub const BODY: usize = 23; pub const FIRST_PERSON_ONLY: usize = 24; }
```

**Decisions:**
- `gltf = { version = "1.4.1", default-features = false, features = ["import", "utils", "names", "extensions", "extras", "KHR_texture_transform", "KHR_materials_unlit", "KHR_materials_emissive_strength"] }`; `vrm-spec = "=0.1.1"` (early, so pinned); `image = { version = "0.25.10", default-features = false, features = ["png", "jpeg"] }`. `[features] synthetic = []`.
- `Document::parse`: `gltf::Gltf::from_slice`, then `gltf::import_buffers(&doc, None, blob)`; a buffer with a non-`data:` URI is refused (`"buffer {i} is an external file; a .vrm keeps everything in itself"`). Images are not decoded here.
- Refusals, in this order, each `LoadError::Invalid`:
  - root `extensions.VRM` without `VRMC_vrm`: `"this is a VRM 0.x avatar, which the VRM plugin does not open yet; convert it to VRM 1.0 with UniVRM or the VRM Add-on for Blender"`;
  - no `VRMC_vrm`: `"this glTF file has no VRMC_vrm extension: it is not a VRM 1.0 avatar"`;
  - `specVersion` not `"1.0"` or starting `"1.0-"`: `"VRMC_vrm specVersion {v} is not 1.0"`;
  - a required bone missing: `"the humanoid has no {bone} bone, which VRM 1.0 requires"`; a bone node out of range or two bones on one node: `"humanoid bone {bone} names node {n}, …"` with the reason.
- **Synthetic avatar** (the fixture every later test uses; node indices are pre-order from `Root`): `0 Root`; `1 hips (0, 0.9, 0)`, `2 spine (0, 1.0, 0)`, `3 chest (0, 1.15, 0)`, `4 neck (0, 1.3, 0)`, `5 head (0, 1.4, 0)`, `6 leftEye (0.03, 1.45, 0.05)`, `7 rightEye (−0.03, 1.45, 0.05)`, `8 hair0 (0, 1.55, −0.05)`, `9 hair1 (0, 1.45, −0.1)`, `10 hair2 (0, 1.35, −0.12)`, `11 leftUpperArm (0.15, 1.25, 0)` with rest rotation Rz(+90°) (a VRoid-style non-identity rest), `12 leftLowerArm (0.40, 1.25, 0)`, `13 leftHand (0.62, 1.25, 0)`, `14–16` the right arm mirrored in x with identity rotations, `17 leftUpperLeg (0.1, 0.85, 0)`, `18 leftLowerLeg (0.1, 0.45, 0)`, `19 leftFoot (0.1, 0.05, 0)`, `20–22` the right leg mirrored, `23 Body` (mesh 0, skin 0), `24 FirstPersonOnly` (mesh 1, translation (0, 1.45, 0.1)) (world positions given; locals are their differences, rotated into the arm's frame below `leftUpperArm`). Skin 0: joints `[hips, head, spine]`, inverse binds the inverse rest worlds, `skeleton` hips. Mesh 0 "Body": one POSITION/NORMAL/TEXCOORD_0/JOINTS_0/WEIGHTS_0 accessor set of 12 vertices shared by primitive 0 (a box (−0.2, 0, −0.1)–(0.2, 1.6, 0.1), vertices 0–7, material 0, every vertex joint 0 weight 1 and slot 1 joint 99 weight 0) and primitive 1 (a quad at z −0.1, y 1.3–1.6, facing −Z, vertices 8–11, material 1, joint 1); three morph targets (0: the top four box vertices +0.1 y; 1 and 2: +0.05 x on the same vertices); `weights [0, 0.25, 0]`. Mesh 1: a quad, material 2, unskinned. Materials: 0 "Body" MToon opaque, `baseColorTexture` texture 0 with `KHR_texture_transform` offset (0.5, 0), `normalTexture` texture 1, MToon `shadeMultiplyTexture` texture 0 and `shadingShiftTexture` texture 2; 1 "Hair" MToon blend, `renderQueueOffsetNumber −1`; 2 "Pbr" plain glTF, base colour (0.5, 0.5, 0.5, 1); 3 "Unlit" `KHR_materials_unlit`, base colour (0, 1, 0, 1). Textures: 0 = image 0 + sampler 0, 1 = image 1 + sampler 0, 2 = image 0 + sampler 1 (`wrapS 33071`, `wrapT 33648`, `magFilter 9728`, `minFilter 9728`). Images: a 2×2 PNG (top-left red, top-right green, bottom-left blue, bottom-right white) and a 1×1 PNG (128, 128, 255). Meta: name `Synthetic`, authors `["Celesta"]`, licence `https://vrm.dev/licenses/1.0/`, `avatarPermission everyone`, `commercialUsage corporation`, `creditNotation unnecessary`, every `allow…` false, `modification prohibited`. Expressions: `happy` (morph node 23 target 0 weight 1; `color` bind on material 0 to (1, 0, 0, 1); `emissionColor` bind to (1, 1, 0, 1); `overrideBlink block`, `overrideMouth blend`, `overrideLookAt none`), `blink` (target 0 weight 0.5), `aa` (`textureTransformBind` material 0 offset (0.25, 0) scale (1, 1)), `lookLeft` (target 2 weight 1), custom `wink` (`isBinary`, target 0 weight 1). LookAt: `bone`, `offsetFromHeadBone [0, 0.06, 0]`, inner (90, 10), outer (90, 20), down (90, 12), up (90, 10). First person: node 24 `firstPersonOnly`. `images`, `meta`, `expressions`, `lookAt` are public fields so tests vary them.
- **CI** (`vrm-plugin`, `ubuntu-latest`, 90 minutes, bash): checkout; `apt-get install libfontconfig-dev mesa-vulkan-drivers`; `./.github/actions/setup-ffmpeg` (the renderer dev-dependency links FFmpeg); the toolchain step of `test` (1.99.1) with `components: clippy, rustfmt`; `Swatinem/rust-cache@v2.9.3` with `workspaces: plugins/vrm -> target`, `env-vars: FFMPEG_VERSION FFMPEG_FEATURES`, `save-if` main; then `cargo fmt --manifest-path plugins/vrm/Cargo.toml --check`, `cargo clippy --manifest-path plugins/vrm/Cargo.toml --all-targets --locked -- -D warnings`, `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/vrm/Cargo.toml --locked`.
- `main.rs` serves `VrmProvider` as the turntable's does. Task 1 creates `src/provider.rs` with `#[derive(Default)] pub struct VrmProvider` whose `open` returns `PluginError::Failed` "not ready" and whose `frame` returns the same; Task 8 fills it.

- [ ] **Step 1:** Tests in `document.rs`:
  - `synthetic_avatar_parses`: `Document::parse(&SyntheticVrm::avatar().glb())` succeeds; 25 nodes; `vrm.humanoid` has 19 bones; `vrm.meta.name == "Synthetic"`.
  - `vrm0_is_refused_with_the_conversion_hint` (Review Focus): `vrm0: true` → message contains `"VRM 0.x"`, `"UniVRM"`, and `"VRM Add-on for Blender"`.
  - `plain_gltf_is_refused`: no VRM extension → `"no VRMC_vrm extension"`.
  - `spec_version_other_than_1_0_is_refused`: `"0.9"` → `"specVersion 0.9"`; `"1.0-beta"` parses.
  - `missing_required_bone_is_refused`: `omit_bone: Some("leftFoot")` → `"no leftFoot bone"`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml document` → fails to compile (no crate).
- [ ] **Step 3:** Implement; `cargo generate-lockfile --manifest-path plugins/vrm/Cargo.toml`; `cargo build --workspace` at the root still builds without the plugin.
- [ ] **Step 4:** The tests pass; then the task checks (plugin and root clippy/fmt).
- [ ] **Step 5:** Commit `feat(plugins): VRM plugin crate that opens VRM 1.0 files`.

### Task 2: Node tree, rest pose, humanoid, and geometry

**Files:**
- Create: `plugins/vrm/src/nodes.rs`, `src/humanoid.rs`, `src/geometry.rs`

**Interfaces — Produces** (consumed by the motion plan):

```rust
#[derive(Clone, Copy, Debug, PartialEq)] pub struct Trs { pub translation: Vec3, pub rotation: Quat, pub scale: Vec3 }
impl Trs { pub fn matrix(&self) -> Mat4 }
pub struct NodeTree { pub parents: Vec<Option<usize>>, pub children: Vec<Vec<usize>>, pub pre_order: Vec<usize>, pub rest: Vec<Trs> }
#[derive(Clone, Debug, PartialEq)] pub struct Pose { pub local: Vec<Trs> }
impl NodeTree { pub fn rest_pose(&self) -> Pose; pub fn world(&self, pose: &Pose) -> Vec<Mat4> }   // pre-order, parent × local
pub struct Humanoid { pub bones: BTreeMap<HumanBone, usize> }       // HumanBone = vrm_spec::vrmc_vrm_1_0::HumanBoneName
impl Humanoid { pub fn node(&self, bone: HumanBone) -> Option<usize>; pub fn required(&self, bone: HumanBone) -> usize }
pub(crate) struct PrimitiveGeometry { pub geometry: celesta_plugin::Geometry, pub mesh: usize, pub primitive: usize }
pub(crate) fn primitive_geometry(doc: &Document, mesh: gltf::Mesh, primitive: gltf::Primitive, skin_joints: Option<usize>, needs_tangents: bool) -> Result<Option<PrimitiveGeometry>, LoadError>   // None: not triangles
```

**Decisions:**
- A node's `matrix` is decomposed into `Trs` (`gltf::scene::Transform::decomposed`). Nodes outside the default scene are kept in the tree but never drawn.
- **Compaction:** a primitive keeps the vertices its indices reference, in ascending original index; indices are remapped; every stream and morph target is compacted the same way. A primitive without indices gets `0..count`. `TRIANGLE_STRIP` and `TRIANGLE_FAN` become lists (glTF's winding rules); `POINTS`, `LINES`, `LINE_LOOP`, `LINE_STRIP` return `None` and Task 8 notes them.
- **Streams:** missing `NORMAL` → area-weighted vertex normals over the compacted triangles (Unity's `RecalculateNormals` on an unwelded mesh); missing `TEXCOORD_0` → zeros; `WEIGHTS_0` read as `f32` and normalised to sum 1 where the sum is positive; `JOINTS_1`, `WEIGHTS_1`, `COLOR_0`, and other UV sets are not read. Morph targets are dense position deltas and, where the target has `NORMAL`, normal deltas (sparse accessors through `gltf`'s reader); tangent deltas are not read.
- **Joint slots:** a slot with weight 0 gets joint 0. A slot with weight > 0 naming a joint ≥ the skin's joint count is `LoadError::Invalid`: `"mesh \"{name}\" primitive {p} names joint {j} of skin {s}, which has {n}"`.
- **Tangents** are MikkTSpace on the compacted triangles whenever `needs_tangents` (the material has a `normalTexture`), never read from the file (UniVRM ignores them), else `None`. A vertex takes the last tangent MikkTSpace writes for it; a non-finite result, or a primitive with flat UVs, gets `[1, 0, 0, 1]`.

- [ ] **Step 1:** Tests:
  - `nodes.rs` `rest_world_matches_the_avatar_positions`: `world(rest_pose)` of `leftHand` translates to (0.62, 1.25, 0) and of `head` to (0, 1.4, 0), within 1e-5; `pre_order` is `0..25`.
  - `humanoid.rs` `humanoid_maps_bones_to_nodes`: `required(Hips) == 1`, `node(LeftEye) == Some(6)`, `node(Jaw) == None`.
  - `geometry.rs` `shared_accessors_are_compacted` (Review Focus): Body primitive 0 has 8 positions, primitive 1 has 4, its indices all < 4; each primitive's three morph targets have its own vertex count.
  - `zero_weight_joint_slots_are_zeroed` (Review Focus): every primitive 0 vertex has `joints[1] == 0` and `weights[1] == 0.0`; with `slot1_weight: 0.5` → message contains `"names joint 99 of skin 0, which has 3"`.
  - `tangents_only_where_a_normal_texture_is_used`: primitive 0 has tangents, all finite, `|xyz| = 1 ± 1e-4`, `w = ±1`; primitive 1 has none.
  - `flat_uvs_give_finite_tangents`: `flat_uvs: true` → every tangent is `[1, 0, 0, 1]`.
  - `missing_normals_are_computed`: `normals: false` → primitive 1's normals are all (0, 0, −1) within 1e-6.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml -- nodes humanoid geometry` → fails to compile.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM node tree, humanoid, and compacted geometry`.

### Task 3: Textures and materials (redone after MToon moved into the plugin)

The first version of this task mapped MToon onto Celesta's built-in MToon material. Celesta no longer has one (its materials are the built-in `unlit` and a plugin's own WGSL), so the task is redone on top as a new commit: a VRM MToon material becomes a custom material on the plugin's MToon shader (Task 3b).

**Files:**
- Create: `plugins/vrm/src/materials.rs`

**Interfaces — Produces:**

```rust
pub(crate) struct Materials { pub textures: Vec<SceneTexture>, pub materials: Vec<Material>, pub notices: Vec<Notice>,
    pub base_transforms: Vec<Option<TextureTransform>> /* by material: the base colour texture's, for UV binds */,
    pub mtoon: Vec<Option<Mtoon>> /* by material: the typed parameters packed into params */,
    pub shaders: Vec<SceneShader> }
pub(crate) fn materials(doc: &Document, cache: &Path) -> Result<Materials, LoadError>
pub(crate) fn texture_cache_dir() -> PathBuf   // $CELESTA_VRM_TEXTURE_CACHE, else temp_dir()/celesta-plugin-vrm
```

**Decisions:**
- **Kinds.** A material with `VRMC_materials_mtoon` is an `Mtoon` (Task 3b), every field the file has copied onto `Mtoon::default()` (UniVRM's defaults already): base colour from `pbrMetallicRoughness.baseColorFactor`; `emissive_factor` = `emissiveFactor` × `KHR_materials_emissive_strength` (UniGLTF's `ImportLinearEmissiveFactor`); `normalTexture.scale` and `shadingShiftTexture.scale` into the scaled slots. It is sent as `MaterialKind::Custom(CustomMaterial { shader: 0, params: mtoon::params(&m, &MtoonOverrides::default(), 0.0, &textures).to_vec(), textures: mtoon::slots(&m).map(…).to_vec() /* 9 slots */, outline: (m.outline_width_mode != OutlineWidthMode::None).then(OutlinePass::default), requires_tangents: m.normal_texture.is_some() })`, and kept typed in `Materials::mtoon`. `shaders` is `vec![mtoon::shader()]` when any material is MToon, else empty. `KHR_materials_unlit` is `Unlit` (base colour × texture). Any other material is `Unlit` with its base colour and base colour texture: **glTF PBR is drawn unlit for now** (spec phase 5), its emission and metal-roughness dropped, and one `info` notice says `"{n} glTF PBR material(s) are drawn as unlit base colour: {names}"`.
- The plugin checks what Celesta can no longer see inside the parameter bytes before packing: `mtoon::invalid(&m, textures.len()) -> Option<String>` (slot ranges by MToon name, every number finite) → `LoadError::Invalid("material \"{name}\": {reason}")`.
- `alphaMode` and `alphaCutoff` (default 0.5) and `doubleSided` as the file says. Blend is `AlphaMode::Blend { depth_write: transparentWithZWrite, queue_offset: renderQueueOffsetNumber }`; opaque and mask drop both (UniVRM ignores them there). Materials keep glTF's indices (unused ones too), so expression binds index them directly.
- **Texture transform:** the base colour texture's `KHR_texture_transform` (offset, rotation, scale; Celesta honours the rotation UniVRM drops) becomes `TextureRef::transform`; every other slot's transform is ignored (UniVRM has one `_MainTex_ST`). Every slot samples `TEXCOORD_0`, whatever its `texCoord`.
- **Colour spaces:** sRGB for base colour, shade multiply, emissive, matcap, rim multiply; linear for normal, shading shift, outline width, UV animation mask.
- **One `SceneTexture` per (image, colour space, sampler)**, in first-use order over materials then slots in `Mtoon` field order.
- **Samplers:** wrap 33071 clamp, 33648 mirrored repeat, 10497 repeat; `magFilter` 9728 nearest, else linear; `minFilter` 9728/9984/9986 nearest, else linear; `mipmaps` false only for `minFilter` 9728 or 9729; no sampler → `Sampler::default()`.
- **Images are files, not pixels.** Each `image/png`, `image/jpeg`, or (`EXT_texture_webp`) `image/webp` image's bytes are written once to `<cache>/<sha256 hex>.<png|jpg|webp>` (written to a temporary name and renamed, skipped when the file exists) and sent as `TextureSource::Path`; Celesta decodes them. This keeps the `open` answer to a few MB.
- **Budget:** each distinct image's dimensions come from its header (`image::ImageReader::with_guessed_format().into_dimensions()`); over `TEXTURE_BUDGET` in total → `"the model's textures decode to {m} MiB, more than 1024 MiB"`.
- Another image type (KTX2) leaves the slot empty with a `warning` notice `"texture {t} is {mime}, which the VRM plugin cannot read; material \"{name}\" draws without it"`.

- [ ] **Step 1:** Tests (`materials.rs`, synthetic avatar, cache in a `tempfile::TempDir`):
  - `mtoon_slots_dedupe_by_image_colour_space_and_sampler`: 3 textures: (image 0, sRGB, default sampler), (image 1, linear, default), (image 0, linear, sampler 1); `materials.mtoon[0]`'s `base_color_texture.texture == 0`, `shade_multiply_texture == Some(TextureSlot { texture: 0 })`, `normal_texture.texture == 1`, `shading_shift_texture.texture == 2`; the custom material's `textures == [Some(0), Some(0), Some(2), Some(1), None, None, None, None, None]`.
  - `base_colour_transform_moves_the_material`: material 0's transform offset `[0.5, 0.0]`, scale `[1.0, 1.0]`, rotation 0; `base_transforms[0]` the same.
  - `hair_keeps_blend_and_its_queue_offset`: material 1 `AlphaMode::Blend { depth_write: false, queue_offset: -1 }`.
  - `mtoon_materials_share_one_shader`: materials 0 and 1 both `shader: 0`, one entry in `shaders`, `requires_tangents` only on material 0.
  - `pbr_is_drawn_unlit_with_a_notice`: material 2 is `Unlit` with base `[0.5, 0.5, 0.5, 1.0]`; one `info` notice containing `"drawn as unlit base colour"` and `"Pbr"`; material 3 (`KHR_materials_unlit`) is `Unlit` `[0, 1, 0, 1]` and not in the notice.
  - `sampler_follows_gltf`: texture 2's sampler is `{ClampToEdge, MirroredRepeat, Nearest, Nearest, mipmaps: false}`.
  - `images_are_written_once_by_content`: texture 0's source is a `Path` inside the cache named `<sha256>.png` holding the embedded bytes; loading twice writes no new file (the directory has 2 files).
  - `texture_budget_is_refused`: `images[0]` replaced by a PNG holding only signature, an IHDR of 32768×32768, and IEND → message contains `"4096 MiB, more than 1024 MiB"`.
  - `unreadable_images_leave_the_slot_empty`: `images[1]` mime `image/ktx2` → material 0 has no `normal_texture`; one `warning` notice naming `"image/ktx2"` and `"Body"`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml materials` → fails to compile.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM materials and textures` (the redone version is a new commit on top, after Task 3b; the earlier commits are not rewritten).

### Task 3b: MToon as the plugin's material (after Task 3)

**Files:**
- Create: `plugins/vrm/src/mtoon.rs`, `plugins/vrm/src/mtoon.wgsl`, `plugins/vrm/tests/mtoon.rs`

**Interfaces — Produces:**

```rust
pub struct Mtoon { /* the former MtoonMaterial's fields and UniVRM defaults, minus transparent_with_z_write and render_queue_offset_number */ }
pub enum OutlineWidthMode { None, WorldCoordinates, ScreenCoordinates }
pub struct TextureSlot { pub texture: usize }   pub struct ScaledTextureSlot { pub texture: usize, pub scale: f32 }
#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct MtoonOverrides {   // one frame's binds
    pub base_color: Option<[f32; 4]>, pub texture_offset: Option<[f32; 2]>, pub texture_scale: Option<[f32; 2]>,
    pub shade_color: Option<[f32; 3]>, pub emission_color: Option<[f32; 3]>, pub matcap_color: Option<[f32; 3]>,
    pub rim_color: Option<[f32; 3]>, pub outline_color: Option<[f32; 3]>,
}
pub const MTOON_PARAMS: usize = 176;            // the Params layout of Celesta's former MToon shader
pub fn shader() -> SceneShader;                 // name "MToon", 9 slots: base colour, shade multiply, shading shift (black), normal, emissive, matcap (black), rim multiply, outline width, UV animation mask
pub fn slots(m: &Mtoon) -> [Option<usize>; 9];
pub fn params(m: &Mtoon, o: &MtoonOverrides, time: f64, textures: &[SceneTexture]) -> [u8; MTOON_PARAMS];   // little-endian; the scroll formed in f64 and wrapped as uv_scroll did
pub(crate) fn invalid(m: &Mtoon, textures: usize) -> Option<String>;
```

**Decisions:**
- `mtoon.wgsl` is Celesta's former `scene_mtoon.wgsl` (UniVRM's MToon10) written against Celesta's material interface: `material_fragment`, `material_outline_vertex`, and `material_outline_fragment`, with the `Params` block above and the nine slots in `shader()`'s order. Both files carry the UniVRM notice.
- Every MToon test Celesta had moves here with the same expected pixels, drawn through Celesta's renderer as a plugin's custom material.

- [ ] **Step 1:** Tests:
  - `tests/mtoon.rs` ports the `lit_`, `shading_`, `surface_`, `outline_` (MToon ones), `override_` (the five colours and base colour), `queue_` MToon cases, and `mask_mtoon_covers_by_cutoff` tests with their expected pixels, drawing through `GpuRenderer::with_content_provider(FixedScene { model, frame })` (a test `ContentProvider` answering one fixed `SceneModel` and `SceneFrame`) and the turntable's adapter-skip pattern.
  - `mtoon.rs` unit tests port `mtoon_slots_and_numbers_are_checked` (as `invalid`), the UV scroll precision and wrapping cases (as `params`), and `mtoon_params_pack_univrm_defaults` (default `Mtoon` → shade `(1, 1, 1, −0.05)` at byte 48, toony 0.95 at 92, GI equalization 0.9 at 120, fresnel power 5 at 112).
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/vrm/Cargo.toml mtoon` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): MToon as the VRM plugin's own scene material`.

### Task 4: The model, skins, draws, and a rest frame

**Files:**
- Create: `plugins/vrm/src/model.rs`, `src/first_person.rs`
- Modify: `src/lib.rs` (exports)

**Interfaces — Produces** (the motion plan fills the marked stages):

```rust
pub struct VrmModel { /* tree, humanoid, document data, scene: SceneModel, notices, draws: Vec<DrawSource> */ }
pub(crate) struct DrawSource { pub node: usize, pub mesh: usize, pub primitive: usize, pub skin: Option<usize>, pub visible: bool }
impl VrmModel {
    pub fn load(path: &Path) -> Result<Self, LoadError>;                       // texture_cache_dir()
    pub fn load_slice(bytes: &[u8], cache: &Path) -> Result<Self, LoadError>;
    pub fn scene_model(&self) -> &SceneModel;
    pub fn notices(&self) -> &[Notice];
    pub fn tree(&self) -> &NodeTree; pub fn humanoid(&self) -> &Humanoid;
    pub fn frame(&mut self, time: f64, data: &PerformanceData) -> Result<SceneFrame, FrameError>;
    // the pipeline, in UniVRM's order (Vrm10Runtime.Process):
    fn base_pose(&self, time: f64, data: &PerformanceData) -> Pose;            // here: rest; motion plan: motions and tracks
    fn apply_constraints(&self, pose: &mut Pose);                              // here: nothing; motion plan
    fn expression_inputs(&self, time: f64, data: &PerformanceData) -> BTreeMap<ExpressionKey, f32>; // here: data.expressions; motion plan: + clips
    fn apply_springs(&mut self, time: f64, data: &PerformanceData, pose: &mut Pose); // here: nothing; motion plan
    pub(crate) fn mesh_centers(&self, worlds: &[Mat4]) -> Vec<Vec3>;            // by draw
}
#[derive(Debug, thiserror::Error)] #[error("{0}")] pub struct FrameError(pub String);
pub const CANVAS: [f32; 2] = [1920.0, 1080.0];
```

`frame` runs: `base_pose` → `apply_constraints` → lookAt yaw/pitch (Task 6) → expressions (Task 5, which applies the eye rotations) → `apply_springs` → worlds → `SceneFrame`. Until Task 7 the camera is a fixed `view = translation(0, −0.8, −3.3)` with Task 7's projection, and the lights are `Lights::default()`.

**Changed with MToon in the plugin:** `frame` packs each MToon draw's `params` with `mtoon::params(…, time, …)` and the frame's binds, and sends `DrawState.params` only when the bytes differ from the material's (UV animation or binds); unlit draws get `MaterialOverrides` (base colour, texture offset and scale) only. `overrides_are_finite` checks the plugin's `MtoonOverrides` before packing (`FrameError("material {m}'s {bind} is not finite")`), since Celesta cannot.

**Decisions:**
- **Draws** go node by node in pre-order over the default scene, then primitive by primitive; `DrawInfo { geometry, material, skin }` with one geometry per (mesh, primitive), shared by nodes instancing the mesh. A draw without a material uses a default unlit white material appended once.
- **Skins:** `SceneModel::skins` one per glTF skin (`joints` = its joint count); a frame's `skins[s][j] = world(joint j) × inverseBind(j)` (identity when the skin has no inverse binds). A skinned draw's `world` is the identity (glTF ignores the mesh node's transform); an unskinned draw's is its node's world.
- **`center`, every draw, ruling on the transparent-sort hazard:** Unity sorts a renderer by its bounds' centre, and UniGLTF's `NodeImporter` makes the skin's `skeleton` the renderer's `rootBone`, whose space Unity's skinned bounds are in. So a draw's `center` is `M × c`, with `c` the centre of the union of the mesh's primitives' `POSITION` min/max (mesh space) and `M` the current world of the skin's `skeleton` node, else of the mesh node. All primitives of a node's mesh share it; ties keep draw (primitive) order. The offset this gives when `skeleton` is not at the mesh origin is UniVRM's too; recorded for the parity checks.
- **Morph weights** per draw: the node's `weights`, else the mesh's, else zeros; Task 5 replaces those targets an expression binds.
- **First person, third-person rule:** a node annotated `firstPersonOnly` draws `visible: false`; `thirdPersonOnly`, `both`, `auto`, and unannotated nodes are visible.

- [ ] **Step 1:** Tests (`model.rs`):
  - `draws_follow_node_and_primitive_order`: three draws, `(23, 0, skin 0)`, `(23, 1, skin 0)`, `(24, 0, none)`; geometries 0 and 1 for Body, 2 for the quad.
  - `rest_frame_skins_to_the_identity`: `frame(0.0, &default)` has `skins[0]` of 3 matrices each the identity within 1e-5; draws 0 and 1 have identity `world`; draw 2's `world` translates by (0, 1.45, 0.1).
  - `one_center_per_mesh_following_its_skeleton` (Review Focus): at rest draws 0 and 1 both have `center` (0, 1.7, 0) (hips (0, 0.9, 0) × mesh centre (0, 0.8, 0)); `mesh_centers` with hips moved to (1, 0.9, 0) gives (1, 1.7, 0) for both; draw 2's is its node's world × its quad's centre.
  - `first_person_only_meshes_are_hidden`: draw 2 is `visible: false`.
  - `morph_weights_default_to_the_mesh_weights`: draws 0 and 1 have `morph_weights == [0.0, 0.25, 0.0]` with no expressions.
  - `frames_repeat_exactly`: two `frame(1.5, …)` calls are equal.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml model` → fails to compile.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM skins, draws, and rest frames`.

### Task 5: Expressions with UniVRM's overrides and binds

**Files:**
- Create: `plugins/vrm/src/expressions.rs` (ported: `Vrm10RuntimeExpression.cs`, `Expression/DefaultExpressionValidator.cs`, `ExpressionMerger.cs`, `MorphTargetBindingMerger/MorphTargetBindingMerger.cs`, `MaterialValueBindingMerger.cs`, under `Packages/VRM10/Runtime/Components/`; `Packages/VRM10/Runtime/IO/ExpressionExtensions.cs`)
- Modify: `src/model.rs`

**Interfaces — Produces:**

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)] pub enum ExpressionKey { Preset(Preset), Custom(String) } // Preset: vrm_spec ExpressionPresetName, ordered as listed below
impl ExpressionKey { pub fn parse(name: &str) -> Self; pub fn is_blink(&self) -> bool; pub fn is_look_at(&self) -> bool; pub fn is_mouth(&self) -> bool }
pub(crate) struct Expressions { /* definitions in key order */ }
pub(crate) struct Applied { pub weights: BTreeMap<ExpressionKey, f32> /* actual */, pub look_at_rate: f32 }
impl Expressions {
    pub(crate) fn validate(&self, inputs: &BTreeMap<ExpressionKey, f32>) -> Applied;      // steps 1–3 below
    pub(crate) fn apply(&self, actual: &BTreeMap<ExpressionKey, f32>, morph: &mut [Vec<f32>] /* by draw */, binds: &mut [MtoonOverrides] /* by material */);
}
```

**Decisions** (each as UniVRM's code, cited files):
- **Order** of keys: `happy angry sad relaxed surprised aa ih ou ee oh blink blinkLeft blinkRight lookUp lookDown lookLeft lookRight neutral`, then custom names in byte order; a custom expression named like a preset is dropped (the preset wins). Blink keys: `blink`, `blinkLeft`, `blinkRight`; lookAt keys: `lookUp/Down/Left/Right`; mouth keys: `aa ih ou ee oh`.
- **Inputs** are clamped to 0..1 (VRM 1.0's range; UniVRM does not clamp); unknown names are ignored (UniVRM's `SetWeight`); non-finite inputs are `FrameError` `"expression {name} is not finite"`.
- **Validate** (`DefaultExpressionValidator`): (1) `w = isBinary ? round_ties_even(w) : w` (Unity's `Mathf.Round` rounds halves to even); each expression that is not itself a blink/lookAt/mouth key adds its override rate to that group: `none` 0, `block` `w > 0 ? 1 : 0`, `blend` `w`. (2) Each rate is clamped to 0..1, multiplier `m = 1 − rate`. (3) Each blink/lookAt/mouth key: binary → 0 when `m < 1`; else `w × m`. `look_at_rate` goes to Task 6's gaze.
- **Apply** (`ExpressionMerger.AccumulateValue`): a binary expression contributes `actual > 0.5 ? 1 : 0`. **Morph:** every morph target any expression binds is `Σ bind.weight × w` from 0 each frame, on every draw of the bound node (`MorphTargetBindingMerger`); others keep Task 4's defaults. **Material colour** (`MaterialValueBindingMerger.Apply`): per bound (material, type), `value = base + Σ (target − base) × w`. UniVRM stores `color`, `shadeColor`, `matcapColor`, `rimColor`, and `outlineColor` as sRGB-encoded material properties (`BuiltInVrm10MToonMaterialImporter.TryGetAllColors`) and uses the file's `targetValue` as it is (`ExpressionExtensions.Build10`), so those blend in sRGB: `base = srgb(linear factor)`, the result decoded back to linear; alpha stays linear. `emissionColor` is stored linear and blends linearly. Override fields: `color` → `base_color` (RGBA), `emissionColor` → `emission_color`, `shadeColor` → `shade_color`, `matcapColor`, `rimColor`, `outlineColor` (RGB). **UV:** per bound material, `scale/offset = base + Σ (bind − base) × w`, `base` the material's base colour transform (scale (1, 1), offset (0, 0) without one), bind defaults scale (1, 1), offset (0, 0); written to `texture_offset` and `texture_scale`. UniVRM blends Unity's flipped `_MainTex_ST`; the flip is affine, so blending glTF's values gives the same.
- Material binds reach every draw using the material: `model.rs` turns an unlit material's binds into `MaterialOverrides` and an MToon material's into parameters (Task 4). Bind types on unlit materials other than `color` (and the UV binds) are dropped by the plugin: an unlit material has no such colour.
- `VRMC_vrm_expressions_node_transform` binds are not read.

- [ ] **Step 1:** Tests (`expressions.rs`, through `VrmModel::frame` on the synthetic avatar; morph target 0 of draw 0 unless said; material binds read through a helper `binds(model, inputs) -> MtoonOverrides` rather than `frame.draws[i].overrides`, their expected values unchanged):
  - `blink_alone_moves_its_target`: `{blink: 1}` → 0.5.
  - `block_override_turns_blink_off` (Review Focus): `{happy: 1, blink: 1}` → 1.0; `{happy: 0.3, blink: 1}` → 0.3.
  - `blend_override_scales_blink` (Review Focus): `happy.overrideBlink = "blend"`; `{happy: 0.3, blink: 1}` → 0.3 + 0.5 × 0.7 = 0.65.
  - `binary_expressions_round_half_to_even` (Review Focus): `{wink: 0.5}` → 0; `{wink: 0.51}` → 1; `blink` made binary with `{happy: 0.3, blink: 1}` → happy's 0.3 only.
  - `mouth_override_scales_vowels`: `{aa: 1, happy: 0.4}` → `texture_offset` of material 0 `[0.35, 0.0]` (aa at 0.6).
  - `inputs_are_clamped`: `{happy: 2}` equals `{happy: 1}`; `{nope: 1}` changes nothing; `{happy: NaN}` → error containing `"happy"`.
  - `colour_binds_blend_in_srgb`: `{happy: 0.5}` → material 0 `base_color` override `[1.0, 0.2140411, 0.2140411, 1.0]` within 1e-5 (linear blending would give 0.5).
  - `emission_binds_blend_linearly`: `{happy: 0.5}` → `emission_color` `[0.5, 0.5, 0.0]`.
  - `uv_binds_blend_from_the_material_transform`: `{aa: 0.5}` → `texture_offset [0.375, 0.0]`, `texture_scale [1.0, 1.0]`; no expressions → no overrides at all.
  - `unbound_targets_keep_their_default`: `{happy: 1}` → target 1 stays 0.25.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml expressions` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM expressions with UniVRM's overrides and binds`.

### Task 6: LookAt

**Files:**
- Create: `plugins/vrm/src/look_at.rs` (ported: `Vrm10Runtime/Vrm10RuntimeLookAt.cs`, `LookAt/CurveMapper.cs`, `LookAt/LookAtEyeDirectionApplicableToBone.cs`, `LookAt/LookAtEyeDirectionApplicableToExpression.cs`, `LookAt/Matrix4x4Extensions.cs`, under `Packages/VRM10/Runtime/Components/`)
- Modify: `src/model.rs`, `src/expressions.rs` (look keys written after validation)

**Interfaces — Produces:**

```rust
pub(crate) struct RangeMap { pub input_max: f32, pub output_scale: f32 }   // default (90, 10)
impl RangeMap { pub(crate) fn map(&self, degrees: f32) -> f32 }           // clamp01(d / max(0.001, input_max)) × output_scale
pub(crate) struct LookAt { /* type, offset, four range maps, rest data */ }
impl LookAt {
    pub(crate) fn origin(&self, tree: &NodeTree, worlds: &[Mat4]) -> Mat4;
    pub(crate) fn yaw_pitch(&self, origin: Mat4, target: Vec3) -> (f32, f32);           // degrees
    pub(crate) fn apply(&self, yaw: f32, pitch: f32, pose: &mut Pose, actual: &mut BTreeMap<ExpressionKey, f32>);
}
```

**Decisions:**
- **Input** (`data.look_at`): `{yaw, pitch}` in degrees as given; a point `[x, y, z]` in model space; `"camera"`: the frame camera's eye (Task 7). Absent → yaw 0, pitch 0 (UniVRM's default `YawPitchValue`).
- **Origin** (`InitializeLookAtOriginTransform`): parented to the head with its rest-time local transform `L = H_rest⁻¹ × T(H_rest × offsetFromHeadBone)` (rotation: the model root's, identity); now `O = H_now × L`. `offsetFromHeadBone` defaults to (0, 0.06, 0). A range map missing from the file is (90, 10) (UniVRM's `VRM10ObjectLookAt` defaults); `inputMaxValue` 0 reads as 90 (`CurveMapper.OnValidate`).
- **Yaw/pitch of a point** (`CalcYawPitch`), `p = O⁻¹ × target`: `yaw = atan2(−p.x, p.z)`, `pitch = atan2(p.y, √(p.x² + p.z²))`, degrees. Positive yaw looks to the avatar's right (−X), positive pitch up.
- **Override:** yaw and pitch are multiplied by `1 − look_at_rate` (Task 5) before applying.
- **Bone type** (`LookAtEyeDirectionApplicableToBone`): `yaw < 0`: left eye `−outer.map(−yaw)`, right eye `−inner.map(−yaw)`; else left `inner.map(yaw)`, right `outer.map(yaw)`. `pitch < 0`: `p = down.map(−pitch)`, else `p = −up.map(pitch)`. Each eye's local rotation = `pre × Ry(−eyeYaw) × Rx(p) × post` with `post = R_rest_world(eye)` (relative to the model root) and `pre = local_rest(eye) × post⁻¹`: Unity's `Euler(p, yaw, 0)` in glTF's axes. Missing eye bones: the bone type does nothing.
- **Expression type** (`…ApplicableToExpression`), written into the actual weights after validation: `yaw < 0`: `lookLeft = clamp01(outer.map(−yaw))`, `lookRight = 0`; else `lookRight = clamp01(outer.map(yaw))`, `lookLeft = 0` (UniVRM uses the outer map both ways); `pitch < 0`: `lookDown = clamp01(down.map(−pitch))`, `lookUp = 0`; else `lookUp = clamp01(up.map(pitch))`, `lookDown = 0`.

- [ ] **Step 1:** Tests (`look_at.rs`; eye forward = the eye's world rotation × (0, 0, 1), within 1e-4):
  - `range_map_clamps_and_scales`: (90, 10) maps 45 → 5, 180 → 10, −5 → 0; (0, 10) maps 45 → 10 (`max(0.001, 0)`; the 90 fallback applies only to read files).
  - `angles_turn_the_eyes_by_the_range_maps`: `{yaw: 20, pitch: −30}` → left eye forward (−0.0386809, −0.0697565, 0.9968138), right (−0.0773037, −0.0697565, 0.9945643).
  - `point_targets_give_yaw_and_pitch_from_the_origin`: origin translation (0, 1.46, 0); target (−1, 1.46, 1) → (45, 0); (0, 2.46, 1) → (0, 45); right eye forward for the first is x −0.1736482.
  - `camera_target_uses_the_camera_eye`: explicit camera at (0, 1.46, 2) → eyes at rest; at (2, 1.46, 2) → yaw −45.
  - `expression_type_sets_look_weights`: lookAt type `expression`, outer (90, 1), up (90, 1); `{yaw: −30, pitch: 20}` → morph target 2 of draw 0 = 0.3333333 (`lookLeft`), and actual `lookUp` 0.2222222.
  - `override_scales_the_gaze`: `happy.overrideLookAt = "blend"`, `{happy: 0.5}`, `{yaw: 20, pitch: 0}` → left eye yaw `inner.map(10)` = 1.1111° (forward x −0.0193913).
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml look_at` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM lookAt by bones or expressions`.

### Task 13: A frame's own canvas size (Celesta side; do it after Task 6, before Task 7)

A scene's canvas is fixed at `open`, which only sees assets; a layer's resolution belongs to the composition. A scene frame may therefore carry its own canvas.

**Files:** Modify `crates/plugin/src/scene.rs`, `crates/plugin/src/protocol.rs`, `crates/gpu-renderer/src/mesh.rs` (`plugin_texture`), `crates/gpu-renderer/src/scene.rs` (validation); tests in `crates/plugin/src/protocol.rs` and `crates/gpu-renderer/src/tests/scene.rs`; docs in `docs/handoff/2026-11-03-plugin-scene-content.md` (Frame section) in the same commit.

**Interfaces — Produces:** `SceneFrame.canvas: Option<[f32; 2]>` (serde `canvas`, skipped when `None`; the frame header's `canvas`). When set, it replaces the model's `canvas` for that frame: the canvas texel size, the projection's aspect, and the layer size `plugin_texture` returns all follow it.

**Decisions:**
- `plugin_texture` asks the provider for the frame before sizing the canvas (the measuring-plane stand-in included, so a layer's bounds follow the frame's canvas too).
- A frame canvas must be finite and positive, each side ≤ 16384 → else `InvalidPluginContent` naming `canvas`; the 8,388,608-texel cap still applies to the texels drawn.
- Meshes (2D) frames are unchanged.

- [ ] **Step 1:** Tests: `scene_frame_canvas_round_trips` (protocol: `Some([1080, 1920])` and `None` both round-trip; `None` writes no key); `frame_canvas_overrides_the_model_canvas` (GPU: a model with canvas 64×64 whose frame says 32×64 gives a layer 32 wide, and an orthographic unit quad stays square on screen); `bad_frame_canvas_is_refused` (`[0, 64]`, `[NaN, 1]`, `[20000, 10]` → `"canvas"`).
- [ ] **Step 2:** `cargo test -p celesta-plugin protocol` and `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer canvas` → fail.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the root checks (`env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer`, workspace clippy, fmt).
- [ ] **Step 5:** Commit `feat(plugins): let a scene frame set its own canvas`.

### Task 7: Camera presets, explicit cameras, and lights

**Files:**
- Create: `plugins/vrm/src/camera.rs`
- Modify: `src/model.rs`

**Interfaces — Produces:**

```rust
#[derive(Deserialize)] #[serde(untagged)] pub enum CameraData { Preset(CameraPreset), Explicit(ExplicitCamera) }
#[derive(Deserialize)] #[serde(rename_all = "lowercase")] pub enum CameraPreset { Full, Bust, Face }
#[derive(Deserialize)] #[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExplicitCamera { pub position: [f32; 3], pub target: [f32; 3], #[serde(default)] pub fov: Option<f32> /* degrees */, #[serde(default)] pub near: Option<f32>, #[serde(default)] pub far: Option<f32> }
#[derive(Deserialize)] #[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LightsData { #[serde(default)] pub directional: Vec<DirectionalLight>, #[serde(default)] pub ambient: [f32; 3] }
pub(crate) struct Framing { pub bounds_min: Vec3, pub bounds_max: Vec3 }        // rest-pose skinned bounds of visible draws, at load
pub(crate) fn camera(data: Option<&CameraData>, framing: &Framing, humanoid: &Humanoid, rest: &[Mat4], aspect: f32 /* canvas width / height */) -> Result<(Camera, Vec3 /* eye */), FrameError>
pub(crate) fn lights(data: Option<&LightsData>) -> Lights
pub const REFERENCE_LIGHTS: … // the Global Constraints' values
```

**Decisions:**
- **Framing bounds:** every vertex of every visible draw at rest (skinned with the rest skinning matrices, no morphs), in model space, computed once at load.
- **Presets** frame a vertical span `[y0, y1]` on the frame's canvas (its aspect from `data.canvas`, Task 13): `full` `[min.y, max.y]`, x centre of the bounds; `bust` `[spine.y, max.y]`, x of the head; `face` `[neck.y, max.y]` (without a neck `head.y − ½(max.y − head.y)`), x of the head; positions at rest. `h = 1.1 (y1 − y0)`, `d = (h / 2) / tan(15°)`; for `full` the bounds' width must also fit: `d = max(d, (1.1 (max.x − min.x) / 2) / (tan(15°) × aspect))`. Eye `(x, (y0 + y1) / 2, head.z + d)`, looking down −Z: `view = translation(−eye)`. Default `full`. The frame answers `canvas = data.canvas` (default `CANVAS`).
- **Explicit:** `view = look_at_rh(position, target, up)` with up +Y, or −Z when the direction is within 1e-6 of ±Y; `fov` default 30 (degrees), `near` 0.01, `far` 100. `position == target`, `fov` outside (0, 180), or `0 < near < far` failing → `FrameError` naming the field.
- **Lights:** absent → `REFERENCE_LIGHTS`; given → its directional lights (linear colours, at most 4, checked by Celesta) and `ambient` as `c3` alone.

- [ ] **Step 1:** Tests (`camera.rs`, synthetic avatar: framing bounds x ±0.2, y 0–1.6, z ±0.1; the hidden quad does not count):
  - `presets_frame_the_humanoid`: at aspect 16/9, `full` eye (0, 0.8, 3.2842047), `bust` (0, 1.3, 1.2315768), `face` (0, 1.45, 0.6157884), within 1e-4; projection `Perspective { yfov: 0.5235988, near: 0.01, far: 100.0 }`; default equals `full`; at aspect 9/16 the same (the 0.44 m width doesn't bind); at aspect 0.1 `full`'s eye z is `0.1 + (0.22 / (tan(15°) × 0.1))` = 8.3104…, within 1e-3 (width binds).
  - `explicit_camera_looks_at_its_target`: `{position: [0, 1.5, 2], target: [0, 1.5, 0], fov: 40}` → `view` translates by (0, −1.5, −2), `yfov` 0.6981317; `{position: [0, 3, 0], target: [0, 0, 0]}` → finite view mapping world −Y to view −Z.
  - `bad_explicit_cameras_are_refused`: same position and target → `"camera position and target"`; `fov: 0` → `"fov"`.
  - `reference_lights_by_default`: `lights(None)` equals the Global Constraints' preset; `{ambient: [0.1, 0.2, 0.3]}` → `ambient[3] == [0.1, 0.2, 0.3]`, no directional lights.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml camera` → fails.
- [ ] **Step 3:** Implement; `frame` takes the camera and lights from here. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM camera presets and the reference light`.

### Task 8: Usage terms, the provider, and the process

**Files:**
- Create: `plugins/vrm/src/meta.rs`, `plugins/vrm/tests/process.rs`
- Modify: `src/provider.rs` (the Task 1 stub)
- Modify: `src/lib.rs`, `src/main.rs`, `src/model.rs` (notices for skipped primitives)

**Interfaces — Produces** (`provider.rs`; the motion plan adds fields to `PerformanceData`):

```rust
#[derive(Debug, Default, Deserialize)] #[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PerformanceData {
    #[serde(default)] pub expressions: BTreeMap<String, f32>,
    #[serde(default)] pub look_at: Option<LookAtData>,
    #[serde(default)] pub camera: Option<CameraData>,
    #[serde(default)] pub lights: Option<LightsData>,
}
#[derive(Deserialize)] #[serde(untagged)] pub enum LookAtData { Keyword(LookAtKeyword), Point([f32; 3]), Angles { yaw: f32, pitch: f32 } }
#[derive(Deserialize)] pub enum LookAtKeyword { #[serde(rename = "camera")] Camera }
#[derive(Default)] pub struct VrmProvider { models: HashMap<ModelId, VrmModel>, next: u64 }
pub(crate) fn usage_notices(meta: &vrm_spec::vrmc_vrm_1_0::Meta) -> Vec<Notice>
```

**Decisions:**
- **Assets:** `model` (the `.vrm`) is required (`"asset \"model\" (the .vrm) is missing"`); any other name is refused (`"unknown asset \"{name}\""`) until the motion plan adds clips.
- `open` loads with `VrmModel::load`, answers `ContentModel::Scene` and the model's notices; `frame` reads `data` (`null` → default; a serde error → `"invalid data: {error}"`), answers `ContentFrame::Scene`; `close` drops the model. Every error is `PluginError::Failed { plugin: "vrm", … }`.
- **Usage terms** (`meta`; a field absent from the file takes VRM 1.0's default, given in brackets). One `info` notice per line, in this order, except where marked `warning`:
  - `VRM "{name}" {version} by {authors}; licence {licenseUrl}` (plus `; other licence {otherLicenseUrl}`), then copyright, contact, references, and third-party licences when present.
  - `avatarPermission` [`onlyAuthor`]: `everyone` → "Avatar use: anyone"; `onlySeparatelyLicensedPerson` → **warning** "Avatar use: only people the author licenses separately"; `onlyAuthor` → **warning** "Avatar use: only the author".
  - `commercialUsage` [`personalNonProfit`]: `personalNonProfit` → **warning** "Commercial use: not allowed (personal, non-profit only)"; `personalProfit` → "Commercial use: individuals only, not corporations"; `corporation` → "Commercial use: allowed".
  - `creditNotation` [`required`]: `required` → **warning** "Credit: required; credit {authors} where the video is published"; `unnecessary` → "Credit: not required".
  - `allowExcessivelyViolentUsage`, `allowExcessivelySexualUsage`, `allowPoliticalOrReligiousUsage`, `allowAntisocialOrHateUsage` [false]: "{Excessively violent} use: allowed/not allowed".
  - `allowRedistribution` [false] and `modification` [`prohibited`]: "Redistribution of the model: …", "Modification of the model: …" (an exported video is neither).
  The plugin never refuses to draw for its terms.
- Other notices from loading (PBR, unreadable images, non-triangle primitives: `"mesh \"{name}\" primitive {p} is {mode}, which is not drawn"`) follow the terms.

- [ ] **Step 1:** Tests:
  - `meta.rs` `terms_that_forbid_likely_video_use_warn` (Review Focus): the synthetic meta → no `warning`; with `commercialUsage` and `creditNotation` removed → two warnings containing `"Commercial use: not allowed"` and `"Credit: required; credit Celesta"`; `avatarPermission: "onlyAuthor"` → a warning; every case's first notice is `info` and contains `"Synthetic"`.
  - `provider.rs` `data_is_checked`: `{"expresions": {}}` → error containing `"invalid data"`; `null` → the default frame; an asset `"motion"` → `"unknown asset"`.
  - `tests/process.rs` `the_process_answers_as_the_provider_does`: the synthetic VRM written to a temp dir, the built `celesta-plugin-vrm` in a temp plugin directory (as the turntable's process test); `PluginHost` and `VrmProvider` open it with `{model}`; contents and notices equal; frames at times `[2.0, 0.0, 2.0]` with data `{"expressions": {"happy": 0.5}, "lookAt": {"yaw": 10, "pitch": 0}, "camera": "bust"}` equal.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml -- meta provider` and `--test process` → fail.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM usage-term notices and the plugin process`.

### Task 9: Model-file components in the manifest and the Add menu

**Files:**
- Modify: `crates/plugin-host/src/manifest.rs` (types, checks, tests), `crates/editor/src/add_element.rs` (kind, file check, aspect; tests)

**Interfaces — Produces:**

```rust
pub enum ComponentSource { Image, Video, Model }                 // "image" | "video" | "model"
pub struct ComponentInfo { /* name, label, src, props as before */
    #[serde(default)] pub extensions: Vec<String>,               // the file types a model component takes, lowercase, no dot
    #[serde(default)] pub aspect: Option<f64>,                   // the layer's width over height, for drawing
}
// add_element.rs
pub(crate) fn refused_model_file(info: &ComponentInfo, path: &Path) -> Option<String>
```

**Decisions:**
- **Manifest checks** (`invalid_reason`, the skip-and-report path every invalid manifest takes): `src: "model"` needs a non-empty `extensions` (`"component \"Vrm\" takes a model file but lists no extensions"`); `extensions` or `aspect` without `src: "model"` → `"component \"Vrm\" lists extensions/an aspect but does not take a model file"`; an extension that is not `[a-z0-9]+` → `"component \"Vrm\"'s extension \".VRM\" is not lowercase letters and digits"`; `aspect` not finite and positive → refused. Drawn props for a model are an image's (`src`, `x`, `y`, `width`).
- **Editor:** `AddComponent::kind` maps `Model` to `NewKind::Image`, so choosing it asks for a file and draws a box as an Image does. GPUI's `PathPromptOptions` has no file-type filter, so `pick_add_file` checks the chosen file before importing it: `refused_model_file` returns `"Choose a .vrm file"` (several: `"Choose a .vrm or .glb file"`, `", "` before the last `" or "`) when the extension, compared ASCII case-insensitively, is not listed; the mode then ends with that notice, as a failed import does. `self.add.component` is set before `step_add`, so the check reads it there.
- The drawing's aspect for a model component is the manifest's `aspect` (sent as `AddEvent::Measured` without probing the file); without one, the box is free, as for an unreadable image.

- [ ] **Step 1:** Tests:
  - `manifest.rs` `model_components_need_extensions`: `{"name":"Vrm","src":"model","extensions":["vrm"],"aspect":1.7777778}` is valid with drawn props `["src","x","y","width"]`; skipped with the messages above for no extensions, `"extensions":["VRM"]`, extensions on an image component, and `"aspect": 0`.
  - `add_element.rs` `model_components_draw_as_images_and_check_the_file`: the component's `kind()` is `NewKind::Image`; `refused_model_file(info, "a/avatar.VRM")` is `None`; for `"a/avatar.png"` it is `Some("Choose a .vrm file")`; with `["vrm", "glb", "gltf"]` the text is `"Choose a .vrm, .glb, or .gltf file"`; `element(&NewElement::Image { src: "./assets/a.vrm", x: 10, y: 20, width: 960 })` writes `src`, `x`, `y`, `width`.
- [ ] **Step 2:** `cargo test -p celesta-plugin-host manifest` and `cargo test -p celesta-editor add_element` → fail to compile.
- [ ] **Step 3:** Implement. **Step 4:** Both pass; root clippy and fmt.
- [ ] **Step 5:** Commit `feat(editor): plugin components that take a model file`.

### Task 10: `<Vrm>`, the build script, and the demo

**Files:**
- Create: `plugins/vrm/js/src/index.ts`, `js/package.json` (`celesta-plugin-vrm-js`, private, `"test": "tsc -p test && vitest run"`), `js/tsconfig.json`, `js/test/tsconfig.json`, `js/vitest.config.mjs`, `js/test/vrm.test.mts`, `plugins/vrm/plugin.json`, `plugins/vrm/build.mjs`, `plugins/vrm/demo.tsx`
- Modify: `pnpm-workspace.yaml` (`"plugins/*/js"`), `pnpm-lock.yaml`, `.github/workflows/ci.yml` (`vrm-plugin` gains mise, `pnpm install --frozen-lockfile`, `pnpm run build:runtime`, `pnpm --filter celesta-plugin-vrm-js test`, `node plugins/vrm/build.mjs`)

**Interfaces — Produces:**

```ts
export interface VrmProps extends CommonProps {
  src: AssetInput;                                    // the .vrm
  width?: number; height?: number;                    // the layer's resolution in px; default 1920×1080
  aspect?: number;                                    // width / height, filling whichever of width/height is missing (else 16:9)
  expressions?: Record<string, number>;               // preset or custom names, 0..1; win over blink and mouth
  mouth?: MouthShape | Partial<Record<Vowel, number>>;// 'a'…'o' → aa…oh at 1
  lipSync?: LipSyncTrack; mouthSmoothing?: number;    // as <Live2D>: default 2
  blink?: BlinkTiming | false;                        // default {}: @celesta/character's blinkPhase
  lookAt?: 'camera' | [number, number, number] | { yaw: number; pitch: number };
  camera?: 'full' | 'bust' | 'face' | VrmCamera;      // default 'full'
  lights?: VrmLights;                                 // default: the plugin's reference light
}
export type Vowel = 'aa' | 'ih' | 'ou' | 'ee' | 'oh';
export interface VrmCamera { position: [number, number, number]; target: [number, number, number]; fov?: number; near?: number; far?: number }
export interface VrmLights { directional?: { direction: [number, number, number]; color?: string; intensity?: number }[]; ambient?: string }
export function Vrm(props: VrmProps): React.ReactElement;   // createElement('celesta-plugin-vrm', vrmElementProps(props, useCurrentFrame(), fps))
export function vrmElementProps(props: VrmProps, frame: number, fps: number): Record<string, unknown>; // pure: the host element's props
export function vrmData(props: VrmProps, frame: number, fps: number): VrmData;   // what the layer sends, a pure function of the frame
```

**Decisions:**
- **Canvas:** `vrmData(…).canvas` is `[w, h]`: both given → as given; one given → the other from `aspect` (default 16/9); neither → 1920×1080, or 1920 wide by `aspect`. Non-finite or non-positive values throw `Error('Vrm width, height, and aspect are positive numbers')`. The layer is not scaled by them: the canvas is the layer's size.
- `vrmElementProps` gives the host element `celesta-plugin-vrm` the common props (without `width`, `height`, `aspect`), `time = frame / fps`, and `data = vrmData(…)`; `content` returns `{ type: 'plugin', plugin: 'vrm', assets: { model: walk.resolveAsset(src) }, time, data }`. Keys are sent only when set (as `<Live2D>`).
- **Blink:** `blinkPhase(frame, fps, blink)`: `open` 0, `half` 0.5, `closed` 1 → `expressions.blink`. **Mouth:** a shape sets its vowel to 1 (`closed` none); an object passes through; `lipSync` mixes the shapes of this frame and the `mouthSmoothing` frames before it, nearest weighted most (weights `smoothing + 1 − back`), per vowel. Explicit `expressions` replace these keys.
- **Lights colours** are CSS `#rgb`/`#rrggbb` (sRGB), decoded to linear; `color` default `#ffffff`, `intensity` default 1; anything else throws `Error('Vrm lights colors are #rgb or #rrggbb')`.
- `plugin.json`: `{ "id": "vrm", "version": "0.1.0", "module": "js/index.ts", "cargoPackage": "celesta-plugin-vrm", "components": [{ "name": "Vrm", "label": "VRM avatar", "src": "model", "extensions": ["vrm"], "aspect": 1.7777778 }] }`.
- `build.mjs` does what `examples/plugins/build.mjs` does for one plugin: `cargo build --release --manifest-path plugins/vrm/Cargo.toml --bin celesta-plugin-vrm [--target]`, the executable from `plugins/vrm/target` (or `CARGO_TARGET_DIR`) to `bin/<os>-<arch>/`, declarations checked against the runtime's project types, the module bundled with `react` and `@celesta/*` external, and the manifest (with `components`, `extensions`, `aspect`) to `--out` (default `plugins/vrm/dist`)`/vrm/celesta-plugin.json`; `--target` and `--skip-js` as there.
- `demo.tsx`: a 1920×1080, 6 s composition, `<Vrm src="./assets/Seed-san.vrm" camera="bust" lookAt="camera" expressions={{ relaxed: 0.3 }} mouth=…/>` cycling vowels every 10 frames; `README.md` says to download Seed-san (VRM Public License 1.0) from vrm-c/vrm-specification's `samples/Seed-san/vrm` into `plugins/vrm/assets/` (git-ignored).

- [ ] **Step 1:** `js/test/vrm.test.mts`:
  - `blinks_from_blinkPhase`: at the first frame in 0..300 where `blinkPhase(f, 30, {})` is `'closed'`, `vrmData({src}, f, 30).expressions.blink === 1`; `blink: false` → no `blink` key.
  - `mouth_shapes_set_vowels`: `mouth: 'a'` → `{aa: 1}`; with `expressions: {aa: 0.2}` → 0.2.
  - `lip_sync_smooths_like_live2d`: a track that is `'a'` before 1 s and `'closed'` after, frame 30 at 30 fps, smoothing 2 → `aa` 0.5.
  - `lights_take_srgb_hex`: `{directional: [{direction: [0, -1, 0], color: '#808080'}], ambient: '#fff'}` → colour `[0.2158605, 0.2158605, 0.2158605]` (5e-7), intensity 1, ambient `[1, 1, 1]`; `'red'` throws.
  - `canvas_follows_width_height_and_aspect`: `{}` → `[1920, 1080]`; `{width: 1080, height: 1920}` → `[1080, 1920]`; `{height: 1920, aspect: 9/16}` → `[1080, 1920]`; `{width: 960}` → `[960, 540]`; `{aspect: 1}` → `[1920, 1920]`; `{width: 0}` throws; the element props carry no `width`/`height`/`aspect` and leave `scale` as given.
- [ ] **Step 2:** `pnpm run build:runtime`, then `pnpm --filter celesta-plugin-vrm-js test` → fails (no module).
- [ ] **Step 3:** Implement. **Step 4:** The tests pass; `node plugins/vrm/build.mjs` writes `plugins/vrm/dist/vrm` with the manifest, the executable, `js/index.js`, `js/index.d.ts`; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): the <Vrm> component and the VRM plugin's build`.

### Task 11: Drawing it, and the official samples

**Files:**
- Create: `plugins/vrm/tests/render.rs`, `plugins/vrm/tests/samples.rs`

**Decisions:**
- `render.rs` draws the synthetic avatar through `GpuRenderer` with the provider in process (no executable), layer 1920×1080 at density 0.25 (480×270 texels), the `full` camera; returns early without an adapter unless `CELESTA_REQUIRE_GPU=1` (the turntable's pattern).
- `samples.rs` reads `CELESTA_VRM_SAMPLES_DIR`; each test prints `skipped: CELESTA_VRM_SAMPLES_DIR is not set` and returns without it.

- [ ] **Step 1:** Tests:
  - `render.rs` `the_avatar_draws_and_repeats`: pixel (240, 135) (the box's middle) has alpha 255 and a corner pixel is clear; rendering frame 1.0 twice gives equal bytes; `{happy: 1}` changes some pixel (the base colour bind).
  - `samples.rs` `seed_san_opens`: `VrmModel::load` succeeds; draws > 0; the first notice is `info` and contains the file's `meta.licenseUrl`; one notice contains `"drawn as unlit base colour"`; frames at times `[3.0, 0.0, 1.5, 3.0]` with `{blink: 1, aa: 0.5}` equal those played in order on a fresh model, and every number in them is finite.
  - `samples.rs` `constraint_twist_sample_opens_with_bone_look_at`: loads; `{lookAt: {yaw: 20, pitch: 0}}` changes the eye nodes' worlds against the rest frame.
  - `samples.rs` `uv_animation_sample_keeps_its_speeds`: some material in `materials.mtoon` has a non-zero `uv_animation_scroll_x_speed_factor`, `_y_`, or rotation speed, and a `uv_animation_mask_texture`.
  - `samples.rs` `seed_san_draws`: (GPU) the bust frame has opaque pixels in the canvas's middle third.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/vrm/Cargo.toml --test render --test samples` with `CELESTA_VRM_SAMPLES_DIR` set → fail (no files).
- [ ] **Step 3:** Implement; fix what they find in the earlier tasks' modules, each fix with its own unit test. **Step 4:** Pass with and without the samples; then the task checks.
- [ ] **Step 5:** Commit `test(plugins): draw the VRM plugin's avatar and open the official samples`.

### Task 12: Docs

**Files:**
- Create: `docs/handoff/2026-11-03-plugin-vrm.md` (what the plugin is and how it builds and installs; loading rules; geometry, materials, textures; skins, draws, `center`; expressions, lookAt, first person; camera presets and the reference light (with its Unity values); usage terms; `<Vrm>`; the manifest's model kind and the Add menu; What landed; Not done; Where Celesta knowingly differs from UniVRM; Checks)
- Modify: `packages/website/content/en/docs/05-(more)/05-plugins.md` and `packages/website/content/ja/docs/05-(more)/05-plugins.md` (a "The VRM plugin" section: building it with `node plugins/vrm/build.mjs`, installing `plugins/vrm/dist/vrm`, `<Vrm>`'s props with an example, the usage-term notices, PBR drawn unlit; "Use its components": components that take a model file), `skills/celesta/references/plugins.md` (the same, and the notes list), `docs/handoff/2026-11-03-plugin-scene-materials.md` ("For plugins": which items the plugin handles, pointing to the new note)

The VRM note gains an **MToon** section moved from the phase-2 mtoon note: its fields and defaults table, how it shades, outlines, binds packed into parameters, and "Where it knowingly differs from UniVRM": the mask edge, `V` per fragment, texture rotation honoured, emissive strength folded in, additional lights in one fragment, the glTF-axis translations, the screen-width formula and its `P[1][1]` cap, `linearstep` at toony 1, coplanar draws and the outline's depth bias.

**Not done** lists at least: VRM 0.x (refused; phase 4); glTF PBR drawn unlit; humanoid motion, constraints, and spring bones (the motion plan); `VRMC_vrm_expressions_node_transform`; KTX2 textures; `TEXCOORD_1`, vertex colours, more than four joints a vertex; the texture cache is never cleaned; Celesta compiles the MToon shader once per renderer, when the first avatar opens (a stall on the first MToon avatar); parity renders against UniVRM not yet made. **Knowingly differs / to check against UniVRM:** colour binds blended in sRGB; the `center` offset from the skin's skeleton; expression inputs clamped to 0..1; texture rotation honoured.

- [ ] **Step 1:** Write the docs. **Step 2:** `pnpm --dir packages/website run build` passes.
- [ ] **Step 3:** Commit `docs: the VRM plugin`.

### Finish

- [ ] Full checks once: plugin `cargo fmt --check`, clippy, `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/vrm/Cargo.toml --locked` (with and without `CELESTA_VRM_SAMPLES_DIR`), root `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p celesta-plugin-host -p celesta-editor`, `pnpm --filter celesta-plugin-vrm-js test`.
- [ ] Export a still: `node plugins/vrm/build.mjs`, Seed-san in `plugins/vrm/assets/`, `cargo run -p celesta-exporter -- --react plugins/vrm/demo.tsx seed-san.png --frame 30 --plugin-dir plugins/vrm/dist`; check its notices print no `warning` before attaching it.
- [ ] Push and open the PR stacked on the phase-2 branch (`gh stack`), attaching the still with `gh pr create --attach` (a new plugin: there is no before).
