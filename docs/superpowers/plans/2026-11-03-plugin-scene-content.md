# Plugin scene content (phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A content plugin can return a `scene` (skinned, morphing triangle meshes with unlit materials and a camera), and Celesta's GPU renderer draws it with depth and MSAA into the layer's canvas.

**Architecture:** `celesta-plugin` gains scene types beside the 2D meshes, a `ContentModel`/`ContentFrame` enum over both kinds, `Opened` with notices, and their wire encoding. `celesta-gpu-renderer` gains `scene.rs`: it uploads a scene model once, skins and morphs in a compute pass each frame, draws unlit materials in queue order into `Rgba16Float` + `Depth32Float` at 4× MSAA, and converts the result to the same layer texture the 2D meshes produce. A sample plugin (`examples/plugins/turntable`) exercises it through a real process.

**Tech Stack:** Rust 2024, wgpu 30 (WGSL), serde; Node.js 26 for the sample's build and module.

**Spec:** `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md` (sections Protocol, Rendering in Celesta → Placement, Targets, Per frame 1–2, Unlit). MToon, lights in shading, and outlines are phase 2's plan.

## Global Constraints

- The protocol stays version 1 and changes in place (unreleased). 2D meshes keep their wire format and drawing exactly.
- `celesta-plugin` keeps depending on nothing of Celesta's (serde, serde_json, thiserror, libc on Unix).
- Scene conventions: right-handed, +Y up, metres; the camera looks down its −Z; matrices `f32` 4×4 column-major; quaternions `x, y, z, w`; scene UVs have their origin at the texture's top-left (glTF), unlike 2D meshes; colors linear unless stated.
- Targets: color `Rgba16Float`, linear, 4× MSAA; depth `Depth32Float`, 4× MSAA; cleared to transparent black; resolved, then converted to the layer texture's encoding (premultiplied, sRGB-encoded values in `Rgba8Unorm`, as `mesh.rs`'s `OUTPUT_FORMAT`). No tone mapping.
- Queue order: `opaque` 2000, `mask` 2450, `blend` 3000; ties by draw index; `blend` sorted back to front by draw centre. Color blends `SrcAlpha, OneMinusSrcAlpha`; alpha `One, OneMinusSrcAlpha`. `mask` uses alpha-to-coverage with the sharpened alpha `(a − cutoff) / max(fwidth(a), 1e-5) + 0.5`, then writes alpha 1.
- The CPU reference renderer and `@celesta/web` keep refusing plugin content.
- Tests are Rust (`cargo test`); JavaScript tests, if any, are Vitest in TypeScript.
- Commits: never amend; one commit per task at least; commit and test runs are separate commands.

## Review Focus

- **A frame that does not fit its model** (draw index out of range, morph weights count ≠ the geometry's targets, missing or short skinning matrices, a joint index ≥ the skin's joint count): an `InvalidPluginContent` error naming what is wrong, never a panic or a GPU validation error. Task 4 tests each.
- **A degenerate camera** (`near ≤ 0`, `far ≤ near`, `yfov` not in (0, π), zero `height`, a non-invertible view): `InvalidPluginContent`, not NaN pixels. Task 4.
- **A layer far smaller than its canvas or zoomed in** (density ≠ 1): drawn at the requested texel size like 2D meshes, edges antialiased. Task 5 renders at density 2.
- **Frames asked out of order or repeatedly**: the same pixels for the same time, nothing carried between frames on the GPU side. Task 5.
- **A plugin that sends an `opened` without `kind`** (old 2D plugins such as the out-of-tree Live2D one): read as `meshes`. Task 2.

---

### Task 1: Scene types, `ContentModel`/`ContentFrame`, notices, and the provider trait

**Files:**
- Create: `crates/plugin/src/scene.rs`
- Modify: `crates/plugin/src/lib.rs`, `crates/plugin/src/provider.rs`, `crates/plugin/src/protocol.rs` (only to compile: `serve_on` matches the new types; encoding of scenes is Task 2 — return `error_message("scene content is not encoded yet")` for scenes until then)
- Modify (compile only): `crates/gpu-renderer/src/mesh.rs` and its callers, `crates/gpu-renderer/src/tests/plugin.rs`, `crates/plugin-host/src/host.rs`, `examples/plugins/flag/src/*.rs`
- Test: `crates/plugin/src/scene.rs` (unit tests)

**Interfaces — Produces** (all `pub`, re-exported from `celesta_plugin`; `Clone, Debug, PartialEq`; serde derives with `rename_all = "camelCase"` where the type appears in a JSON header):

