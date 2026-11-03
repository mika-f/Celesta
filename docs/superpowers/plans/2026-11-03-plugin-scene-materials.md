# Plugin scene materials (phase 2, reworked) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Celesta's 3D scene content stays generic: a plugin brings its own materials as WGSL written against Celesta's material interface, with a parameter block it packs and texture slots it fills, and Celesta draws them with its lights, render queues, alpha modes, outline pass, and resource cache. MToon leaves Celesta: it becomes the VRM plugin's material. Celesta keeps one built-in material, `unlit`.

**Architecture:** `celesta-plugin` gains `SceneModel::shaders` (`SceneShader`: WGSL and texture slots), `MaterialKind::Custom(CustomMaterial)` (a shader index, parameter bytes, texture slots, an optional `OutlinePass`, `requires_tangents`), and `DrawState::params` (a draw's parameter bytes for one frame); `AlphaMode::Blend` carries `depth_write` and `queue_offset` for every material. Parameter bytes travel in the message payload. `celesta-gpu-renderer`'s new `scene_material.rs` assembles a material module (`scene_material_prelude.wgsl` + the plugin's WGSL + `scene_material_entry.wgsl` and, when the material has one, `scene_material_outline_entry.wgsl`), checks it with naga as `shader.rs` checks custom shaders, and compiles it once per source. Every material, the built-in `unlit` (`scene_unlit.wgsl`) included, is drawn through that interface: group 0 the frame (camera, lights, SH ambient, time), group 1 the draw (world, normal matrix, handedness, alpha cutoff) and the material's parameters at two dynamic offsets, group 2 the material's texture slots. Before MToon leaves, Task 3 rewrites Celesta's built-in MToon against the same interface, so the whole phase-2 MToon test suite proves the interface carries MToon; Task 9 deletes it, and the VRM plugin takes that WGSL and those tests (Appendix).

**Tech Stack:** Rust 2024, wgpu 30 (WGSL), naga 30 (`wgsl-in`), serde.

**Spec:** `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`, as amended in Task 10 (Decisions taken → Celesta draws; Where the pieces live; Protocol → Model `shaders`/`materials`, Frame `draws`; Rendering in Celesta → Per frame, Materials; The VRM plugin → MToon; Checks; Phases). Phase 1: `docs/handoff/2026-11-03-plugin-scene-content.md`. This plan reworks the phase-2 branch (`claude/plugin-scene-mtoon`, PR #659) with new commits on top of it.

## Global Constraints

- The protocol stays version 1 and changes in place; `celesta-plugin` keeps its dependencies (serde, serde_json, thiserror, libc on Unix).
- Scene conventions, targets, and blending are unchanged: right-handed, +Y up, metres; matrices `f32` column-major; UV origin top-left; colours linear; color `Rgba16Float` and depth `Depth32Float`, 4× MSAA, cleared to transparent black and 1; resolved and converted to premultiplied sRGB-encoded `Rgba8Unorm`; colour blends `SrcAlpha, OneMinusSrcAlpha`, alpha `One, OneMinusSrcAlpha`; depth compare `LessEqual` for every pipeline.
- Queues, for every material kind: `opaque` 2000; `mask` 2450; `blend` with `depthWrite` 2501 + clamp(`queueOffset`, 0, 9), depth written; `blend` 3000 + clamp(`queueOffset`, −9, 0), depth not written. Opaque and mask have no offset. Queue > 2500 sorts back to front (perspective: squared distance from the eye to `center` or the rest-bounds centre moved by `world`; orthographic: view-space z); ties and lower queues by draw index.
- Every blended fragment whose alpha is below 2⁻¹⁰ (`0.0009765625`) is discarded, for every material, `unlit` included; mask turns the material's alpha into sample coverage `(a − cutoff) / max(fwidth(a), 1e-5) + 0.5` over 4 samples; opaque writes alpha 1.
- Limits (`pub const` in `celesta_plugin::scene`, checked by the GPU renderer): `MAX_SCENE_SHADERS = 32` shaders a model; `MAX_SHADER_BYTES = 262_144` bytes of WGSL a shader; `MAX_MATERIAL_TEXTURES = 12` texture slots a shader (WebGPU's default 16 sampled textures and 16 samplers a stage, 4 kept for Celesta's own later use, e.g. a shadow map); `MAX_MATERIAL_PARAMS = 512` bytes of `Params`. `MAX_DIRECTIONAL_LIGHTS` stays 4.
- Bindings Celesta fixes (a material binds nothing of its own): group 0 binding 0 `frame: Frame` (512 bytes, `VERTEX | FRAGMENT`); group 1 binding 0 `draw: Draw` (128 bytes) and binding 1 `params: Params` (bound 512 bytes, zero-padded), both dynamic, `VERTEX | FRAGMENT`; group 2 binding `2k` `texture_k: texture_2d<f32>` (filterable float) and `2k + 1` `sampler_k: sampler` (filtering) for each slot `k`, `VERTEX | FRAGMENT`. A draw's slot in the draw buffer is `aligned(128) + aligned(512)` bytes (768 at the usual 256-byte alignment): `draw` at its start, `params` at `aligned(128)`.
- Vertex buffers for every material: buffer 0 the skinned vertex (stride 48: position `@location(0)` at 0, normal `@location(1)` at 16, tangent `@location(2)` at 32, all `Float32x4`), buffer 1 `uv0` (`@location(3)`, `Float32x2`, stride 8).
- Names starting with `celesta` belong to Celesta in a material's WGSL (`shader.rs`'s rule). No VRM, UniVRM, or MToon name or concept remains in `crates/plugin` or `crates/gpu-renderer` (code, comments, docs, tests) once Task 9 lands; Unity's conventions (render queue numbers, `ShadeSH9` order, `ZTest LEqual`, `Offset 1, 1`, pixel light count) stay as the origin of Celesta's own rules.
- Shader modules are made with `Device::create_shader_module` from the naga module (runtime bounds checks and loop bounding on, `ShaderRuntimeChecks::checked()`), never `create_shader_module_trusted`. Module and pipeline creation run inside `device.push_error_scope(wgpu::ErrorFilter::Validation)`; a popped error is `InvalidPluginContent`, never a panic.
- Frames stay deterministic: nothing a frame draws depends on an earlier frame, parameter slots are written in full (zero-padded) every frame, and the shader and pipeline caches only save work.
- The CPU reference renderer and `@celesta/web` keep refusing plugin content.
- GPU tests return early without an adapter: run every red/green and full run with `env CELESTA_REQUIRE_GPU=1`. Red/green runs only the task's new tests; the full suite runs once at the task's end.
- Each task ends with `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-plugin -p celesta-gpu-renderer -p celesta-plugin-turntable`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` passing, then its own commit. Never amend; tests and commits are separate commands.

## Review Focus

- **Broken or hostile material WGSL** (a parse or validation error, a resource or entry point of its own, a `celesta` name, implicit-LOD sampling in a vertex hook): refused as `InvalidPluginContent` naming the material and the shader, with a line and column in the plugin's own WGSL (or the interface signature when the error is in Celesta's entry points), before any pipeline exists; never a panic, device error, or lost device. Task 2 (checks), Task 5 (`material_broken_wgsl_is_refused_naming_the_material`).
- **Parameter blocks that do not fit their shader** (a model's or a frame's bytes of another length than the shader's `Params`, parameters on an `unlit` draw, unlit overrides on a custom draw): refused naming the draw or material and both sizes; a slot never shows bytes of an earlier frame or another draw. Task 5 (`material_params_must_match_the_shader`), Task 6 (`frame_params_must_match_the_material`).
- **Limits** (WGSL over 262,144 bytes, more than 12 slots, `Params` over 512 bytes, more than 32 shaders, more slots filled than the shader has, a slot naming a missing texture): refused before naga or the GPU sees the source. Task 2 (`limits_are_refused`), Task 5 (`material_limits_are_refused`).
- **A material that needs tangents on a geometry without them** (a plugin that forgets MikkTSpace): `InvalidPluginContent` naming the draw, the material, and "tangents" at load; a material that does not ask reads `tangent` as zero, never NaN. Task 5 (`material_tangents_need_tangent_geometry`, `material_tangents_and_handedness_follow_world`).
- **One shader for many materials, and a broken one re-sent every frame:** a WGSL source is assembled, checked, and compiled once per renderer whatever the number of materials or shader entries naming it; a broken source's error is kept, so a model refused every frame does not recompile it. Task 5 (`material_shaders_compile_once`).

---

### Step 0: The "before" still

- [ ] On the branch head before Task 1 (#659 as it is): `node examples/plugins/build.mjs turntable`, then `cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx turntable-before.png --frame 30 --plugin-dir examples/plugins/dist`. Keep `turntable-before.png` outside the repository for the PR.

### Task 1: Blend depth writes and queue offsets on every material

**Files:**
- Modify: `crates/plugin/src/scene.rs` (`AlphaMode`, `MtoonMaterial` loses two fields; `mtoon_material_json_names` moves `transparentWithZWrite` and `renderQueueOffsetNumber` onto `AlphaMode::Blend { depth_write: true, queue_offset: -3 }` and checks `"depthWrite":true`, `"queueOffset":-3`), `crates/plugin/src/protocol.rs` (`mtoon_scene_model_round_trips` sets the two values on its blend mode instead), `crates/gpu-renderer/src/scene.rs` (`Alpha::of`, `render_queue`, unit test, `mtoon_slots_and_numbers_are_checked` if it sets them), `crates/gpu-renderer/src/scene.wgsl` (`fs_blend` discard), `crates/gpu-renderer/src/tests/scene.rs` (`queue_` tests on unlit materials; `outline_every_alpha_side_and_mode_draws` and every other MToon helper that sets `transparent_with_z_write` or `render_queue_offset_number` sets `AlphaMode::Blend { depth_write, queue_offset }` instead), `examples/plugins/turntable/src/lib.rs` (compile only)

**Interfaces — Produces:**

```rust
#[serde(tag = "alphaMode", rename_all = "camelCase")]
pub enum AlphaMode {
    Opaque,
    Mask { #[serde(rename = "alphaCutoff")] cutoff: f32 },
    Blend {
        #[serde(default, rename = "depthWrite", skip_serializing_if = "std::ops::Not::not")]
        depth_write: bool,
        #[serde(default, rename = "queueOffset", skip_serializing_if = "is_zero")]
        queue_offset: i32,
    },
}
impl AlphaMode { pub const BLEND: Self = Self::Blend { depth_write: false, queue_offset: 0 }; }
// MtoonMaterial loses transparent_with_z_write and render_queue_offset_number.
// gpu-renderer: Alpha { Opaque, Mask, BlendDepthWrite, Blend }; Alpha::of(&AlphaMode); fn render_queue(&AlphaMode) -> u32
```

**Decisions:**
- Depth write and queue offset belong to blending, so they live in `AlphaMode::Blend`: an opaque or mask material cannot carry an offset that would be ignored. `{"alphaMode":"blend"}` still reads as before.
- They apply to every material kind, `unlit` included (correctness over compatibility: an unlit transparent part of a model sorts like the rest).
- `fs_blend` (unlit) gains the 2⁻¹⁰ discard every blended material has.
- `Alpha::BlendZWrite` is renamed `Alpha::BlendDepthWrite`.

- [ ] **Step 1:** Tests:
  - `crates/plugin/src/scene.rs` `blend_json_carries_depth_write_and_queue_offset`: `{"alphaMode":"blend","doubleSided":false,"kind":"unlit","baseColor":[1,1,1,1]}` reads `AlphaMode::BLEND`; `Blend { depth_write: true, queue_offset: -3 }` writes `"alphaMode":"blend","depthWrite":true,"queueOffset":-3` and round-trips; `AlphaMode::BLEND` writes neither key.
  - `crates/gpu-renderer/src/scene.rs` `render_queue_clamps_and_ignores_offsets` (rewritten on `AlphaMode`): `(false, 5)` → 3000, `(false, -3)` → 2997, `(false, -20)` → 2991, `(false, i32::MIN)` → 2991, `(true, -3)` → 2501, `(true, 4)` → 2505, `(true, 600)` → 2510, `(true, i32::MAX)` → 2510; mask → 2450, opaque → 2000.
  - `tests/scene.rs`: the `queue_` tests rebuilt on unlit blended squares: `glowing(emissive, alpha, z_write, offset)` becomes `see_through(color: [f32; 3], alpha, depth_write, queue_offset) -> Material` (unlit, base `[r, g, b, alpha]`), expected pixels unchanged (`RED_OVER_BLUE`, `BLUE_OVER_RED`, `HALF_RED`, `HALF_BLUE`): `queue_offsets_order_blend_draws_before_distance`, `queue_depth_write_blend_draws_first_and_hides_what_is_behind` (renamed from `queue_zwrite_…`), `queue_discards_nearly_clear_blend_pixels`, `queue_perspective_blend_sorts_by_distance`, `queue_orthographic_blend_sorts_by_view_depth`.
- [ ] **Step 2:** `cargo test -p celesta-plugin blend_json` → fails to compile.
- [ ] **Step 3:** Implement; update every `AlphaMode::Blend` pattern and literal (`AlphaMode::BLEND`, `AlphaMode::Blend { .. }`).
- [ ] **Step 4:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer queue_ render_queue` passes; then the task checks.
- [ ] **Step 5:** Commit `refactor(plugins): blend depth writes and queue offsets on every scene material`.

### Task 2: Check material shaders against Celesta's interface

**Files:**
- Create: `crates/gpu-renderer/src/scene_material.rs`, `crates/gpu-renderer/src/scene_material_prelude.wgsl`, `crates/gpu-renderer/src/scene_material_entry.wgsl`, `crates/gpu-renderer/src/scene_material_outline_entry.wgsl`, `crates/gpu-renderer/src/wgsl.rs`
- Modify: `crates/gpu-renderer/src/shader.rs` (uses `wgsl.rs`), `crates/gpu-renderer/src/lib.rs` (modules)

**Interfaces — Produces** (`wgsl.rs`, moved from `shader.rs` and generalised; `shader.rs` passes its own names and hint, its tests unchanged):

```rust
pub(crate) fn locate(code: &str, author: &Range<usize>, at: Option<usize>, message: String,
    before: &str /* hint for errors in the prelude */, after: &str /* hint for errors in the entry points */) -> String;
pub(crate) fn error_chain(error: &dyn std::error::Error) -> String;
pub(crate) fn check_declarations(module: &naga::Module, code: &str, author: &Range<usize>,
    prelude_resource: &dyn Fn(&str) -> bool, entry_points: &[&str], what: &str /* "a custom shader" | "a material" */) -> Result<(), String>;
```

`scene_material.rs`:

```rust
pub(crate) struct CheckedMaterial {
    pub(crate) module: naga::Module,
    pub(crate) params_size: u32,   // naga's layout size of `Params`
    pub(crate) outline: bool,      // defines material_outline_fragment
}
/// Assembles and checks a material's WGSL with `textures` slots. CPU only.
pub(crate) fn check_material(wgsl: &str, textures: usize) -> Result<CheckedMaterial, String>
```

The interface (`scene_material_prelude.wgsl`; slots appended by `check_material` after it as `@group(2) @binding(2k) var texture_k: texture_2d<f32>;` and `@group(2) @binding(2k + 1) var sampler_k: sampler;`):

```wgsl
struct Light { to_light: vec4f, color: vec4f };      // towards the light, length 1; colour × intensity; black when absent
struct Frame {
    view_projection: mat4x4f, view: mat4x4f, projection: mat4x4f,
    eye: vec4f,               // camera position in world; w 1 perspective, 0 orthographic
    forward: vec4f,           // the camera's +Z (towards the viewer) in world
    time: f32,                // the frame's time in seconds, as f32
    height_over_width: f32,   // the canvas's height over its width
    light_count: u32,         // 0 to MAX_LIGHTS
    celesta_reserved: u32,
    lights: array<Light, 4>,  // the main light first
    ambient: array<vec4f, 9>, // RGB of the SH coefficients c0…c8 (Lights::ambient)
};
struct Draw {
    world: mat4x4f,
    normal_matrix: mat3x3f,   // takes model normals to world, up to a positive scale
    handedness: f32,          // −1 when world mirrors, else 1
    alpha_cutoff: f32,        // a mask material's cutoff, else 0
};
struct Vertex {               // one skinned vertex in world space
    position: vec3f,
    normal: vec3f,            // safe_normalize(normal_matrix × skinned normal)
    tangent: vec4f,           // xyz: safe_normalize(world 3×3 × skinned tangent); w: sign(tangent.w) × handedness; all 0 without tangents
    uv: vec2f,                // the geometry's uv0
    clip: vec4f,              // view_projection × position
};
struct VertexOffset { world: vec3f, clip: vec2f };   // added to position before projecting, then to clip.xy
struct Fragment {
    position: vec3f,          // world, after the vertex offset
    normal: vec3f,            // interpolated, renormalised; NOT turned on back faces
    tangent: vec4f,           // xyz renormalised, w interpolated
    uv: vec2f,
    view: vec3f,              // towards the viewer: perspective normalize(eye − position), orthographic forward
    front_facing: bool,
    pixel: vec2f,             // @builtin(position).xy
};
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(1) var<uniform> params: Params;    // the material declares `struct Params`
const MAX_LIGHTS: u32 = 4u;
fn linearstep(start: f32, end: f32, t: f32) -> f32   // saturate((t − start) / (end − start)); a step (0 up to start, 1 above) when end == start
fn ambient_light(n: vec3f) -> vec3f                  // c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)
fn safe_normalize(v: vec3f) -> vec3f                 // v / |v|, or v when |v| is 0
```

What a material defines:

```wgsl
struct Params { … }                                           // required, at least one member, ≤ 512 bytes
fn material_fragment(fragment: Fragment) -> vec4f             // required: straight linear RGB and alpha
fn material_vertex(vertex: Vertex) -> VertexOffset            // optional; Celesta supplies a zero offset
fn material_outline_fragment(fragment: Fragment) -> vec4f     // optional: the shader has an outline pass
fn material_outline_vertex(vertex: Vertex) -> VertexOffset    // optional, only with material_outline_fragment; zero by default
```

Celesta's entry points (`scene_material_entry.wgsl`): `@vertex celesta_surface_vertex`, `@fragment celesta_surface_opaque` (rgb, alpha 1), `celesta_surface_mask` (rgb, alpha 1, `@builtin(sample_mask)` from `celesta_coverage(alpha)`), `celesta_surface_blend` (discard below 2⁻¹⁰ after the call, `(rgb, clamp(a, 0, 1))`); `scene_material_outline_entry.wgsl`: `celesta_outline_vertex`, `celesta_outline_opaque`, `celesta_outline_mask`, `celesta_outline_blend`, the same wrappers around the outline functions. Internal: `CelestaVertexInput`, `CelestaVarying` (`@builtin(position) clip`, locations 0–3: position, normal, tangent, uv), `celesta_vertex`, `celesta_place(vertex, offset)`, `celesta_fragment(varying, front_facing) -> Fragment`, `celesta_coverage`, `CelestaCovered`.

**Decisions:**
- **Assembly, as `shader.rs`'s `assemble`:** prelude, slot declarations, `\n`, the plugin's WGSL (its byte range kept for locating errors), then a default `material_vertex` when the WGSL has none, a default `material_outline_vertex` when it has `material_outline_fragment` but no `material_outline_vertex`, then the surface entry points, then the outline entry points when it has `material_outline_fragment`.
- **Two parses.** The first parses prelude + slots + WGSL alone (the entry points call into the WGSL, not the other way, so it stands alone) and reads which functions exist; the second parses the assembled module and is validated with `ValidationFlags::all()`, `Capabilities::default()`. Errors are located with `locate`: inside the WGSL as `{line}:{column}: {message}`; in the prelude with the hint `(a material declares \`struct Params\`, the layout of its parameters)`; in the entry points with `(a material defines \`fn material_fragment(fragment: Fragment) -> vec4f\` and may define \`fn material_vertex(vertex: Vertex) -> VertexOffset\`, \`fn material_outline_fragment(fragment: Fragment) -> vec4f\`, and \`fn material_outline_vertex(vertex: Vertex) -> VertexOffset\`)`.
- **One function per pass, no pass flag.** A pass flag would be a function argument, which WGSL counts as non-uniform, so a material would branch on a non-uniform value before `textureSample` (derivatives undefined, and refused by WGSL implementations that enforce fragment uniformity). Separate surface and outline functions need no branch. naga 30 does not enforce fragment derivative uniformity (`DISABLE_UNIFORMITY_REQ_FOR_FRAGMENT_STAGE`), so Celesta does not refuse such branching; the note documents the rule.
- **Checks, in order:** `wgsl.len() ≤ MAX_SHADER_BYTES` (`"is {n} bytes of WGSL, more than 262144"`); `textures ≤ MAX_MATERIAL_TEXTURES` (`"has {n} texture slots, more than 12"`); first parse; `check_declarations` (resources allowed only from the prelude: `frame`, `draw`, `params`, `texture_k`, `sampler_k` for `k < textures`; no entry point of its own; no `celesta` name); `material_fragment` present (`"defines no \`fn material_fragment(fragment: Fragment) -> vec4f\`"`); `material_outline_vertex` without `material_outline_fragment` refused (`"defines \`material_outline_vertex\` but no \`material_outline_fragment\`"`); second parse and validation; `Params`' size from `naga::proc::Layouter::update(module.to_ctx())` ≤ 512 (`"Params is {n} bytes, more than 512"`).
- **Textures:** every slot is visible to both stages, so a vertex hook can read one with `textureSampleLevel`; naga refuses implicit-LOD sampling there.
- `Fragment.normal` is not turned toward the viewer: a material that wants that uses `front_facing` (an outline pass culling front faces shades its back faces with their outward normal).

- [ ] **Step 1:** Unit tests in `scene_material.rs` (no adapter):
  - `minimal_material_checks`: `struct Params { color: vec4f }` with `fn material_fragment(fragment: Fragment) -> vec4f { return params.color; }`, 0 slots → `params_size` 16, `outline` false.
  - `params_size_follows_the_wgsl_layout`: `{ a: f32, b: vec3f }` → 32; `{ a: vec4f, b: array<vec4f, 4> }` → 80.
  - `outline_functions_make_an_outline`: with `material_outline_fragment` → `outline` true; `material_outline_vertex` alone → error containing `"no \`material_outline_fragment\`"`.
  - `broken_wgsl_is_located_in_the_plugins_source` (Review Focus): line 3 `  return nope;` → error starting `"3:10:"` and containing `nope`.
  - `wrong_signatures_name_the_interface`: `fn material_fragment(fragment: Fragment) -> vec3f` → error containing `fn material_fragment(fragment: Fragment) -> vec4f`.
  - `missing_parts_are_refused`: no `material_fragment` → `"defines no"`; no `struct Params` → `"struct Params"`.
  - `own_resources_entry_points_and_celesta_names_are_refused` (Review Focus): `@group(3) @binding(0) var<uniform> mine: vec4f;` → `"declares the resource \`mine\`"`; an `@fragment fn fs` → `"declares the entry point \`fs\`"`; `fn celesta_x() {}` → `"belong to Celesta"`.
  - `vertex_hooks_cannot_sample_with_implicit_lod`: 1 slot, `material_outline_vertex` using `textureSample(texture_0, sampler_0, vertex.uv)` → Err; with `textureSampleLevel(…, 0.0)` → Ok.
  - `textures_are_declared_by_slot_count`: 2 slots reading `texture_1` → Ok; reading `texture_2` → Err containing `texture_2`.
  - `limits_are_refused` (Review Focus): 262,145 bytes → `"more than 262144"`; 13 slots → `"more than 12"`; `struct Params { a: array<vec4f, 33> }` → `"528 bytes, more than 512"`.
- [ ] **Step 2:** `cargo test -p celesta-gpu-renderer scene_material` → fails to compile.
- [ ] **Step 3:** Implement. `shader.rs`'s tests pass unchanged.
- [ ] **Step 4:** `cargo test -p celesta-gpu-renderer scene_material shader` passes; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): check scene material shaders against Celesta's interface`.

### Task 3: Draw every material through the interface

The phase-2 test suite is this task's acceptance test: every existing scene test (unlit, MToon, queues, outlines, overrides, resources) passes with its expected pixels unchanged once both built-in materials are drawn through `check_material`'s modules.

**Files:**
- Create: `crates/gpu-renderer/src/scene_unlit.wgsl`
- Modify: `crates/gpu-renderer/src/scene.rs`, `crates/gpu-renderer/src/scene.wgsl` (keeps `vs_full`/`fs_convert` only), `crates/gpu-renderer/src/scene_mtoon.wgsl` (rewritten against the interface), `crates/gpu-renderer/src/mesh.rs` (`LoadedScene::load` returns `prepare`'s message as `InvalidPluginContent`)
- Test: unit tests in `scene.rs`; `tests/scene.rs` unchanged

**Interfaces — Produces:**

```rust
#[repr(C)] struct FrameUniform {   // 512 bytes, WGSL `Frame`
    view_projection: [f32; 16], view: [f32; 16], projection: [f32; 16],
    eye: [f32; 4], forward: [f32; 4],
    time: f32, height_over_width: f32, light_count: u32, reserved: u32,
    lights: [LightUniform; MAX_DIRECTIONAL_LIGHTS], ambient: [[f32; 4]; 9],
}
#[repr(C)] struct DrawUniform {    // 128 bytes, WGSL `Draw`
    world: [f32; 16], normal_matrix: [[f32; 4]; 3], handedness: f32, alpha_cutoff: f32, reserved: [f32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] struct ShaderId(u32);   // 0 unlit, 1 MToon (until Task 9), then plugin shaders in compile order
pub(crate) struct CompiledShader { id: ShaderId, module: wgpu::ShaderModule, params_size: u32, textures: usize, outline: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] enum Pass { Surface, Outline { bias_constant: i32, bias_slope: u32 /* f32 bits */ } }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] struct PipelineKey { shader: ShaderId, pass: Pass, facing: Facing, alpha: Alpha }
impl ScenePipelines {
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, model: &SceneModel) -> Result<Vec<u32> /* params bytes by material */, String>;
}
// ScenePipelines: `unlit`, `mtoon`: Arc<CompiledShader>; `texture_layouts: HashMap<usize, (wgpu::BindGroupLayout, wgpu::PipelineLayout)>` by slot count;
//   `draw: HashMap<PipelineKey, wgpu::RenderPipeline>`; `draw_layout` (group 1: Draw + Params); `view_layout` (group 0)
// LoadedScene gains `params_sizes: Vec<u32>` (by material)
const UNLIT_PARAMS: usize = 48;    // base_color, uv_u, uv_v
const MTOON_PARAMS: usize = 176;
fn unlit_params(unlit: &UnlitMaterial, overrides: &MaterialOverrides) -> [u8; 48]
fn mtoon_params(mtoon: &MtoonMaterial, overrides: &MaterialOverrides, scroll: [f32; 2]) -> [u8; 176]
```

`scene_unlit.wgsl` (1 slot, empty white):

```wgsl
struct Params { base_color: vec4f, uv_u: vec4f, uv_v: vec4f };   // the texture transform's rows
fn material_fragment(fragment: Fragment) -> vec4f {
    let point = vec3f(fragment.uv, 1.0);
    let uv = vec2f(dot(params.uv_u.xyz, point), dot(params.uv_v.xyz, point));
    return params.base_color * textureSample(texture_0, sampler_0, uv);
}
```

The built-in MToon's `Params` (9 slots, the `MTOON_SLOTS` order and empty texels as before):

```wgsl
struct Params {
    base_color: vec4f,        //   0
    uv_u: vec4f, uv_v: vec4f, //  16, 32: the base colour texture transform's rows (overrides applied)
    shade: vec4f,             //  48: shade colour, shading shift factor
    emissive: vec4f,          //  64: emissive factor, shading shift texture scale
    matcap: vec4f,            //  80: matcap factor, toony
    rim: vec4f,               //  96: parametric rim colour, rim lighting mix
    rim_shape: vec4f,         // 112: fresnel power, rim lift, GI equalization, normal texture scale
    outline: vec4f,           // 128: outline colour, outline lighting mix
    outline_width: vec4f,     // 144: width factor, mode (0 none, 1 world, 2 screen), 0, 0
    uv_animation: vec4f,      // 160: scroll x, scroll y at the frame's time (uv_scroll), rotation in turns a second, 1 with a normal texture
};                            // 176 bytes
```

**Decisions:**
- **Port of `scene_mtoon.wgsl`:** `material_fragment` is `mtoon_surface` + `mtoon_color(surface, false)` returning `(color, lit.a)`, its normal `select(-fragment.normal, fragment.normal, fragment.front_facing)`; `material_outline_fragment` shades with the unturned `fragment.normal` and `mtoon_color(surface, true)`; `material_outline_vertex` returns `VertexOffset(normal × width, 0)` in world mode and `VertexOffset(0, offset × facing)` in screen mode, `offset` from `vertex.clip` and `frame.height_over_width` as before; the width texture is `textureSampleLevel(texture_7, sampler_7, vertex.uv, 0.0).g`. `frame.params.x/y/z` become `frame.time`, `frame.height_over_width`, `frame.light_count`; `draw.outline_width.z` becomes `draw.handedness` (already in `Vertex.tangent.w`); the mask cutoff is `draw.alpha_cutoff`; coverage, the blend discard, and opaque's alpha 1 move to the entry points. The texture transform is applied per fragment for both built-ins (it is affine, so the result is the vertex one up to rounding).
- **Compiled through `check_material`** at `ScenePipelines::new`, `.expect("the built-in materials pass the material checks")`; they keep `ShaderId` 0 and 1.
- **Pipelines** are made in `prepare` per material, inside one error scope per material: the surface pass and, for a material with an outline, the outline pass (`Cull::Front`, bias constant 1, slope 1.0: Unity's `Offset 1, 1`), each in both windings. A popped error is `"material {m}'s pipelines: {error}"` and nothing of that material is cached.
- **Per draw:** `DrawUniform` from `world` (normal matrix and handedness as phase 2 computes them) and the mask cutoff; the parameter slot holds `unlit_params`, `mtoon_params` (with `uv_scroll` at the frame's time), zero-padded to 512.
- `oversized_uniforms` counts a draw as `aligned(128) + aligned(512)` bytes of draw uniforms.

- [ ] **Step 1:** Tests in `scene.rs`'s `mod tests`:
  - `frame_and_draw_uniforms_match_the_prelude`: `check_material` on `scene_unlit.wgsl`; with `Layouter`, the prelude's `Frame` is 512 bytes with `light_count` at 232 and `ambient` at 368, and `Draw` is 128 bytes with `alpha_cutoff` at 116; `size_of::<FrameUniform>() == 512`, `size_of::<DrawUniform>() == 128`.
  - `built_in_materials_pass_the_material_checks`: unlit → `params_size` 48, no outline; MToon (9 slots) → 176, outline.
  - `unlit_params_pack_colour_and_transform_rows`: base `[1, 0.5, 0.25, 1]`, transform offset (0.5, 0) → bytes 0..16 the colour, 16..32 `[1, 0, 0.5, 0]`, 32..48 `[0, 1, 0, 0]` (f32 little-endian); a `texture_offset` override `[0.25, 0]` gives `uv_u.z` 0.25.
- [ ] **Step 2:** `cargo test -p celesta-gpu-renderer frame_and_draw_uniforms built_in_materials unlit_params` → fails to compile.
- [ ] **Step 3:** Implement. Run the whole scene suite while porting: `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer scene` must pass with no expected pixel changed.
- [ ] **Step 4:** The new tests and the scene suite pass; then the task checks.
- [ ] **Step 5:** Commit `refactor(gpu-renderer): draw scene materials through a WGSL material interface`.

### Task 4: Custom materials and their shaders on the wire

**Files:**
- Modify: `crates/plugin/src/scene.rs` (types, docs, tests), `crates/plugin/src/lib.rs` (re-exports), `crates/plugin/src/protocol.rs` (header, payload, tests)
- Modify (compile only): `crates/gpu-renderer/src/scene.rs` (`invalid_scene` refuses `custom` with `"material {m} is custom, which is not drawn yet"` until Task 5, and any draw `params` with `"draw {i} has parameters, which are not drawn yet"`), `crates/gpu-renderer/src/tests/scene.rs` and `examples/plugins/turntable/src/lib.rs` (`shaders: Vec::new()`, `params: None`)

**Interfaces — Produces** (`pub`, re-exported, `Clone, Debug, PartialEq, Serialize, Deserialize`, `rename_all = "camelCase"`):

```rust
pub struct SceneModel { /* … */ pub shaders: Vec<SceneShader> /* header "shaders", default [] */ }
#[derive(Eq, Hash)] pub struct SceneShader {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub name: Option<String>,   // for messages
    pub wgsl: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub textures: Vec<ShaderTexture>,   // slot k is texture_k / sampler_k
}
#[derive(Eq, Hash)] pub struct ShaderTexture {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(default)] pub empty: EmptyTexel,          // what an empty slot samples
}
#[derive(Copy, Default, Eq, Hash)] pub enum EmptyTexel { #[default] White, Black }   // "white" | "black": 1×1 (255,255,255,255) / (0,0,0,255)
pub enum MaterialKind { Unlit(UnlitMaterial), Mtoon(MtoonMaterial) /* until Task 9 */, Custom(CustomMaterial) }   // "kind": "custom"
pub struct CustomMaterial {
    pub shader: usize,                                 // SceneModel::shaders index
    #[serde(skip)] pub params: Vec<u8>,                // the shader's Params, little-endian WGSL uniform layout; travels in the payload
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub textures: Vec<Option<usize>>,   // slot k's SceneModel::textures entry; missing or null slots are empty
    #[serde(default, skip_serializing_if = "Option::is_none")] pub outline: Option<OutlinePass>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub requires_tangents: bool,
}
#[derive(Copy)] pub struct OutlinePass {
    #[serde(default = "CullFace::front")] pub cull: CullFace,
    #[serde(default)] pub depth_bias: DepthBias,
}   // Default: cull Front, bias (1, 1.0)
#[derive(Copy, Eq, Hash)] pub enum CullFace { None, Front, Back }                   // "none" | "front" | "back"
#[derive(Copy)] pub struct DepthBias { pub constant: i32, pub slope_scale: f32 }    // Default { 1, 1.0 }: Unity's Offset 1, 1
pub struct DrawState { /* … */ pub params: Option<Vec<u8>> }   // replaces the material's params for this frame
pub const MAX_SCENE_SHADERS: usize = 32; pub const MAX_SHADER_BYTES: usize = 262_144;
pub const MAX_MATERIAL_TEXTURES: usize = 12; pub const MAX_MATERIAL_PARAMS: usize = 512;
```

Wire: `SceneOpenedHeader` gains `shaders: Vec<SceneShader>` and `materialParams: Vec<u32>` (bytes of each material's parameters, `[]` when no material has any); the bytes follow the textures' pixels in the payload, material by material. `DrawStateHeader` gains `params: Option<u32>` (bytes, omitted for `None`); the bytes follow that draw's morph weights.

**Decisions:**
- Parameter bytes are bytes on the wire, never JSON numbers; `Material`'s own serde leaves them out, and `encode_opened`/`decode_opened` carry them.
- A `materialParams` whose length is neither 0 nor the material count, or a non-zero count for a non-custom material, is `ProtocolError::Malformed` (`"material {i} is not custom but has {n} parameter bytes"`). Lengths are otherwise the renderer's to check.
- Doc comments state the interface's contract on each type (what `params` must match, slot `k`'s names, the outline functions an `outline` needs, that `requires_tangents` makes a draw on a geometry without tangents invalid), pointing at the handoff note for the WGSL.

- [ ] **Step 1:** Tests:
  - `scene.rs` `custom_material_json_names`: `CustomMaterial { shader: 1, params: vec![1, 2, 3, 4], textures: vec![Some(0), None], outline: Some(OutlinePass::default()), requires_tangents: true }` writes `"kind":"custom"`, `"shader":1`, `"textures":[0,null]`, `"outline":{"cull":"front","depthBias":{"constant":1,"slopeScale":1.0}}`, `"requiresTangents":true`, no `params`; reads back equal with `params` empty. `{"alphaMode":"opaque","doubleSided":false,"kind":"custom","shader":0}` reads with no textures, no outline, `requires_tangents` false. `{"outline":{}}` reads `OutlinePass::default()`.
  - `scene_shader_json_names`: `{"wgsl":"x","textures":[{"empty":"black"},{}]}` reads `[Black, White]`, names `None`.
  - `protocol.rs` `custom_scene_model_round_trips`: two shaders, a 16-byte and a 32-byte custom material and an unlit one, an inline texture: `decode_opened(encode_opened(…))` equals the model, parameters included; the header's `materialParams` is `[16, 32, 0]`.
  - `draw_params_round_trip`: draw 0 with 32 bytes and two morph weights, draw 1 without: round-trips; draw 1's header has no `params` key.
  - `material_params_that_do_not_fit_are_malformed`: `materialParams` of length 3 for 2 materials, and `[4]` for an unlit material → `ProtocolError::Malformed`.
- [ ] **Step 2:** `cargo test -p celesta-plugin custom scene_shader draw_params material_params` → fails to compile.
- [ ] **Step 3:** Implement; update the compile-only files.
- [ ] **Step 4:** `cargo test -p celesta-plugin` passes; `cargo build --workspace --all-targets`; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): custom scene materials and their WGSL shaders on the wire`.

### Task 5: Draw custom materials

**Files:**
- Modify: `crates/gpu-renderer/src/scene.rs` (`invalid_scene`, `prepare`, the shader cache, material bind groups), `crates/gpu-renderer/src/mesh.rs` (load error path)
- Test: `crates/gpu-renderer/src/tests/scene.rs`

**Interfaces — Produces:**

```rust
impl ScenePipelines {
    /// The compiled module of `shader`, from the cache or checked and compiled now.
    fn shader(&mut self, device: &wgpu::Device, shader: &SceneShader) -> Result<Arc<CompiledShader>, String>;
}
// ScenePipelines gains `shaders: HashMap<(String, usize), Result<Arc<CompiledShader>, String>>` (by WGSL and slot count),
//   `next_shader: u32`, `#[cfg(test)] pub(crate) compiles: usize` (plugin shaders checked, cache misses only)
```

Test helpers (`tests/scene.rs`): `fn shader(wgsl: &str, textures: &[EmptyTexel]) -> SceneShader`, `fn custom(shader: usize, params: Vec<u8>) -> Material` (opaque, single-sided), `fn bytes(values: &[f32]) -> Vec<u8>` (little-endian), `fn custom_square(shaders: Vec<SceneShader>, material: Material) -> Scenes` (`one_square()` with them), and these materials:

```rust
const FLAT: &str = "struct Params { color: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f { return params.color; }";
const LAMBERT: &str = "struct Params { albedo: vec4f };   // N·L under the main light, plus ambient
fn material_fragment(fragment: Fragment) -> vec4f {
    let light = frame.lights[0];
    let lit = light.color.rgb * max(dot(fragment.normal, light.to_light.xyz), 0.0) + ambient_light(fragment.normal);
    return vec4f(params.albedo.rgb * lit, 1.0);
}";
const SLOTS: &str = "struct Params { slot: u32 };   // 3 slots: white, black, white
fn material_fragment(fragment: Fragment) -> vec4f {
    let a = textureSample(texture_0, sampler_0, fragment.uv);
    let b = textureSample(texture_1, sampler_1, fragment.uv);
    let c = textureSample(texture_2, sampler_2, fragment.uv);
    return select(select(c, b, params.slot == 1u), a, params.slot == 0u);
}";
const PROBE: &str = /* `struct Params { mode: u32, p: vec4f }` (32 bytes: mode at 0, p at 16), every result computed, then
    picked with `select` by mode: 0 (0.5 + 0.5 tangent.w, 0.5 + 0.5 draw.handedness, 0, 1);
    1 front_facing ? red : blue; 2 (0.5 + 0.5 view.x, 0, 0, 1);
    3 (linearstep(p.x, p.y, p.z), length(safe_normalize(vec3f(0.0))), 0, 1) */;
fn probe(mode: u32, p: [f32; 4]) -> Vec<u8>   // its 32 parameter bytes
```

**Decisions:**
- **`invalid_scene` (before naga or the GPU):** at most 32 shaders (`"the scene has {n} shaders, more than 32"`); each shader's WGSL ≤ 262,144 bytes and ≤ 12 slots (`"shader {s} ({name}) …"`, the name only when given); a custom material's `shader` in range (`"material {m} uses shader {s}, of {n}"`), its `textures` no longer than the shader's slots (`"material {m} fills {a} texture slots, but shader {s} has {b}"`), each filled slot's texture in range (`"material {m}'s texture slot {k} ({name}) uses texture {t}, of {n}"`), an outline's `slope_scale` finite (`"material {m}'s outline depth bias is not finite"`); a draw whose material has `requires_tangents` on a geometry without tangents (`"draw {d}'s material {m} requires tangents, but geometry {g} has none"`).
- **`prepare` (compiles):** every shader the model lists, used or not, through `shader()`; an error is `"material {m}'s shader {s} ({name}): {error}"` for the first material using it, else `"shader {s} ({name}): {error}"`. Then each custom material's `params.len()` must equal the shader's `params_size` (`"material {m}'s parameters are {a} bytes, but shader {s}'s Params is {b}"`). A custom `outline` is refused until Task 7 (`"material {m}'s outline is not drawn yet"`).
- **The cache** is keyed by `(wgsl, slots)` (names and empty texels do not change the module), holds failures too, and is never evicted (as phase 2's pipelines are not). `ShaderId`s are handed out in compile order after the built-ins. A failed `shader()` leaves no `ShaderId` consumed.
- **Bind groups:** a custom material's group 2 is made at load: slot `k` binds its texture with that texture's own sampler, an empty slot `EmptyTexel`'s 1×1 texel with the default sampler.
- **Per draw:** the parameter slot holds the material's `params`, zero-padded to 512.
- Celesta reads nothing of the parameter bytes; a material that computes NaN draws whatever the GPU makes of it, in its own layer only.

- [ ] **Step 1:** Tests (prefix `material_`; 64×64 `one_square()` seen orthographically from +Z unless said; expected bytes `srgb8(linear)` within `near`'s 2):
  - `material_params_colour_the_surface`: `FLAT` with `(0.5, 0.25, 1, 1)` → `[188, 137, 255, 255]`.
  - `material_texture_slots_sample_their_texture_or_their_empty_texel`: `SLOTS` with slot 0 = `texel([255, 0, 0, 255], Srgb)`, slots 1 and 2 empty: `slot` 0 → `[255, 0, 0, 255]`, 1 → `[0, 0, 0, 255]`, 2 → `WHITE`; slot 1 filled with the same texture → red.
  - `material_lights_and_ambient_reach_the_shader`: `LAMBERT` albedo white, light `direction (0, −0.8660254, −0.5)` white ×1 (N·L 0.5), `ambient[3] = [0.25; 3]` → 225; no lights → 137.
  - `material_ambient_follows_the_sh_polynomial`: normal (0.48, 0.6, 0.64) on every vertex, no lights; for each k only `ambient[k] = [0.5; 3]` (`[-0.5; 3]` for k = 8) → greys `[134, 149, 153, 188, 106, 121, 125, 109, 72]`.
  - `material_normals_follow_the_inverse_transpose_of_world`: normal (0.6, 0.8, 0), `world = scaling(2, 1, 1)`, light `direction (0, −1, 0)` → N·L 0.9363 → 248 (through `world` itself: 196).
  - `material_tangents_and_handedness_follow_world`: `PROBE` mode 0 on a square with tangents `[1, 0, 0, 1]`: identity → `[255, 255, 0, 255]`; `scaling(-1, 1, 1)` → `[0, 0, 0, 255]`; without tangents → `[188, 255, 0, 255]`.
  - `material_fragments_know_their_facing`: `PROBE` mode 1, double-sided: identity → red; `scaling(-1, 1, -1)` (the back shows) → blue.
  - `material_view_points_to_the_viewer`: `PROBE` mode 2: orthographic → 188 everywhere covered; perspective `yfov` 1 from 2 m → pixel (46, 32) 165 (V's x −0.24).
  - `material_helpers_follow_their_formulas`: `PROBE` mode 3: `(0.5, 0.5, 0.5)` → red 0; `(0.5, 0.5, 0.6)` → 255; `(0, 1, 0.5)` → 188; `(1, 0, 0.25)` → 225; green always 0.
  - `material_alpha_modes_wrap_the_returned_alpha`: `FLAT` `(1, 0, 0, 0.5)`: opaque → `[255, 0, 0, 255]`; blend → `HALF_RED`; mask cutoff 0.4 → `[255, 0, 0, 255]`, cutoff 0.6 → `CLEAR`; blend with `depth_write` and alpha 0.0005 in front of a blended blue square → `HALF_BLUE`.
  - `material_degenerate_worlds_draw_without_nan`: `LAMBERT`, `world = scaling(0, 0, 0)` → renders, (32, 32) `CLEAR`.
  - `material_broken_wgsl_is_refused_naming_the_material` (Review Focus): `FLAT` with line 2 returning `nope` → message contains `"material 0's shader 0"` and `"2:"`; a shader with a `@group(3)` resource → `"declares the resource"`; an unused broken shader beside a good one → `"shader 1"`.
  - `material_params_must_match_the_shader` (Review Focus): `FLAT` with 12 bytes → `"material 0's parameters are 12 bytes, but shader 0's Params is 16"`; with none → `"are 0 bytes"`.
  - `material_limits_are_refused` (Review Focus): 33 shaders → `"33 shaders, more than 32"`; 2 slots filled on `FLAT` (0 slots) → `"fills 2 texture slots, but shader 0 has 0"`; `SLOTS` slot 0 = texture 5 of none → `"texture slot 0 uses texture 5, of 0"`; `shader: 3` of 1 → `"uses shader 3, of 1"`; 262,145 bytes of WGSL → `"more than 262144"`.
  - `material_tangents_need_tangent_geometry` (Review Focus): `requires_tangents` on `square(false)` → message contains `"draw 0"`, `"material 0"`, `"tangents"`.
  - `material_shaders_compile_once` (Review Focus): shaders `[FLAT, FLAT (same text, another name), LAMBERT]`, three materials → after one render `compiles == 2`; a renderer drawing a model with a broken shader twice → both `InvalidPluginContent`, `compiles == 1`.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer material_` → fail (custom refused).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `material_` and the scene suite pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): draw plugins' custom scene materials`.

### Task 6: Material parameters per frame

**Files:** Modify `crates/gpu-renderer/src/scene.rs` (`invalid_scene_frame(model, params_sizes, frame)`, the parameter slot); tests in `tests/scene.rs`.

**Decisions:**
- A draw's `params` replace its material's for that frame only; its length must be the material's (`"draw {i}'s parameters are {a} bytes, but its material {m}'s shader {s} takes {b}"`).
- `params` on an `unlit` (or, until Task 9, MToon) draw → `"draw {i}'s material {m} is {kind}, which takes no parameters"`. `MaterialOverrides` are unlit's: any `Some` override on a custom draw → `"draw {i}'s material {m} is custom: overrides are for unlit materials"`.
- `MaterialOverrides`' doc says it is the unlit material's per-frame values.

- [ ] **Step 1:** Tests:
  - `frame_params_replace_the_material_params`: `FLAT` red; frame 1's draw `params` green → `[0, 255, 0, 255]`; the same renderer's next frame without `params` → red (nothing kept).
  - `frame_params_follow_each_draw`: two `FLAT` squares sharing material 0 side by side, only draw 1 green → left red, right green.
  - `frame_params_must_match_the_material` (Review Focus): 8 bytes → `"draw 0's parameters are 8 bytes, but its material 0's shader 0 takes 16"`; `params` on `plain` → `"is unlit, which takes no parameters"`; `overrides.base_color` on a custom draw → `"overrides are for unlit materials"`.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer frame_params` → fail.
- [ ] **Step 3:** Implement. **Step 4:** Pass with `overrides_replace_the_material_texture_transform_and_color`; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): replace a scene material's parameters per draw and frame`.

### Task 7: Outline passes for custom materials

**Files:** Modify `crates/gpu-renderer/src/scene.rs` (`prepare`, `invalid_scene`, drawing); tests in `tests/scene.rs`.

**Decisions:**
- A custom material with `outline` draws its outline right after its surface, in the same queue slot, from the same skinned vertices, uniforms, parameters, and textures (phase 2's rule): `PipelineKey { pass: Pass::Outline { bias_constant, bias_slope: slope_scale.to_bits() }, facing: Facing { cull: outline.cull, clockwise } }`, the material's alpha, both windings.
- A material asking for an outline whose shader defines no `material_outline_fragment` → `"material {m} asks for an outline, but shader {s} defines no \`material_outline_fragment\`"`.
- The vertex offset is added before projection (`world`) and after it (`clip.xy`), so width in metres and width on screen are both exact; `Fragment.position` is the offset position.

Test material:

```rust
const RINGED: &str = "struct Params { width: vec4f };   // x width, y 0 world / 1 clip, z 1 to scale by texture_0's green
fn material_fragment(fragment: Fragment) -> vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
fn material_outline_fragment(fragment: Fragment) -> vec4f { return vec4f(1.0, 0.0, 0.0, params.width.w); }
fn material_outline_vertex(vertex: Vertex) -> VertexOffset {
    var width = params.width.x;
    if params.width.z == 1.0 { width *= textureSampleLevel(texture_0, sampler_0, vertex.uv, 0.0).g; }
    if params.width.y == 0.0 { return VertexOffset(vertex.normal * width, vec2f(0.0)); }
    return VertexOffset(vec3f(0.0), normalize(vertex.normal.xy) * width * vertex.clip.w);
}";   // 1 slot, empty white
```

- [ ] **Step 1:** Tests (prefix `outline_`; phase 2's `shell()` (the square and its reversed twin, radial normals `(±1, ±1, 0)/√2`), orthographic height 2, 32 px a metre, the square x 16..48):
  - `outline_vertex_offsets_in_world_and_clip_space`: world width 0.17677670 → (14, 32) and (50, 32) red, (10, 32) `CLEAR`, (32, 32) green; clip width 0.17677670 (0.125 NDC a side) → the same four pixels.
  - `outline_texture_slots_reach_the_vertex_stage`: world 0.17677670 scaled by a 2×1 nearest clamp linear texture `[(0, 255, 0) | (0, 0, 0)]` → (14, 32) red, (50, 32) `CLEAR`.
  - `outline_culls_and_biases_as_asked`: width 0: default pass → (32, 32) green (front faces culled, bias behind); `cull: none`, bias `{0, 0.0}` → (32, 32) red (drawn after, coplanar, `LessEqual`).
  - `outline_mirrored_worlds_cull_the_front_faces`: `world = scaling(-1, 1, 1)`, world width 0.17677670 → the same ring.
  - `outline_draws_with_the_material_alpha`: blend, outline alpha 0.5 → (14, 32) alpha within 2 of 128; mask cutoff 0.5 with outline alpha 0.3 → (14, 32) `CLEAR`.
  - `outline_needs_its_fragment_function`: `FLAT` with `outline` → `"defines no \`material_outline_fragment\`"`; slope `NaN` → `"outline depth bias is not finite"`.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer outline_vertex outline_texture outline_culls outline_mirrored_worlds_cull outline_draws outline_needs` → fail.
- [ ] **Step 3:** Implement. **Step 4:** The new tests and phase 2's `outline_` tests pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): outline passes for custom scene materials`.

### Task 8: The turntable's own toon shader

**Files:**
- Create: `examples/plugins/turntable/src/toon.wgsl`
- Modify: `examples/plugins/turntable/src/lib.rs` (material, shader, tests), `examples/plugins/turntable/README.md`, `examples/plugins/README.md` (the `turntable` row)

**Interfaces — Produces:** `fn toon() -> Material` becomes a `MaterialKind::Custom` on shader 0; `fn toon_shader() -> SceneShader` (`name: Some("turntable toon")`, one slot `checker`, empty white, `wgsl: include_str!("toon.wgsl")`); `fn toon_params() -> Vec<u8>`:

```wgsl
struct Params {
    shade: vec4f,    // shade colour (times the checker), shading shift
    rim: vec4f,      // rim colour, fresnel power
    outline: vec4f,  // outline colour, how far it is shaded like the surface
    shape: vec4f,    // toony, outline width (a share of the canvas height where a metre fills it), 0, 0
};                   // 64 bytes: (0.4, 0.42, 0.75, −0.05), (0.25, 0.25, 0.32, 8), (0.01, 0.008, 0.012, 0.5), (0.95, 0.02, 0, 0)
```

**Decisions:**
- The look stays the current MToon cube's, written as the toon shading this sample needs and nothing more: `lit = checker`; `step = linearstep(−1 + toony, 1 − toony, N·L₀ + shift)`; `color = mix(shade × lit, lit, step) × C₀ + lit × ambient_light(N) + rim`, `rim = pow(saturate(1 − N·V), power) × rimColour × (C₀ + ambient_light(N))` (0 where the base is 0); the outline grows on screen as MToon's screen-coordinates width (clip offset `normalize(n_clip) × 2 × width × min(clip.w, P[1][1])`, x by `height_over_width`, times `1 − n_view.z²`), coloured `outline × mix(1, surface colour, mix)`. With the sample's one light and flat ambient this equals MToon's result, so the before and after stills should match to within rounding.
- `outline: Some(OutlinePass::default())`; `requires_tangents` false; the pane and the leaf stay unlit.

- [ ] **Step 1:** Unit tests in `src/lib.rs`: `the_cube_is_toon_shaded_by_its_own_shader` (material 0 is `Custom` with shader 0, `textures == [Some(0)]`, `outline == Some(OutlinePass::default())`, `params.len() == 64`, the f32 at byte 52 is 0.02; materials 1 and 2 unlit; `shaders.len() == 1` with one slot); `the_scene_is_lit_by_one_light_and_ambient` unchanged. The process test `the_renderer_draws_the_process_scene` must still pass (it compiles `toon.wgsl` on the GPU).
- [ ] **Step 2:** `cargo test -p celesta-plugin-turntable` → fail.
- [ ] **Step 3:** Implement; README "How it works": the cube's material is the sample's own WGSL toon shader, sent with the model; what each parameter does; the outline is the material's outline pass.
- [ ] **Step 4:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-plugin-turntable` passes; `node examples/plugins/build.mjs turntable`; `cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx turntable-after.png --frame 30 --plugin-dir examples/plugins/dist` (outside the repository); compare with Step 0's still; then the task checks.
- [ ] **Step 5:** Commit `feat(examples): the turntable's cube in its own WGSL toon material`.

### Task 9: MToon leaves Celesta

This commit's parent is where the VRM plugin takes MToon from (Appendix): `crates/gpu-renderer/src/scene_mtoon.wgsl`, `mtoon_params` and `uv_scroll` in `scene.rs`, `MtoonMaterial` and its defaults in `crates/plugin/src/scene.rs`, and the MToon tests in `tests/scene.rs`.

**Files:**
- Delete: `crates/gpu-renderer/src/scene_mtoon.wgsl`
- Modify: `crates/plugin/src/scene.rs` (remove `MaterialKind::Mtoon`, `MtoonMaterial`, `OutlineWidthMode`, `TextureSlot`, `ScaledTextureSlot`, the five MToon `MaterialOverrides` fields; `Lights` and `MaterialOverrides` docs without the VRM/UniVRM sentences), `crates/plugin/src/lib.rs`, `crates/plugin/src/protocol.rs` (remove `mtoon_scene_model_round_trips`, `mtoon_overrides_round_trip`; the `serve_on` test's `"vrm"` plugin and `C:/m.vrm` asset become `"scenes"` and `C:/m.glb`), `crates/gpu-renderer/src/scene.rs` (remove `ShaderId` 1, `MTOON_SLOTS`, `MTOON_VERTEX_SLOT`, `mtoon_textures`, `mtoon_numbers`, `mtoon_params`, `uv_scroll`, the MToon arms of `invalid_scene`, `invalid_scene_frame`, and `prepare`, `mtoon_slots_and_numbers_are_checked`; comments say "Unity's render queues" where they said UniVRM's), `crates/gpu-renderer/src/tests/scene.rs` (remove the `lit_`, `shading_`, `surface_`, MToon `outline_`, MToon `override_` tests, `mask_mtoon_covers_by_cutoff`, `outline_every_alpha_side_and_mode_draws`'s MToon cases, and their helpers `toon`, `flat_toon`, `toon_square`, `black_toon`, `rim`, `matcap`, `animated`, `shell_scene`, …; keep `shell()` for Task 7's tests and `light()`, `with_normals()`, `texel()`, `nearest()` where Tasks 5–7 use them)

**Decisions:**
- Generic coverage that the removed tests gave is already in Tasks 1, 5, 6, 7 (queues, lights and ambient, normal matrix, handedness, facing, view, degenerate worlds, alpha modes, outlines). `outline_every_alpha_side_and_mode_draws` is kept as a custom-material test over `RINGED` (every alpha mode, `depthWrite` included, × single/double-sided × world/clip, opened and drawn in both windings).
- **Check, as a step:** `git grep -n -i -E "mtoon|vrm|toony|matcap" -- crates/plugin crates/gpu-renderer` prints nothing.

- [ ] **Step 1:** Rewrite `outline_every_alpha_side_and_mode_draws` over `RINGED`; run it (`env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer outline_every`) before the removal → passes.
- [ ] **Step 2:** Remove; the grep above prints nothing (`vrm` also catches `UniVRM`).
- [ ] **Step 3:** `cargo build --workspace --all-targets`; then the task checks.
- [ ] **Step 4:** Commit `refactor(plugins): MToon leaves Celesta for the VRM plugin`.

### Task 10: Docs and the spec

**Files:**
- Rename and rewrite: `docs/handoff/2026-11-03-plugin-scene-mtoon.md` → `docs/handoff/2026-11-03-plugin-scene-materials.md` (`git mv`, then rewritten; the format of `2026-11-03-plugin-scene-content.md`)
- Modify: `docs/handoff/2026-11-03-plugin-scene-content.md` (its intro, Model `materials`, Frame `lights`/`time`/`overrides`, queue lines, and "Not done" point at the materials note; nothing names MToon), `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md` (the amendment's replacement sections), `docs/superpowers/plans/2026-11-03-plugin-scene-mtoon.md` (one line under its title: `> Superseded: Celesta's MToon became the VRM plugin's material; see \`2026-11-03-plugin-scene-materials.md\`.`), `packages/website/content/en/docs/05-(more)/05-plugins.md`, `packages/website/content/ja/docs/05-(more)/05-plugins.md`, `skills/celesta/references/plugins.md`

**The materials note:** what it adds (custom materials in WGSL, the built-in `unlit`, the generic blend fields, the outline pass, per-frame parameters); the protocol (types and JSON names, the payload order, limits); the interface (the prelude as Task 2 gives it, the functions, the entry points' wrappers, slot names, empty texels, a minimal material); lights and the SH polynomial (moved from the mtoon note, without the UniVRM sentence; "values copied from a Unity scene negate `c0`, `c4`, `c7` and a light's x, since Unity's X is mirrored against glTF's" stays as a Unity fact); queues and sorting; the outline pass; how Celesta draws it (bind groups, the shader cache, pipelines made on open, the frame resource cache, the normal matrix and handedness); the turntable change; What landed; **For plugins** (phase 2's hazards, generic: sort a renderer's parts by one `center`; compact shared accessors; the skinning pass at 8 storage bindings; no byte budget on `model.textures`; tangents are the plugin's; a model's shaders and pipelines compile on open); **Not done** at least: no custom varyings (a material computes per fragment what it needs from `Fragment`); `Frame.time` is `f32` (a plugin sends time-precise values in its parameters); compiled shaders and pipelines are never evicted; shadows (phase 5, to be offered to materials); point and spot lights; glTF PBR (a plugin material, phase 5); a material's NaN is not caught; the depth-bias tests have run on Metal only; phase 1's still-open items. **Checks:** the test groups of Tasks 1–9.

Guide replacement (en, "A program" bullet, from "Celesta draws them; …" to the end of the material sentence): "Celesta draws them; scenes get depth, anti-aliasing, and opaque, cut-out, and see-through materials. A material is either Celesta's unlit material or one the program brings, written in WGSL against Celesta's material interface, which can shade with the directional lights and ambient light the program sends and add an outline. Each frame can also change a material's parameters, as a model's material animation does." The `turntable` row: "…a checkered cube turning on a turntable, toon-shaded by the plugin's own WGSL material under a light and outlined, …". The notes sentence names `docs/handoff/2026-11-03-plugin-scene-materials.md` (materials in WGSL, lights, render queues, and outlines).

Guide replacement (ja, the same places): 「Celesta はシーンを奥行きとアンチエイリアス付きで描き、不透明・切り抜き・半透明のマテリアルを扱います。マテリアルは Celesta のライティングなし（unlit）のマテリアルか、プログラムが持ち込むマテリアルです。持ち込むマテリアルは Celesta のマテリアルインターフェースに沿って WGSL で書き、プログラムが送る平行光源と環境光で陰影を付けたり、アウトラインを足したりできます。フレームごとに、モデルのマテリアルアニメーションのようにマテリアルのパラメーターを変えることもできます。」; `turntable` の行は「…プラグイン自身の WGSL マテリアルでトゥーン調に陰影が付き、アウトラインが描かれた…」; 資料の文は「…持ち込むマテリアル（WGSL）・ライト・描画順・アウトラインは `docs/handoff/2026-11-03-plugin-scene-materials.md` で説明しています。」

Skill (`skills/celesta/references/plugins.md`): the error row lists "a material shader that does not compile, parameters of the wrong size, a material that needs tangents on geometry without them" in place of "a normal map on geometry without tangents"; the scene paragraph says "each material Celesta's unlit one or the plugin's own in WGSL, shaded by the directional lights and spherical-harmonics ambient light and able to add an outline; a frame can replace a material's parameters"; the notes list names the materials note.

- [ ] **Step 1:** Write the docs; apply the spec amendment. **Step 2:** `pnpm --dir packages/website run build` passes; `git grep -n "plugin-scene-mtoon.md"` lists only the superseded plan's own references and its line.
- [ ] **Step 3:** Commit `docs: plugin materials in WGSL for scene content`.

### Finish

- [ ] Full checks once: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-plugin -p celesta-plugin-host -p celesta-gpu-renderer -p celesta-plugin-flag -p celesta-plugin-turntable`.
- [ ] Push the new commits to #659's branch; update the PR's title and description to the reworked scope (custom materials; MToon moves to the VRM plugin) and attach `turntable-before.png` and `turntable-after.png` with `gh pr edit --attach`.
- [ ] Then `gh stack rebase` the VRM branch onto it and redo its material work as the Appendix says.

---

## Appendix: changes to `docs/superpowers/plans/2026-11-03-plugin-vrm-core.md`

After `gh stack rebase`, the VRM branch's existing commits that name `MtoonMaterial`, `OutlineWidthMode`, `TextureSlot`, `ScaledTextureSlot`, the MToon `MaterialOverrides` fields, a unit `AlphaMode::Blend`, `transparent_with_z_write`, or `render_queue_offset_number` no longer build against the new base. They are not rewritten (no amends): the redone tasks below land as new commits on top and restore the build. A rebase conflict in `crates/plugin` or `crates/gpu-renderer` (from the plan's Task 13 frame canvas, if it has landed) keeps both sides.

**Header and Global Constraints**
- Architecture: `materials.rs` maps materials and textures; **`mtoon.rs` and `mtoon.wgsl` are the plugin's MToon material** (UniVRM's MToon10 written against Celesta's material interface).
- Spec line: `2026-11-03-plugin-scene-mtoon.md` → `2026-11-03-plugin-scene-materials.md` (its "For plugins" list).
- New constraint: "MToon is the plugin's: `src/mtoon.wgsl`, recovered from `crates/gpu-renderer/src/scene_mtoon.wgsl` at the parent of Celesta's commit `refactor(plugins): MToon leaves Celesta for the VRM plugin` (`git show <that commit>^:crates/gpu-renderer/src/scene_mtoon.wgsl`), and `src/mtoon.rs` (`Mtoon` with UniVRM's defaults from that parent's `crates/plugin/src/scene.rs`; `params` from its `scene.rs`'s `mtoon_params` and `uv_scroll`); both carry the UniVRM notice. Its GPU tests are `tests/mtoon.rs`."
- Review Focus gains: "**MToon parity survives the move:** every MToon test Celesta had passes in `tests/mtoon.rs` with the same expected pixels. Task 3b."

**Task 3 (textures and materials), redone**
- Interfaces: `pub(crate) struct Materials { pub textures, pub materials, pub notices, pub base_transforms, pub mtoon: Vec<Option<Mtoon>> /* by material: the typed parameters packed into params */, pub shaders: Vec<SceneShader> }`.
- A material with `VRMC_materials_mtoon` becomes `MaterialKind::Custom(CustomMaterial { shader: 0, params: mtoon::params(&m, &MtoonOverrides::default(), 0.0, &textures).to_vec(), textures: mtoon::slots(&m).map(…).to_vec() /* 9 slots */, outline: (m.outline_width_mode != OutlineWidthMode::None).then(OutlinePass::default), requires_tangents: m.normal_texture.is_some() })`; `shaders` is `vec![mtoon::shader()]` when any material is MToon, else empty. Blend: `AlphaMode::Blend { depth_write: transparentWithZWrite, queue_offset: renderQueueOffsetNumber }`; opaque and mask drop both (UniVRM ignores them there).
- The plugin checks what Celesta can no longer see inside the bytes before packing: `mtoon::invalid(&m, textures.len()) -> Option<String>` (slot ranges by MToon name, every number finite) → `LoadError::Invalid("material \"{name}\": {reason}")`.
- Tests: `mtoon_slots_dedupe_by_image_colour_space_and_sampler` asserts on `materials.mtoon[0]` and the custom material's `textures == [Some(0), Some(0), Some(2), Some(1), None, None, None, None, None]`; `hair_keeps_blend_and_its_queue_offset` expects `AlphaMode::Blend { depth_write: false, queue_offset: -1 }`; new `mtoon_materials_share_one_shader` (materials 0 and 1 both `shader: 0`, one entry in `shaders`, `requires_tangents` only on material 0).

**Task 3b (new, after Task 3): MToon as the plugin's material**
- Files: create `plugins/vrm/src/mtoon.rs`, `plugins/vrm/src/mtoon.wgsl`, `plugins/vrm/tests/mtoon.rs`.
- Interfaces:
  ```rust
  pub struct Mtoon { /* the former MtoonMaterial's fields and UniVRM defaults, minus transparent_with_z_write and render_queue_offset_number */ }
  pub enum OutlineWidthMode { None, WorldCoordinates, ScreenCoordinates }
  pub struct TextureSlot { pub texture: usize }   pub struct ScaledTextureSlot { pub texture: usize, pub scale: f32 }
  #[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct MtoonOverrides {   // one frame's binds
      pub base_color: Option<[f32; 4]>, pub texture_offset: Option<[f32; 2]>, pub texture_scale: Option<[f32; 2]>,
      pub shade_color: Option<[f32; 3]>, pub emission_color: Option<[f32; 3]>, pub matcap_color: Option<[f32; 3]>,
      pub rim_color: Option<[f32; 3]>, pub outline_color: Option<[f32; 3]>,
  }
  pub const MTOON_PARAMS: usize = 176;            // Task 3's Params layout
  pub fn shader() -> SceneShader;                 // name "MToon", 9 slots: base colour, shade multiply, shading shift (black), normal, emissive, matcap (black), rim multiply, outline width, UV animation mask
  pub fn slots(m: &Mtoon) -> [Option<usize>; 9];
  pub fn params(m: &Mtoon, o: &MtoonOverrides, time: f64, textures: &[SceneTexture]) -> [u8; MTOON_PARAMS];   // little-endian; the scroll formed in f64 and wrapped as uv_scroll did
  pub(crate) fn invalid(m: &Mtoon, textures: usize) -> Option<String>;
  ```
- Tests: `tests/mtoon.rs` ports the `lit_`, `shading_`, `surface_`, `outline_` (MToon ones), `override_` (the five colours and base colour), `queue_` MToon cases, and `mask_mtoon_covers_by_cutoff` tests with their expected pixels, drawing through `GpuRenderer::with_content_provider(FixedScene { model, frame })` (a test `ContentProvider` answering one fixed `SceneModel` and `SceneFrame`) and the turntable's adapter-skip pattern; `mtoon.rs` unit tests port `mtoon_slots_and_numbers_are_checked` (as `invalid`), the UV scroll precision and wrapping cases (as `params`), and `mtoon_params_pack_univrm_defaults` (default `Mtoon` → shade `(1, 1, 1, −0.05)` at byte 48, toony 0.95 at 92, GI equalization 0.9 at 120, fresnel power 5 at 112).
- Commit `feat(plugins): MToon as the VRM plugin's own scene material`.

**Task 4 (model, frame), changed**
- `frame` packs each MToon draw's `params` with `mtoon::params(…, time, …)` and the frame's binds, and sends `DrawState.params` only when the bytes differ from the material's (UV animation or binds); unlit draws get `MaterialOverrides` (base colour, texture offset and scale) only.
- `overrides_are_finite` checks the plugin's `MtoonOverrides` before packing (`FrameError("material {m}'s {bind} is not finite")`), since Celesta cannot.

**Task 5 (expressions), changed**
- `apply(&self, actual, morph: &mut [Vec<f32>], binds: &mut [MtoonOverrides] /* by material */)`; `model.rs` turns an unlit material's binds into `MaterialOverrides` and an MToon material's into parameters. "Bind types on unlit materials other than `color` are sent and ignored by Celesta" becomes "… are dropped by the plugin: an unlit material has no such colour". Tests read the binds through a helper `binds(model, inputs) -> MtoonOverrides` instead of `frame.draws[i].overrides`; their expected values do not change.

**Task 11, changed:** `uv_animation_sample_keeps_its_speeds` reads `materials.mtoon`.

**Task 12 (docs), changed:** `docs/handoff/2026-11-03-plugin-vrm.md` gains an "MToon" section moved from the phase-2 mtoon note (its fields and defaults table, how it shades, outlines, binds packed into parameters, and "Where it knowingly differs from UniVRM": the mask edge, `V` per fragment, texture rotation honoured, emissive strength folded in, additional lights in one fragment, the glTF-axis translations, the screen-width formula and its `P[1][1]` cap, `linearstep` at toony 1, coplanar draws and the outline's depth bias). Its "Not done" says Celesta compiles the MToon shader once per renderer, when the first avatar opens. The modify list's `2026-11-03-plugin-scene-mtoon.md` becomes `2026-11-03-plugin-scene-materials.md` ("For plugins": which items the plugin handles).