```rust
pub struct SceneModel { pub canvas: [f32; 2], pub textures: Vec<SceneTexture>, pub geometries: Vec<Geometry>,
    pub skins: Vec<Skin>, pub materials: Vec<Material>, pub draws: Vec<DrawInfo> }
pub struct SceneTexture { pub source: TextureSource, pub color_space: ColorSpace, pub sampler: Sampler }
pub enum ColorSpace { Srgb, Linear }                                   // serde "srgb" | "linear"
pub struct Sampler { pub wrap_s: Wrap, pub wrap_t: Wrap, pub mag_filter: Filter, pub min_filter: Filter, pub mipmaps: bool }
pub enum Wrap { Repeat, MirroredRepeat, ClampToEdge }                 // Default: Repeat
pub enum Filter { Nearest, Linear }                                    // Default: Linear; Sampler::default() mipmaps: true
pub struct Geometry { pub positions: Vec<[f32; 3]>, pub normals: Vec<[f32; 3]>, pub uvs: Vec<[f32; 2]>,
    pub tangents: Option<Vec<[f32; 4]>>, pub joints: Option<Vec<[u16; 4]>>, pub weights: Option<Vec<[f32; 4]>>,
    pub targets: Vec<MorphTarget>, pub indices: Vec<u32> }
pub struct MorphTarget { pub positions: Vec<[f32; 3]>, pub normals: Vec<[f32; 3]> }   // normals empty or one per vertex
pub struct Skin { pub joints: u32 }
pub struct Material { pub alpha_mode: AlphaMode, pub double_sided: bool, pub kind: MaterialKind }
pub enum AlphaMode { Opaque, Mask { cutoff: f32 }, Blend }             // serde tag "alphaMode": "opaque" | "mask" (+ "alphaCutoff") | "blend"
pub enum MaterialKind { Unlit(UnlitMaterial) }                         // serde tag "kind": "unlit"; phase 2 adds Mtoon
pub struct UnlitMaterial { pub base_color: [f32; 4], pub base_color_texture: Option<TextureRef> }
pub struct TextureRef { pub texture: usize, pub transform: Option<TextureTransform> }
pub struct TextureTransform { pub offset: [f32; 2], pub rotation: f32, pub scale: [f32; 2] }   // KHR_texture_transform
pub struct DrawInfo { pub geometry: usize, pub material: usize, pub skin: Option<usize> }

pub struct SceneFrame { pub camera: Camera, pub lights: Lights, pub time: f64,
    pub draws: Vec<DrawState>, pub skins: Vec<Vec<[f32; 16]>> }      // skins[i]: skinning matrices (joint world × inverse bind)
pub struct Camera { pub view: [f32; 16], pub projection: Projection }
pub enum Projection { Perspective { yfov: f32, near: f32, far: f32 }, Orthographic { height: f32, near: f32, far: f32 } }
pub struct Lights { pub directional: Vec<DirectionalLight>, pub ambient: [[f32; 3]; 9] }   // SH, ShadeSH9 order; Default: none, zeros
pub struct DirectionalLight { pub direction: [f32; 3], pub color: [f32; 3], pub intensity: f32 }
pub struct DrawState { pub index: usize, pub visible: bool, pub world: [f32; 16], pub morph_weights: Vec<f32>,
    pub center: Option<[f32; 3]>, pub overrides: MaterialOverrides }
pub struct MaterialOverrides { pub base_color: Option<[f32; 4]>, pub texture_offset: Option<[f32; 2]>,
    pub texture_scale: Option<[f32; 2]> }                              // Default: all None

pub enum ContentModel { Meshes(MeshModel), Scene(SceneModel) }        // From<MeshModel>, From<SceneModel>
pub enum ContentFrame { Meshes(MeshFrame), Scene(SceneFrame) }        // From<MeshFrame>, From<SceneFrame>
pub struct Notice { pub level: NoticeLevel, pub message: String }
pub enum NoticeLevel { Info, Warning }                                 // serde "info" | "warning"
pub struct Opened { pub model: ModelId, pub content: ContentModel, pub notices: Vec<Notice> }

pub trait ContentProvider: Send {
    fn open(&mut self, plugin: &str, assets: &BTreeMap<String, PathBuf>) -> Result<Opened, PluginError>;
    fn frame(&mut self, model: ModelId, time: f64, data: &serde_json::Value) -> Result<ContentFrame, PluginError>;
    fn close(&mut self, _model: ModelId) {}
}
```

Semantics fixed here (doc comments say them): a skinned draw's vertex is `world × Σ wᵢ·skinᵢ·p` (the plugin sends `world` = identity for glTF skinned meshes); unskinned is `world × p`; morph deltas are added before skinning; `center` is the world-space point transparent draws are sorted by, else Celesta uses `world` × the geometry's rest bounds centre.

- [ ] **Step 1:** Write `scene_model_default_sampler` (asserting `Sampler::default()` is repeat/repeat/linear/linear/mipmaps) and `alpha_mode_json` (asserting `AlphaMode::Mask { cutoff: 0.5 }` serializes inside `Material` as `"alphaMode":"mask","alphaCutoff":0.5` and `Opaque` as `"alphaMode":"opaque"`).
- [ ] **Step 2:** `cargo test -p celesta-plugin scene` → fails to compile (types missing).
- [ ] **Step 3:** Add `scene.rs` with the types; change the trait and `ContentModel`/`ContentFrame`/`Opened`; update every implementor and caller to compile (flag returns `Opened { model, content: model.into(), notices: vec![] }`; renderer and host match `ContentModel::Meshes` and error on `Scene` until Tasks 3–4).
- [ ] **Step 4:** `cargo test -p celesta-plugin scene` passes; `cargo build --workspace --all-targets` builds.
- [ ] **Step 5:** Commit `feat(plugins): scene content types and notices in the provider interface`.

### Task 2: Wire encoding — `kind`, `notices`, `roles`, scene model and frame

**Files:** Modify `crates/plugin/src/protocol.rs` (tests in its `mod tests`).

**Interfaces — Produces:** `encode_opened(&Opened) -> Message`, `decode_opened(&Message) -> Result<Opened, ProtocolError>`, `encode_content_frame(&ContentFrame) -> Message`, `decode_content_frame(&Message, kind: ContentKind) -> Result<ContentFrame, ProtocolError>`, `pub enum ContentKind { Meshes, Scene }`, `pub const ROLES: &[&str] = &["content"]`. The old `encode_model`/`decode_model`/`encode_frame`/`decode_frame` stay as the `meshes` halves (private or `pub(crate)` if nothing outside uses them).

Wire format (header JSON, then the payload in exactly this order, little-endian):

- `hello` answer adds `"roles": ["content"]`.
- `opened` header adds `"kind": "meshes" | "scene"` (absent = `"meshes"`) and `"notices": [{"level","message"}]` (absent = none).
- Scene `opened` header: `model`, `canvas`, `textures: [{path} | {width,height}` each plus `colorSpace` and `sampler: {wrapS,wrapT,magFilter,minFilter,mipmaps}]`, `geometries: [{vertices, indices, tangents: bool, skinned: bool, targets: n, targetNormals: bool}]`, `skins: [{joints}]`, `materials: [Material as JSON]`, `draws: [DrawInfo as JSON]`. Payload, per geometry in order: positions (3×f32), normals (3×f32), uvs (2×f32), tangents (4×f32) if any, joints (4×u16) and weights (4×f32) if skinned, then each target's positions (3×f32) and normals (3×f32, if `targetNormals`); then every geometry's indices (u32); then inline texture pixels, in texture order.
- Scene `frame` header: `camera`, `lights`, `time`, `draws: [{index, visible, world: [16], center?, overrides, weights: n}]`, `skins: [{joints: n}]`. Payload: every skin's matrices (16×f32 each) in skin order, then every draw's morph weights (f32) in draw order.

- [ ] **Step 1:** Write tests: `scene_model_round_trips` (a model with one sRGB inline texture, a skinned geometry with tangents and two targets, one unskinned geometry without tangents, both alpha modes, a texture transform — decode(encode(x)) == x); `scene_frame_round_trips` (perspective camera, a directional light, SH ambient, two draws with weights, overrides, one with `center`); `opened_without_kind_is_meshes` (a hand-built v1 `opened` header without `kind`/`notices` decodes to `ContentModel::Meshes` with no notices); `notices_round_trip`; `hello_names_roles` (a served session's hello answer has `roles == ["content"]`); `scene_payload_too_short_is_malformed`; `serves_a_scene_session` (hello, open, frame, close through `serve_on` with an in-test provider returning a scene).
- [ ] **Step 2:** `cargo test -p celesta-plugin protocol` → the new tests fail.
- [ ] **Step 3:** Implement the encoding with the existing `Cursor` (add `triples`, `quads`, `u16x4`, `matrices` readers) and `serve_on` using `encode_opened`/`encode_content_frame`.
- [ ] **Step 4:** `cargo test -p celesta-plugin` passes.
- [ ] **Step 5:** Commit `feat(plugins): encode scene content, notices, and roles`.

### Task 3: The plugin host routes both kinds

**Files:** Modify `crates/plugin-host/src/host.rs`, `examples/plugins/flag/tests/process.rs`.

**Interfaces:** `PluginHost` implements the new `ContentProvider`. It remembers each opened model's `ContentKind` (by `(plugin, ModelId)`) to decode its frames with `decode_content_frame`, and keeps it across a restart of the process (re-opening gives the same kind). `host.rs` has no tests of its own; processes are tested from the samples' `tests/process.rs`, so the scene path is tested by Task 8's turntable test.

- [ ] **Step 1:** In the flag's `tests/process.rs`, add `flag_opens_as_meshes_without_notices` (through `PluginHost`: `Opened.content` is `ContentModel::Meshes`, `notices` empty).
- [ ] **Step 2:** Run `cargo test -p celesta-plugin-flag` → fail (or not compile).
- [ ] **Step 3:** Implement the routing.
- [ ] **Step 4:** `cargo test -p celesta-plugin-flag` passes.
- [ ] **Step 5:** Commit `feat(plugin-host): route scene content and notices`.

### Task 4: Loading and validating a scene in the GPU renderer

**Files:** Create `crates/gpu-renderer/src/scene.rs` (model upload, validation), `crates/gpu-renderer/src/tests/scene.rs`; modify `crates/gpu-renderer/src/mesh.rs` (`LoadedModel` becomes an enum over the 2D model and `LoadedScene`; `plugin_texture` dispatches on it), `crates/gpu-renderer/src/tests.rs` (add `mod scene;`).

**Interfaces — Produces:** `pub(crate) struct LoadedScene` (GPU buffers per geometry: positions, normals, uvs, tangents, joints, weights, morph deltas, indices; textures per `SceneTexture` with their own `wgpu::Sampler`); `fn invalid_scene(model: &SceneModel) -> Option<String>`; `fn invalid_scene_frame(model: &SceneModel, frame: &SceneFrame) -> Option<String>`; errors surface as `GpuRenderError::InvalidPluginContent { layer, message }`.

- Textures: `Srgb` uploads as `Rgba8UnormSrgb`, `Linear` as `Rgba8Unorm`; straight alpha (no premultiplying, unlike 2D meshes); mip levels built on the CPU with a 2×2 box filter averaged in linear light for sRGB textures, when `sampler.mipmaps`.
- Model checks (`invalid_scene`): canvas positive; every stream length equals `positions.len()`; indices `< vertices` and a multiple of 3; `joints`/`weights` both present or both absent; each target's positions length equals vertices and normals empty or equal; texture/material/geometry/skin indices in range; a draw with `skin` has a skinned geometry and vice versa; `alphaCutoff` finite.
- Frame checks (`invalid_scene_frame`): every `index` in range and listed at most once; `morph_weights.len()` equals the geometry's targets; `skins.len()` equals the model's skins and each has exactly `joints` matrices; joint indices in the geometry `< joints` of its draw's skin (checked once at load per draw); all floats finite; camera valid (`0 < near < far`, `0 < yfov < π`, `height > 0`, view invertible).

- [ ] **Step 1:** In `tests/scene.rs`, a provider `Scenes` returning a scene model built per test; tests `scene_with_bad_index_is_invalid`, `scene_with_wrong_weight_count_is_invalid`, `scene_with_short_skin_is_invalid`, `scene_with_joint_out_of_range_is_invalid`, `scene_with_bad_camera_is_invalid` (each: `renderer.render(&scene)` is `Err(GpuRenderError::InvalidPluginContent { message, .. })` with `message` containing the offending thing's name, e.g. `"draw 3"`, `"weights"`, `"skin 0"`, `"joint"`, `"near"`).
- [ ] **Step 2:** `cargo test -p celesta-gpu-renderer scene` → fail.
- [ ] **Step 3:** Implement upload and validation; drawing returns a transparent canvas for now.
- [ ] **Step 4:** `cargo test -p celesta-gpu-renderer scene` passes; `cargo test -p celesta-gpu-renderer plugin` still passes.
- [ ] **Step 5:** Commit `feat(gpu-renderer): load and validate scene content`.

### Task 5: Drawing — skin and morph, depth, MSAA, unlit, camera, conversion

**Files:** Modify `crates/gpu-renderer/src/scene.rs`; create `crates/gpu-renderer/src/scene_skin.wgsl` (compute), `crates/gpu-renderer/src/scene.wgsl` (unlit + resolve-convert); tests in `tests/scene.rs`.

**Decisions:**
- Compute pass, one dispatch per visible draw: `out = Σ wᵢ·skinᵢ·(p + Σ wₜ·Δpₜ)` for positions, the same with the upper 3×3 for normals and tangents' xyz (renormalised; tangent w kept); unskinned draws copy morphed values. Only non-zero morph weights are passed (index, weight pairs).
- Vertex shader: `clip = projection × view × world × out`; perspective is right-handed with depth 0..1 (`glam`-style `perspective_rh` with near/far); orthographic `height` is the visible height in metres, width by the canvas aspect.
- Culling: back faces (counter-clockwise front, glTF) unless `double_sided`; with a world matrix whose determinant is negative, the front face flips.
- Unlit fragment: `color = base_color × texture(uv')` with `uv' = T(offset) · R(rotation) · S(scale) · uv` (overrides replace `offset`/`scale` when set; `base_color` override replaces the factor).
- Depth test `Less`, cleared to 1.
- Resolve, then a full-screen pass converts: un-premultiply, encode linear → sRGB, premultiply, write `Rgba8Unorm`.
- Nothing about a frame lives on the GPU between frames except the model's buffers.

- [ ] **Step 1:** Tests (64×64 canvas, transparent background, `pixel`/`near` helpers like `tests/plugin.rs`):
  - `nearer_quad_covers_farther_in_either_order`: red quad at z = −2, green at z = −4, camera at origin looking down −Z; centre pixel red whether red is listed first or last.
  - `orthographic_maps_height_to_canvas`: a 1 m × 1 m white quad with `Orthographic { height: 2.0 }` covers the central 32×32 pixels (pixel (31,31) white, (8,8) transparent).
  - `skinning_moves_vertices`: a quad skinned to one joint whose matrix translates +0.5 m in x moves its left edge from x=16 to x=32 (orthographic height 2).
  - `morph_weight_moves_vertices`: a target moving all vertices +0.5 m in x, weight 1.0, gives the same pixels as the skinned test.
  - `srgb_texture_round_trips`: a uniform sRGB texel (128, 64, 200, 255), unlit, full canvas: output within 2 of (128, 64, 200, 255).
  - `back_faces_are_culled_unless_double_sided`.
  - `hidden_draws_are_not_drawn`.
  - `edges_are_antialiased_at_density_two`: a quad edge at a non-integer pixel has an intermediate alpha when rendered with the layer scaled ×2.
  - `frames_repeat_exactly`: rendering time 1, 0, 1 gives identical pixels for both time-1 renders.
- [ ] **Step 2:** `cargo test -p celesta-gpu-renderer scene` → the new tests fail.
- [ ] **Step 3:** Implement the passes.
- [ ] **Step 4:** `cargo test -p celesta-gpu-renderer scene` passes.
- [ ] **Step 5:** Commit `feat(gpu-renderer): draw scene content with skinning, depth, and MSAA`.

### Task 6: Alpha modes and queue order

**Files:** Modify `crates/gpu-renderer/src/scene.rs`, `scene.wgsl`; tests in `tests/scene.rs`.

- Order: queue (`opaque` 2000, `mask` 2450, `blend` 3000), then for `blend` farthest first by view-space depth of `center` (given, or `world` × rest bounds centre), then draw index.
- `mask`: alpha-to-coverage with the sharpened alpha; output alpha 1; depth write on. `blend`: blend state above, depth write off, depth test on. `opaque`: no blend, depth write on, alpha 1.

- [ ] **Step 1:** Tests:
  - `mask_cuts_at_cutoff`: texture alpha 0.3 on the left half, 0.8 on the right, cutoff 0.5 → left transparent, right opaque.
  - `blend_draws_back_to_front_whatever_the_list_order`: two half-transparent quads (red near, blue far) listed near-first → centre is red over blue: red channel > blue channel, alpha ≈ 0.75 × 255.
  - `opaque_draws_before_blend_even_if_listed_after`: a transparent quad in front of an opaque one listed later still shows the opaque behind it.
  - `center_overrides_sorting`: swapping `center`s flips the result of the previous test.
- [ ] **Step 2:** Run → fail. **Step 3:** Implement. **Step 4:** `cargo test -p celesta-gpu-renderer scene` passes.
- [ ] **Step 5:** Commit `feat(gpu-renderer): order scene draws by queue and blend them like UniVRM`.

### Task 7: Notices reach the exporter and the editor

**Files:** Modify `crates/gpu-renderer/src/renderer.rs`, `mesh.rs` (store notices on the loaded model, record them per frame for each plugin layer drawn), `crates/exporter/src/render.rs` (report as `ExportProgress::Warning`, each once), `crates/editor/src/preview.rs` (append to `warnings`); tests in `tests/plugin.rs` and the exporter's tests if it has a render-warnings test to extend.

**Interfaces — Produces:** `pub struct PluginNotice { pub layer: String, pub plugin: String, pub level: NoticeLevel, pub message: String }` with `Display` = `"{plugin} (layer {layer}): {message}"`, prefixed `"note: "` for `Info`; `GpuRenderer::plugin_notices(&self) -> &[PluginNotice]` (the last rendered or submitted frame's, each `(plugin, message)` once), cleared where `missing_glyphs` is cleared.

- [ ] **Step 1:** Test `plugin_notices_are_reported_per_frame` (a provider returning one info and one warning notice; after a render, `plugin_notices()` lists both once; after rendering a scene without that layer, none).
- [ ] **Step 2:** Run → fail. **Step 3:** Implement, including the exporter and editor wiring. **Step 4:** `cargo test -p celesta-gpu-renderer notices` passes; `cargo build -p celesta-exporter -p celesta-editor`.
- [ ] **Step 5:** Commit `feat: show plugin notices in the exporter and the editor`.

### Task 8: Sample plugin `turntable`

**Files:** Create `examples/plugins/turntable/` (`Cargo.toml` workspace member `celesta-plugin-turntable`, `src/lib.rs` provider, `src/main.rs` serving it, `plugin.json`, `js/index.ts` exporting `<Turntable>`, `demo.tsx`, `README.md`, `tests/process.rs`); modify the root `Cargo.toml` members, `examples/plugins/build.mjs` (if it needs to know the sample), `examples/plugins/README.md`.

- The provider: no assets; a unit cube (24 vertices, per-face UVs) with an inline 2×2 sRGB checker texture, unlit, plus a half-transparent blue quad in front of it (`blend`) and a cutout leaf quad (`mask`); perspective camera at (0, 1.2, 3) looking at the origin, `yfov` 0.8; the cube turns `data.speed` (default 0.5) turns per second about +Y: `world` = rotation(time). Canvas 640×360.
- `<Turntable speed? />` emits the plugin layer through `registerHostElement` like `<Flag>`.
- The process test starts the built program through `PluginHost` from a plugin directory (as the flag's does) and checks: answers for times 2, 0, 2 equal the in-process provider's; the GPU renderer draws it (the cube's centre pixel is not transparent).

- [ ] **Step 1:** Write `tests/process.rs`. **Step 2:** `cargo test -p celesta-plugin-turntable` → fail. **Step 3:** Implement the sample. **Step 4:** Tests pass; `node examples/plugins/build.mjs turntable` builds `examples/plugins/dist/turntable`; `cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx <out>.mp4 --plugin-dir examples/plugins/dist` exports, and a still (`--frame 30`) shows the cube with depth (save it for the PR).
- [ ] **Step 5:** Commit `feat(examples): a turntable sample plugin drawing scene content`.

### Task 9: Docs

**Files:** Create `docs/handoff/2026-11-03-plugin-scene-content.md` (specification, what landed, checks — the format of `2026-11-01-plugin-system.md`); modify `docs/handoff/2026-11-01-plugin-system.md` (point to the new note in its "Later notes" paragraph), `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md` (the "Write a plugin" and "Samples" sections: the scene kind, notices, the turntable sample), `skills/celesta/references/plugins.md` (the same, where it describes the protocol or samples).

- [ ] **Step 1:** Write the docs. **Step 2:** `pnpm --dir packages/website run build` (or the website's check command in its `package.json`) passes if the guide has one.
- [ ] **Step 3:** Commit `docs: scene content for plugins`.

### Finish

- [ ] Full checks once: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p celesta-plugin -p celesta-plugin-host -p celesta-gpu-renderer -p celesta-plugin-flag -p celesta-plugin-turntable`, `pnpm -C packages/cli test` (plugin resolution unaffected).
- [ ] Push the branch and open the PR on top of #655 (`gh stack link 647 <branch>`), with the turntable still as the screenshot (there is no "before": 3D content did not exist; say so).
