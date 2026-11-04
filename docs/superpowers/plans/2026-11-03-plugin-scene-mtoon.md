# Plugin scene MToon (phase 2) Implementation Plan

> Superseded: Celesta's MToon became the VRM plugin's material; see `2026-11-03-plugin-scene-materials.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scene materials can be MToon (`VRMC_materials_mtoon` 1.0 as UniVRM's MToon10 draws it): lit by the frame's directional lights and SH ambient, with UniVRM's render queues, outlines, rim and matcap, UV animation, normal maps, and emission; frame resources are reused instead of reallocated.

**Architecture:** `celesta-plugin` gains `MaterialKind::Mtoon(MtoonMaterial)` and five more `MaterialOverrides`. `celesta-gpu-renderer`'s `scene.rs` keeps per-frame targets and buffers in `SceneFrameResources`, binds a frame uniform (camera, lights, SH, time) as group 0, a per-draw uniform with the normal matrix and the material's factors as group 1, and one bind group per material (nine MToon texture slots, Unity's default texel for an empty one) as group 2. `scene_mtoon.wgsl` adds MToon's main and outline passes beside `scene.wgsl`'s unlit ones; draws go in UniVRM's queue order with offsets, the outline right after its draw. The turntable sample's cube becomes MToon.

**Tech Stack:** Rust 2024, wgpu 30 (WGSL), serde.

**Spec:** `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md` (Rendering in Celesta → Targets, Per frame 2–3, MToon, Unlit and PBR, Lighting presets and shadows; Protocol → Model `materials`, Frame `lights`/`time`/`overrides`; Phases → 2). Phase 1: `docs/handoff/2026-11-03-plugin-scene-content.md`. Shadows, PBR, and point/spot lights are phase 5.

## Global Constraints

- The protocol stays version 1 and changes in place; `celesta-plugin` keeps its dependencies (serde, serde_json, thiserror, libc on Unix).
- Scene conventions: right-handed, +Y up, metres; the camera looks down its −Z; matrices `f32` 4×4 column-major; UV origin top-left (glTF); colors linear unless stated.
- Targets: color `Rgba16Float`, linear, 4× MSAA; depth `Depth32Float`, 4× MSAA; cleared to transparent black; resolved, then converted to premultiplied, sRGB-encoded values; no tone mapping.
- Queues: `opaque` 2000; `mask` 2450; `blend` + `transparentWithZWrite` 2501 + clamp(offset, 0, 9), depth write on; `blend` 3000 + clamp(offset, −9, 0), depth write off, sorted back to front by draw bounds centre; opaque before transparent; ties by draw index.
- Color blends `SrcAlpha, OneMinusSrcAlpha` in linear space; alpha `One, OneMinusSrcAlpha`.
- `mask`: alpha-to-coverage with the sharpened alpha `(a − cutoff) / max(fwidth(a), 1e-5) + 0.5`.
- Outlines: after each draw's main pass, in the same queue slot; front faces culled; depth bias (Unity's `Offset 1,1`); the material's blend and depth write; color `outlineColor × lerp(1, shaded, outlineLightingMix)`, `shaded` including emission and rim.
- World width: `position + normalize(normal) × width × widthTex.g` (metres, unaffected by scale).
- Screen width, in clip space: `offset = normalize(normalClip.xy) × 2 × width × min(clip.w, P[1][1])`, `offset.x ×= height / width`, `offset ×= saturate(1 − normalView.z²)`.
- The width texture is sampled with raw `uv0`: no texture transform, no UV animation.
- `lit = baseTex × baseColorFactor`; `shift = shadingShiftFactor + shadingShiftTex.r × scale`; `shade = linearstep(−1 + toony, 1 − toony, dot(N, L) + shift) × atten`; `direct = lerp(shadeColor × shadeMultiplyTex.rgb, lit, shade) × lightColor`; `gi = lit × lerp(SH(N), ½(SH(+Y) + SH(−Y)), giEqualization)`; `rim = (matcapFactor × matcapTex(uvMatcap) + pow(saturate(1 − dot(N, V) + rimLift), max(fresnelPower, 2^−10)) × rimColor) × lerp(1, lightColor + SH(N), rimLightingMix) × rimMultiplyTex`; `color = direct + gi + emissiveTex × emissiveFactor + rim`.
- `atten` is 1 (no shadows yet).
- Additional directional lights add `lerp(shadeColor, lit, linearstep(…)) × lightColor × shadow`, `shadow = atten × ½(min(0, dot(N, L)) + 1)`, no attenuation inside the step, and their rim with `lerp(0, lightColor × shadow, rimLightingMix)`; no GI or emission.
- UV animation on every texture but the matcap (and the outline width texture); speeds are the file's, in glTF's UV space.
- Back faces of double-sided materials flip the normal. Vertex colors are ignored.
- Missing MToon fields take UniVRM's values: shading shift −0.05, toony 0.95, shade color white, matcap factor black.
- `lights`: `directional` list, the first is the main light; `ambient` nine SH coefficients per channel in Unity's `ShadeSH9` order and scale. `time`: seconds for UV animation.
- `unlit`: base color × texture, alpha modes as above; it has no lighting and no outline.
- Where UniVRM's MToon10 source differs from the spec's wording, the task says so and follows UniVRM (the spec's "Look like UniVRM" decision).
- The CPU reference renderer and `@celesta/web` keep refusing plugin content.
- GPU tests return early without an adapter: run every red/green and full run with `env CELESTA_REQUIRE_GPU=1` so a missing adapter fails instead of passing.
- Each task ends with `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` passing, then its own commit. Never amend; tests and commits are separate commands.

## Review Focus

- **A normal-mapped MToon material on a geometry without tangents** (a plugin that forgets MikkTSpace): `InvalidPluginContent` naming the draw, the material, and "tangents" at load, never a GPU validation error or NaN pixels. Task 5.
- **Lights the shader cannot use** (five or more directional lights, a zero `direction`) and **no lights at all**: the first two are `InvalidPluginContent` naming the light; no lights draws ambient only (black when the ambient is zero), never NaN. Task 3.
- **Degenerate `world` matrices** (scale 0, which animation uses to hide parts; a mirrored world, determinant < 0): scale 0 draws nothing without error; a mirrored draw lights like its mirror image (normal matrix and tangent sign follow the determinant). Task 3 (normals), Task 5 (tangents).
- **A camera looking straight along ±Y** (top-down shots; `cross(+Y, V)` vanishes): the matcap still samples a finite texel. Task 4.
- **`shadingToonyFactor` 1** (hard anime shading, common in VRM files; `linearstep` with start = end): a hard step, shade where `dot(N, L) + shift ≤ 0`, lit above, never NaN. Task 3.

---

### Task 1: Reuse scene targets and frame buffers

**Files:**
- Modify: `crates/gpu-renderer/src/scene.rs`, `crates/gpu-renderer/src/mesh.rs` (`MeshState::scene_frames`; `plugin_texture` passes it), `crates/gpu-renderer/src/prepare.rs` (frame start)
- Test: `crates/gpu-renderer/src/tests/scene.rs`

**Interfaces — Produces:**

```rust
#[derive(Default)]
pub(crate) struct SceneFrameResources {
    targets: HashMap<(u32, u32), SceneTargets>,   // by canvas size in texels
    used: HashSet<(u32, u32)>,                    // sizes drawn since the last begin_frame
    frame_data: GrowingBuffer,                    // STORAGE: skinning matrices, morph pairs
    skin_params: GrowingBuffer, draws: GrowingBuffer, view: GrowingBuffer,   // UNIFORM
    groups: Option<FrameGroups>,                  // rebuilt when a GrowingBuffer reallocates
    pub(crate) allocations: u64,                  // textures and buffers created, ever
}
struct SceneTargets { multisampled: wgpu::TextureView, depth: wgpu::TextureView,
    resolved: wgpu::TextureView, convert_group: wgpu::BindGroup }
struct GrowingBuffer { buffer: Option<wgpu::Buffer>, capacity: u64 }
impl GrowingBuffer { fn write(&mut self, device, queue, usage, bytes: &[u8], largest: u64,
    allocations: &mut u64) -> bool /* reallocated */ }
impl SceneFrameResources { pub(crate) fn begin_frame(&mut self) }   // drops targets of sizes not in `used`, clears `used`
// MeshState gains: pub(crate) scene_frames: SceneFrameResources
// GpuRenderer::scene_texture gains a trailing `resources: &mut SceneFrameResources`
```

**Decisions:**
- `plugin_texture` (`&mut self`) takes the resources out with `std::mem::take(&mut self.meshes.scene_frames)`, calls `scene_texture(&self, …, &mut resources)`, and puts them back whether it succeeded or not; no interior mutability.
- `prepare.rs` calls `self.meshes.scene_frames.begin_frame()` where it clears `plugin_notices`: every size a frame draws stays cached (two scene layers of different sizes keep both), a size no layer drew in the last frame is dropped.
- A `GrowingBuffer` reallocates only when the bytes do not fit, to `bytes.len().next_power_of_two()` (at least 256, at most `largest`), and is written with `queue.write_buffer`. Each `scene_texture` submits its own encoder, so a later layer's writes cannot reach an earlier layer's submitted commands.
- The layer texture (`plugin_canvas`) is still made per frame: it is handed out as the layer's `LayerTexture`.

- [ ] **Step 1:** Tests (64×64, `one_square()`):
  - `scene_resources_are_reused_between_frames`: render `scene()` twice; `renderer.meshes.scene_frames.allocations` after the first is > 0 and unchanged after the second; the two frames' pixels are equal.
  - `scene_resources_follow_the_canvas_size`: render at scale 1 (count `a`), at scale 2 (> `a`, call it `b`), at scale 2 again (== `b`), then at scale 1 (> `b`: the 64² targets were dropped by the frame that did not draw them).
  - `scene_resources_keep_every_size_a_frame_draws`: a scene with two layers of plugin `scenes`, ids `scenes` (scale 1) and `scenes-2x` (scale 2); the second render of it allocates nothing.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer scene_resources` → fails to compile (`scene_frames` missing).
- [ ] **Step 3:** Implement. `frames_repeat_exactly` must still pass (nothing read is left from an earlier frame: every buffer range read is written that frame, targets are cleared).
- [ ] **Step 4:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer scene` passes; then the task checks.
- [ ] **Step 5:** Commit `perf(gpu-renderer): reuse scene targets and frame buffers`.

### Task 2: MToon material and overrides on the wire

**Files:**
- Modify: `crates/plugin/src/scene.rs` (types, docs, tests), `crates/plugin/src/lib.rs` (re-exports), `crates/plugin/src/protocol.rs` (tests only: materials and overrides travel as header JSON already)
- Modify (compile only): `crates/gpu-renderer/src/scene.rs` (the three irrefutable `let MaterialKind::Unlit(..)` become `match`es; `invalid_scene` refuses `Mtoon` with `"material {index} is mtoon, which is not drawn yet"` until Task 3), `examples/plugins/turntable/src/lib.rs` (`textured` matches both kinds)

**Interfaces — Produces** (`pub`, re-exported, `Clone, Debug, PartialEq, Serialize, Deserialize`, `rename_all = "camelCase"`):

```rust
pub enum MaterialKind { Unlit(UnlitMaterial), Mtoon(MtoonMaterial) }   // "kind": "unlit" | "mtoon"
#[serde(default)]   // Default = the values in the comments (UniVRM's MToon10 property defaults)
pub struct MtoonMaterial {
    pub base_color: [f32; 4],                          // [1, 1, 1, 1]
    pub base_color_texture: Option<TextureRef>,        // its transform is the material's, see below
    pub normal_texture: Option<ScaledTextureSlot>,     // scale: glTF normalTexture.scale
    pub emissive_factor: [f32; 3],                     // [0, 0, 0]; may exceed 1
    pub emissive_texture: Option<TextureSlot>,
    pub transparent_with_z_write: bool,                // false
    pub render_queue_offset_number: i32,               // 0
    pub shade_color_factor: [f32; 3],                  // [1, 1, 1]
    pub shade_multiply_texture: Option<TextureSlot>,
    pub shading_shift_factor: f32,                     // −0.05
    pub shading_shift_texture: Option<ScaledTextureSlot>,
    pub shading_toony_factor: f32,                     // 0.95
    pub gi_equalization_factor: f32,                   // 0.9
    pub matcap_factor: [f32; 3],                       // [0, 0, 0]
    pub matcap_texture: Option<TextureSlot>,
    pub parametric_rim_color_factor: [f32; 3],         // [0, 0, 0]
    pub rim_multiply_texture: Option<TextureSlot>,
    pub rim_lighting_mix_factor: f32,                  // 1
    pub parametric_rim_fresnel_power_factor: f32,      // 5
    pub parametric_rim_lift_factor: f32,               // 0
    pub outline_width_mode: OutlineWidthMode,          // None
    pub outline_width_factor: f32,                     // 0
    pub outline_width_multiply_texture: Option<TextureSlot>,
    pub outline_color_factor: [f32; 3],                // [0, 0, 0]
    pub outline_lighting_mix_factor: f32,              // 1
    pub uv_animation_mask_texture: Option<TextureSlot>,
    pub uv_animation_scroll_x_speed_factor: f32,       // 0, UV units a second
    pub uv_animation_scroll_y_speed_factor: f32,       // 0
    pub uv_animation_rotation_speed_factor: f32,       // 0, radians a second
}
pub enum OutlineWidthMode { None, WorldCoordinates, ScreenCoordinates }   // "none" | "worldCoordinates" | "screenCoordinates"; Default None
pub struct TextureSlot { pub texture: usize }
pub struct ScaledTextureSlot { pub texture: usize, #[serde(default = "one")] pub scale: f32 }
pub struct MaterialOverrides {   // existing three, plus VRM 1.0 materialColorBind targets (Default: all None)
    pub base_color: Option<[f32; 4]>, pub texture_offset: Option<[f32; 2]>, pub texture_scale: Option<[f32; 2]>,
    pub shade_color: Option<[f32; 3]>, pub emission_color: Option<[f32; 3]>, pub matcap_color: Option<[f32; 3]>,
    pub rim_color: Option<[f32; 3]>, pub outline_color: Option<[f32; 3]>,   // "shadeColor", "emissionColor", …
}
```

Doc comments fix these semantics:
- MToon samples every texture but the matcap and the outline width at the base color texture's coordinates: `base_color_texture.transform` (and the frame's `texture_offset`/`texture_scale`) moves them all, then UV animation, as UniVRM's single `_MainTex_ST` does. That is why the other slots are `TextureSlot`s without a transform. The outline width texture uses raw `uv0`; the matcap its own coordinates.
- Channels: `shadingShiftTexture` red, `outlineWidthMultiplyTexture` green, `uvAnimationMaskTexture` blue; the others RGB. The plugin picks each texture's `colorSpace`: sRGB for base color, shade multiply, emissive, matcap, rim multiply; linear for normal, shading shift, outline width, UV animation mask.
- `emissive_factor` holds `KHR_materials_emissive_strength` multiplied in, if the file has it.
- `Lights`: a light's `direction` is where it travels (kept from phase 1); shading uses `L = −normalize(direction)` and color `color × intensity`; at most 4 directional lights (Unity's default pixel light count). `ambient[k]` is the RGB of coefficient `k` of `SH(n) = c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)` (Unity's `unity_SHAr/Ag/Ab` xyzw, `unity_SHBr/Bg/Bb` xyzw, `unity_SHC` rgb), for normals in the scene's axes: a flat ambient color is `c3` alone. UniVRM mirrors X between glTF and Unity, so coefficients read from a Unity scene negate `c0`, `c4`, `c7`, and its light directions negate x.
- MToon-only overrides on an unlit draw are ignored.

- [ ] **Step 1:** Tests:
  - `crates/plugin/src/scene.rs`: `mtoon_material_defaults_follow_univrm`: `{"alphaMode":"opaque","doubleSided":false,"kind":"mtoon"}` deserializes to `MaterialKind::Mtoon(MtoonMaterial::default())`, and that default has shading shift −0.05, toony 0.95, shade `[1.0; 3]`, matcap `[0.0; 3]`, GI equalization 0.9, fresnel power 5, rim mix 1, outline mix 1, outline mode `None`, base `[1.0; 4]`; `{"texture":2}` is `ScaledTextureSlot { texture: 2, scale: 1.0 }`.
  - `mtoon_material_json_names`: a material with `transparent_with_z_write: true`, `render_queue_offset_number: -3`, `outline_width_mode: WorldCoordinates` serializes containing `"kind":"mtoon"`, `"transparentWithZWrite":true`, `"renderQueueOffsetNumber":-3`, `"outlineWidthMode":"worldCoordinates"`, `"uvAnimationRotationSpeedFactor":0.0`, and round-trips.
  - `crates/plugin/src/protocol.rs`: `mtoon_scene_model_round_trips` (a scene with one MToon material with every slot set, decode(encode) == model) and `mtoon_overrides_round_trip` (a frame draw with all eight overrides set).
- [ ] **Step 2:** `cargo test -p celesta-plugin mtoon` → fails to compile.
- [ ] **Step 3:** Implement; update the compile-only files.
- [ ] **Step 4:** `cargo test -p celesta-plugin` passes; `cargo build --workspace --all-targets` builds; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): MToon materials and color overrides in scene content`.

### Task 3: MToon main light and GI; frame uniform, normal matrix, material bind groups

**Files:**
- Modify: `crates/gpu-renderer/src/scene.rs`, `crates/gpu-renderer/src/scene.wgsl`, `crates/gpu-renderer/src/mesh.rs` (`LoadedScene::load` gets `&mut ScenePipelines`)
- Create: `crates/gpu-renderer/src/scene_mtoon.wgsl` (compiled as one module with `scene.wgsl`: `concat!(include_str!("scene.wgsl"), include_str!("scene_mtoon.wgsl"))`)
- Test: `crates/gpu-renderer/src/tests/scene.rs`; unit tests in `scene.rs`'s `mod tests`

**Interfaces — Produces:**

```rust
const MAX_DIRECTIONAL_LIGHTS: usize = 4;
#[repr(C)] struct FrameUniform {             // group 0 binding 0, VERTEX | FRAGMENT; WGSL `Frame`, 512 bytes
    view_projection: [f32; 16], view: [f32; 16], projection: [f32; 16],
    eye: [f32; 4],        // camera position in world; w 1 perspective, 0 orthographic
    forward: [f32; 4],    // the camera's +Z (towards the viewer) in world
    params: [f32; 4],     // x time (s), y canvas height / width, z directional light count
    lights: [LightUniform; MAX_DIRECTIONAL_LIGHTS],   // { to_light: [f32; 4], color: [f32; 4] } (color × intensity)
    ambient: [[f32; 4]; 9],
}
#[repr(C)] struct DrawUniform {              // group 1, dynamic offset; WGSL `Draw`, 304 bytes; unlit fills the first five
    world: [f32; 16],
    normal_matrix: [[f32; 4]; 3],            // columns
    base_color: [f32; 4],
    uv_u: [f32; 4], uv_v: [f32; 4],          // transform rows; uv_u.w mask cutoff
    shade: [f32; 4],                         // rgb shade color, w shading shift factor
    emissive: [f32; 4],                      // rgb emissive factor, w shading shift texture scale
    matcap: [f32; 4],                        // rgb matcap factor, w toony
    rim: [f32; 4],                           // rgb rim color, w rim lighting mix
    rim_shape: [f32; 4],                     // fresnel power, rim lift, GI equalization, normal scale
    outline: [f32; 4],                       // rgb outline color, w outline lighting mix
    outline_width: [f32; 4],                 // width factor, mode (0 none, 1 world, 2 screen), 0, 0
    uv_animation: [f32; 4],                  // scroll x, scroll y, rotation in turns a second (rad/s ÷ 2π), normal mapped (0/1)
}
fn normal_matrix(world: &[f32; 16]) -> [[f32; 4]; 3]
enum Shading { Unlit, Mtoon }   enum Pass { Main, Outline }   enum Cull { None, Back, Front }
struct Facing { cull: Cull, clockwise: bool }   enum Alpha { Opaque, Mask, BlendZWrite, Blend }
struct PipelineKey { shading: Shading, pass: Pass, facing: Facing, alpha: Alpha }
impl ScenePipelines { fn prepare(&mut self, device: &wgpu::Device, model: &SceneModel) }   // creates the keys the model's materials use, both windings
// LoadedScene: `textures: Vec<(wgpu::TextureView, wgpu::Sampler)>`, `materials: Vec<wgpu::BindGroup>` (one per model material)
// ScenePipelines: `view_layout` (group 0), `draw_layout`, `unlit_layout` (phase 1's two entries), `mtoon_layout`,
//   `white`, `black` (1×1 Rgba8Unorm views), `default_sampler`, `draw: HashMap<PipelineKey, wgpu::RenderPipeline>`
```

WGSL entry points: `vs_mtoon` (vertex buffers: skinned position @0, normal @1, tangent @2 at offsets 0/16/32 of the 48-byte stride; uv @3), `fs_mtoon_opaque`, `fs_mtoon_mask`, `fs_mtoon_blend`; `fn mtoon_color(surface: Surface, main_only: bool) -> vec3<f32>` shared with the outline (Task 7).

**Decisions:**
- **Bind groups (≤ 4 of WebGPU's defaults: 3 used).** MToon's group 2 is built once per material at load: binding `2k` is slot `k`'s texture and `2k + 1` its own sampler, slots in this order: 0 base color, 1 shade multiply, 2 shading shift, 3 normal, 4 emissive, 5 matcap, 6 rim multiply, 7 outline width multiply (visibility `VERTEX`; the others `FRAGMENT`), 8 UV animation mask. Nine sampled textures and nine samplers per stage at most, within the default 16 each. An empty slot binds Unity's property default as a 1×1 texel: black `(0, 0, 0, 255)` for shading shift and matcap, white for the rest (an unset normal slot binds white and is not read: `uv_animation.w` is 0). These defaults reproduce UniVRM's keyword gates: a shift of the factor alone, a matcap term of 0, a full UV animation time, a width of the factor alone.
- **Normal matrix:** the cofactor matrix of `world`'s upper 3×3 times the sign of its determinant (sign +1 for a determinant of 0), computed on the CPU in f64; the shader normalises `normal_matrix × normal`. It equals the inverse transpose up to a positive scale, and stays finite for a world of scale 0. Skinning keeps transforming normals by the skin matrices' 3×3, as Unity's skinning does.
- **Lights:** the main light is `lights[0]`; without lights its color is black (Unity's `_LightColor0` with no directional light). A `direction` of length 0 or more than 4 lights is `InvalidPluginContent` (`"directional light {i} has no direction"`, `"the frame has {n} directional lights, more than 4"`). The view direction `V` is `normalize(eye − position)` in perspective, `forward` in orthographic, per fragment (UniVRM normalises a per-vertex value; the difference is below a 1/255 step for the scenes Celesta draws).
- **Validation added to `invalid_scene`:** each MToon slot's texture in range (`"material {i}'s {slotName} uses texture {t}, of {n}"`, the serde name, e.g. `shadeMultiplyTexture`), and every MToon number finite (`"material {i}'s mtoon parameters are not finite"`). The Task 2 refusal goes.
- **Pipelines are made when a model loads** (`prepare`), only for the shading, alpha, and cull its materials use, both windings; drawing looks them up (`expect("prepared when the model loaded")`). `Alpha::BlendZWrite` exists from here; `Alpha::of` first returns it in Task 6.
- `eye` is `−Rᵀ·t` of `view` (computed in f64); `forward` is row 2 of `view`'s 3×3.

Algorithm (this task: the main light and GI; Task 4 adds the rest of `mtoon_color`):

```wgsl
fn linearstep(start: f32, end: f32, t: f32) -> f32 {
    if end <= start { return select(0.0, 1.0, t > start); }   // toony ≥ 1: Unity's x/0 then max(NaN, 0) = 0 at t = start
    return saturate((t - start) / (end - start));
}
fn sh(n: vec3<f32>) -> vec3<f32> {   // frame.ambient[k].rgb = c_k
    return c0*n.x + c1*n.y + c2*n.z + c3 + c4*(n.x*n.y) + c5*(n.y*n.z) + c6*(n.z*n.z) + c7*(n.z*n.x) + c8*(n.x*n.x - n.y*n.y);
}
// N: normalize(interpolated world normal), negated when !front_facing and the material is double-sided
let shift = draw.shade.w + textureSample(shading_shift, uv).r * draw.emissive.w;
let toony = draw.matcap.w;
let shade_rgb = draw.shade.rgb * textureSample(shade_multiply, uv).rgb;
let l0 = frame.lights[0];                         // color 0 when params.z == 0
let s0 = linearstep(-1.0 + toony, 1.0 - toony, dot(N, l0.to_light.xyz) + shift);
var color = mix(shade_rgb, lit, s0) * l0.color.rgb;
color += lit * mix(sh(N), 0.5 * (sh(vec3(0, 1, 0)) + sh(vec3(0, -1, 0))), draw.rim_shape.z);
```

- [ ] **Step 1:** Tests (prefix `lit_`; `one_square()` with an opaque single-sided MToon material, orthographic from +Z so `V = (0, 0, 1)`, ambient zero unless said; helpers `toon(MtoonMaterial) -> Material`, `flat_toon()` = default with base `[1.0; 4]`, shade `[0.0; 3]`, shift 0, toony 0, GI equalization 0; `light(direction, color, intensity)`; `with_normals(Scenes, [f32; 3])` sets every vertex normal; expected bytes are `srgb8(linear)` rounded, within `near`'s 2):
  - `lit_main_light_steps_by_toony_and_shift`: base `[1, 0, 0, 1]`, shade `[0, 0, 1]`, toony 0.5, shift −0.25, light `direction (0, −0.8660254, −0.5)` (N·L = 0.5) white ×1 → shade 0.75 → linear (0.75, 0, 0.25) → `[225, 0, 137, 255]`; the same with color `[0.5; 3]` intensity 2 → the same pixel.
  - `lit_shading_shift_texture_adds_red_times_scale`: as above with shift factor −0.5 and `shadingShiftTexture` a linear 1×1 `(255, 0, 0, 255)` at scale 0.25 → `[225, 0, 137, 255]` (green or blue channels would leave shift −0.5 → shade 0.5).
  - `lit_shade_multiply_texture_tints_the_shade`: shade `[1; 3]`, `shadeMultiplyTexture` sRGB 1×1 `(128, 64, 200)`, toony 0.5, light `direction (0, 0, 1)` (N·L = −1 → shade 0) → `[128, 64, 200, 255]`.
  - `lit_flat_ambient_is_the_constant_term`: no lights, `ambient[3] = [0.2; 3]`, GI equalization 0.9 → 124 grey; with `ambient` all zero → black opaque `[0, 0, 0, 255]`.
  - `lit_ambient_terms_follow_shade_sh9`: normal (0.48, 0.6, 0.64); for each k in 0..9 only `ambient[k] = [0.5; 3]` (`[-0.5; 3]` for k = 8) → greys `[134, 149, 153, 188, 106, 121, 125, 109, 72]`.
  - `lit_gi_equalization_blends_toward_the_vertical_average`: normal (0, 1, 0), `ambient[1] = [0.5; 3]`, `ambient[3] = [0.25; 3]`: equalization 0 → 0.75 (225), 1 → 0.25 (137), 0.5 → 0.5 (188).
  - `lit_normals_follow_the_inverse_transpose_of_world`: normal (0.6, 0.8, 0), `world = scaling(2, 1, 1)`, light `direction (0, −1, 0)`, `flat_toon` → N·L = 0.9363 → linear 0.9682 → 251 (transforming by `world` itself gives 228).
  - `lit_back_faces_of_double_sided_materials_flip_the_normal`: double-sided, base `[1, 0, 0, 1]`, shade `[0, 0, 1]`, toony 0.5, `world = scaling(−1, 1, −1)` (a half turn about Y: the back face shows), light `direction (0, 0, −1)` → `[255, 0, 0, 255]`.
  - `lit_toony_one_is_a_hard_step` (Review Focus): toony 1, shift 0, base red, shade blue; light `direction (0, −1, 0)` (N·L = 0) → `[0, 0, 255, 255]`; light `direction (0, −0.8660254, −0.5)` (0.5) → `[255, 0, 0, 255]`.
  - `lit_degenerate_worlds_draw_without_nan` (Review Focus): `world = scaling(0, 0, 0)` renders and pixel (32, 32) is `CLEAR`; `world = scaling(−1, 1, 1)`, light `direction (0, 0, −1)`, base red, shade blue, toony 0.5 → `[255, 0, 0, 255]` (an unsigned cofactor would flip N and give blue).
  - `lit_lights_the_shader_cannot_use_are_refused` (Review Focus): five lights → message contains `"5 directional lights"`; a light with direction `[0.0; 3]` → `"directional light 0"`; a frame without lights renders (`lit_flat_ambient_is_the_constant_term` covers the pixels).
  - `lit_mtoon_texture_out_of_range_is_refused`: `shadeMultiplyTexture: { texture: 3 }` with no textures → message contains `"shadeMultiplyTexture uses texture 3"`.
  - `scene.rs` unit `lit_normal_matrix_is_the_signed_cofactor`: `scaling(2, 1, 1)` → columns ∝ (0.5, 0, 0), (0, 1, 0), (0, 0, 1); `scaling(−1, 1, 1)` → diag(−1, 1, 1); `scaling(0, 0, 0)` → finite.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer lit_` → the new tests fail (MToon is refused; the unit test does not compile).
- [ ] **Step 3:** Implement. Phase 1's scene tests keep passing (unlit reads group 0 as `Frame`).
- [ ] **Step 4:** `lit_` and `scene` filters pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): MToon shading under the main light and SH ambient`.

### Task 4: Additional lights, emission, rim, matcap

**Files:** Modify `crates/gpu-renderer/src/scene_mtoon.wgsl`, `scene.rs` (uniform fill); tests in `tests/scene.rs`.

**Interfaces — Consumes:** Task 3's `Frame`, `Draw`, group 2 slots. **Produces:** the complete `mtoon_color(surface, main_only)`.

**Decisions:**
- Additional lights are a loop in the same fragment, not extra passes: Unity's ForwardAdd pass adds `c·a` with `Blend SrcAlpha One` on the base pass's fragments, which sums to the same `(base + add)·a + dst·(1 − a)`.
- **Matcap axes, ruling on the spec:** the spec's `right = normalize(cross(V, +Y))`, `up = normalize(cross(right, V))` are written in Unity's left-handed space; in Celesta's right-handed space the same screen-right and screen-up are `right = normalize(cross(+Y, V))` and `up = normalize(cross(V, right))`, and UniVRM's v-up texture coordinate becomes glTF's `uvMatcap = (dot(right, N) × ½ + ½, ½ − dot(up, N) × ½)`: a normal tilted towards screen right samples the texture's right, one tilted up its top row.
- When `cross(+Y, V)` has length² < 1e-12 (the camera looks along ±Y), `right` is the view's x axis (row 0 of `frame.view`'s 3×3).
- `pow(x, p)` with `x ≤ 0` is 0 (WGSL leaves `pow(0, p)` undefined).
- The matcap term is always computed: its black default texel makes it 0 when no matcap texture is set, as UniVRM's `_MTOON_RIMMAP` gate does.

```wgsl
let V = select(normalize(frame.forward.xyz), normalize(frame.eye.xyz - position), frame.eye.w == 1.0);
let x = saturate(1.0 - dot(N, V) + draw.rim_shape.y);
let parametric = select(pow(x, max(draw.rim_shape.x, 1.0 / 1024.0)), 0.0, x <= 0.0) * draw.rim.rgb;
let matcap = draw.matcap.rgb * textureSample(matcap_texture, matcap_sampler, uv_matcap).rgb;
let rim_tex = textureSample(rim_multiply, uv).rgb;
color += emissive_factor * textureSample(emissive, uv).rgb;
color += (matcap + parametric) * mix(vec3(1.0), l0.color.rgb + sh(N), draw.rim.w) * rim_tex;
if !main_only { for (var i = 1u; i < light_count; i++) {
    let d = dot(N, L_i);
    let shadow = 0.5 * (min(0.0, d) + 1.0);
    color += mix(shade_rgb, lit, linearstep(-1.0 + toony, 1.0 - toony, d + shift)) * C_i * shadow;
    color += (matcap + parametric) * mix(vec3(0.0), C_i * shadow, draw.rim.w) * rim_tex;
} }
```

- [ ] **Step 1:** Tests (prefix `shading_`; as Task 3's set-up):
  - `shading_additional_lights_use_half_lambert_shadow`: base red, shade blue, toony 0.5, shift 0; lights `[black main (direction (0, −1, 0), intensity 0), white (0, −0.8660254, −0.5)]` (N·L = 0.5) → `[188, 0, 0, 255]`; second light `(0, 0.8660254, 0.5)` (N·L = −0.5) → `[0, 0, 137, 255]`; the 0.5 light as the main light → `[255, 0, 0, 255]`.
  - `shading_emission_adds_factor_times_texture`: base and shade black, emissive `[0.5, 0.25, 0.0]`, `emissiveTexture` linear 1×1 `(255, 128, 255)` → (0.5, 0.1255, 0) → `[188, 99, 0, 255]`.
  - `shading_parametric_rim_follows_lift_power_and_mix`: base and shade black, rim color white, lift 0.5, power 1 (N·V = 1 → 0.5): mix 0 → 188; mix 1 with a white main light and `ambient[3] = [0.25; 3]` → 0.625 (207); mix 0.5 → 0.5625 (198); mix 1 plus a second white light `direction (0, −1, 0)` (N·L = 0, shadow 0.5) → 0.875 (240).
  - `shading_rim_multiply_texture_scales_rim_and_matcap`: the mix-0 case with `rimMultiplyTexture` sRGB 1×1 `(128, 128, 128)` → 0.1079 → 92.
  - `shading_matcap_maps_screen_right_and_up_to_texture_right_and_top`: base black, matcap factor white, rim mix 0, nearest clamp sRGB matcaps: 2×1 `[red | green]` with normal (0.6, 0, 0.8) → `[0, 255, 0, 255]`, normal (−0.6, 0, 0.8) → `[255, 0, 0, 255]`; 1×2 `[blue top; yellow bottom]` with normal (0, 0.6, 0.8) → `[0, 0, 255, 255]`.
  - `shading_matcap_looking_straight_down_is_finite` (Review Focus): `one_square()` with `frame.camera.view` replaced by the view looking down −Y from (0, 2, 0), column-major `[1,0,0,0, 0,0,1,0, 0,−1,0,0, 0,0,−2,1]` (camera +Z is world +Y, camera up world −Z), orthographic height 2; the square turned to face +Y by `world = [1,0,0,0, 0,0,−1,0, 0,1,0,0, 0,0,0,1]`; matcap sRGB 1×1 `(128, 64, 200)`, factor white, base black, rim mix 0 → pixel (32, 32) `[128, 64, 200, 255]`.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer shading_` → the new tests fail.
- [ ] **Step 3:** Implement. **Step 4:** `shading_` and `lit_` pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): MToon additional lights, emission, rim, and matcap`.

### Task 5: Normal maps and UV animation

**Files:** Modify `crates/gpu-renderer/src/scene_mtoon.wgsl`, `scene.rs` (validation, uniform fill); tests in `tests/scene.rs`.

**Decisions:**
- **Tangents are required, not made up:** a draw whose MToon material has a `normalTexture` on a geometry without tangents is `InvalidPluginContent` at load: `"draw {d}'s material {m} has a normalTexture, but geometry {g} has no tangents"` (the spec puts tangents on the plugin).
- Tangent space as UniVRM's `MToon_GetTangentToWorld`: `T = normalize(world3×3 × tangent.xyz)` (not the normal matrix), `sign = (tangent.w > 0 ? 1 : −1) × sign(det world)`, `B = normalize(sign × cross(N, T))` with `N` the (back-face-flipped) geometric normal.
- Decoding as Unity's `UnpackNormalWithScale` for its desktop normal-map layout: `xy = (rg × 2 − 1) × scale`, `z = sqrt(saturate(1 − dot(xy, xy)))`, then `N = normalize(T·x + B·y + N·z)`; blue is not read. Normal mapping feeds everything after it (shading, SH, rim, matcap).
- **UV animation, ruling on the spec:** the spec applies it before the texture transform and turns by `frac(t × speed) × 2π`; UniVRM's `GetMToonGeometry_Uv` applies `_MainTex_ST` first (sampling the mask there), and its importer divides the file's radians a second by 2π and negates scroll Y into Unity's v-up space. Followed UniVRM, written in glTF's UV space (which un-negates scroll Y and reverses the turn):

```wgsl
let uv_raw = in.uv;                                     // KHR transform and overrides, applied in vs_mtoon
let t = frame.params.x * textureSample(uv_mask, uv_raw).b;
let angle = fract(t * draw.uv_animation.z) * TAU;       // uv_animation.z = rotation speed / 2π
let d = uv_raw + t * draw.uv_animation.xy - 0.5;
let uv = vec2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y) + 0.5;
// every slot but the matcap and the outline width samples at `uv`
```

- [ ] **Step 1:** Tests (prefix `surface_`; Task 3's set-up; square tangents `[1, 0, 0, 1]`; UV tests use `ambient[3] = [1; 3]` so the pixel is the texel, a 2×2 nearest sRGB base texture TL red, TR green, BL blue, BR white, pixel (23, 23) at uv ≈ (0.234, 0.234), and `animate` setting `frame.time`):
  - `surface_normal_texture_tilts_the_normal`: linear 1×1 normal texel `(204, 128, 230)` (tangent-space x 0.6), `flat_toon`, light `direction (−1, 0, 0)`: scale 1 → linear 0.8 → 231; scale 0.5 → 0.65 → 211; scale 0 → 0.5 → 188.
  - `surface_mirrored_worlds_keep_normal_maps_upright` (Review Focus): texel `(128, 204, 230)` (tilted up), `world = scaling(−1, 1, 1)`, light `direction (0, −1, 0)` → 231 (an unsigned bitangent gives 124).
  - `surface_normal_texture_without_tangents_is_refused` (Review Focus): the same material on `square(false)` (no tangents) → message contains `"draw 0"` and `"tangents"`.
  - `surface_uv_animation_scrolls_and_turns_with_time`: scroll x 0.5: time 0 → red, time 1 → green; rotation `π/2` rad/s, no scroll: time 1 → blue.
  - `surface_uv_animation_mask_scales_time_by_blue`: scroll x 0.5, time 1: linear mask `(255, 255, 0)` → red; mask `(0, 0, 255)` → green.
  - `surface_texture_transform_applies_before_uv_animation`: base transform offset `(0.5, 0)`, rotation `π/2` rad/s, time 1 → red (animation first would give white).
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer surface_` → fail.
- [ ] **Step 3:** Implement. **Step 4:** `surface_`, `shading_`, `lit_` pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): MToon normal maps and UV animation`.

### Task 6: Render queues with offsets, z-written blend, and distance sorting

**Files:** Modify `crates/gpu-renderer/src/scene.rs` (`draw_queue`, `Alpha`), `scene_mtoon.wgsl` (`fs_mtoon_blend` discard); tests in `tests/scene.rs`.

**Interfaces — Produces:** `fn render_queue(material: &Material) -> u32`; `Alpha::of(material: &Material) -> Alpha`; `draw_queue` keeps its signature.

**Decisions:**
- Queues as in the Global Constraints (UniVRM's `MToonValidator`): offsets count only for blend; `transparentWithZWrite` only for blend; unlit has no offset (2000, 2450, 3000).
- **Sorting:** by queue; draws with queue > 2500 (both blend kinds, Unity's transparent range) back to front; then draw index. Back to front is the squared distance from the camera position (`eye`) to the draw's centre (`center`, else `world` × rest-bounds centre), larger first, for a perspective camera, and the view-space z, lower first, for an orthographic one: Unity's default transparent sort. Opaque and mask keep draw-index order.
- `BlendZWrite`: blend state of `Blend`, depth write on.
- **Depth compare becomes `LessEqual`** for every scene pipeline, unlit included (Unity's `ZTest LEqual`): of two coplanar draws, the later in the queue shows.
- MToon blend fragments with alpha < 2^−10 are discarded (UniVRM's `GetMToonGeometry_Alpha`, vrm-c/UniVRM#2598), so a clear z-written pixel writes no depth. Unlit is unchanged.
- **Mask, parity ruling:** UniVRM clips at `t < cutoff` and outputs alpha 1, so its `AlphaToMask` adds nothing; phase 1's coverage from `t` (the spec's) is kept. The edge moves by at most `|cutoff − 0.5|` of a pixel and is antialiased; recorded in the handoff note.

- [ ] **Step 1:** Tests (prefix `queue_`; MToon blend squares color by emission only: base `[0, 0, 0, 0.5]`, emissive red or blue, no lights; constants `RED_OVER_BLUE`, `BLUE_OVER_RED` from phase 1's tests):
  - `queue_offsets_order_blend_draws_before_distance`: red at z = 0, blue at z = −1 (phase 1's `red_over_blue` layout): offsets 0 → `RED_OVER_BLUE`; red's offset −1 (2999) → `BLUE_OVER_RED`.
  - `queue_zwrite_blend_draws_first_and_hides_what_is_behind`: red with `transparentWithZWrite` (2501) in front, plain blue behind → red alone, `[128, 0, 0, 128]`; red offset 20 and blue offset −20 (clamped to 2510 and 2991) → the same.
  - `queue_discards_nearly_clear_blend_pixels`: the z-written front square with base alpha 0 → blue alone shows, `[0, 0, 128, 128]`.
  - `queue_perspective_blend_sorts_by_distance`: identity view, perspective `yfov` 1; red 1 m square at (0, 0, −4) (distance 4, view z −4), blue at `translation(3, 0, −3.5) × scaling(10, 10, 1)` (distance 4.61, view z −3.5) → `RED_OVER_BLUE` (by view z it would be `BLUE_OVER_RED`).
  - `queue_orthographic_blend_sorts_by_view_depth`: the same draws, orthographic height 2 → `BLUE_OVER_RED`.
  - `queue_coplanar_opaque_later_draw_wins`: two unlit opaque squares at z = 0, red draw 0, green draw 1 → green (`LessEqual`).
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer queue_` → fail.
- [ ] **Step 3:** Implement. **Step 4:** `queue_` and phase 1's `blend_`/`center_`/`opaque_` tests pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): UniVRM render queues with offsets and distance sorting`.

### Task 7: Outlines

**Files:** Modify `crates/gpu-renderer/src/scene.rs` (outline draws), `scene_mtoon.wgsl` (`vs_outline`, `fs_outline_opaque`, `fs_outline_mask`, `fs_outline_blend`); tests in `tests/scene.rs`.

**Decisions:**
- After a draw's main pass, its outline is drawn when its material is MToon and `outlineWidthMode` is not `none`, whatever the width (UniVRM's outline pass runs then; at width 0 it still draws the back faces). Unlit has none.
- Pipeline: `Cull::Front` whatever `doubleSided` (UniVRM's `Cull Front`), winding from the world's determinant, the material's `Alpha` (blend, depth write, mask coverage), `DepthBiasState { constant: 1, slope_scale: 1.0, clamp: 0.0 }` (Unity's `Offset 1, 1`: pushed away from the camera under depth 0 near, 1 far).
- Fragment: `outline × mix(vec3(1.0), mtoon_color(surface, true), outline.w)`. `main_only`: the built-in pipeline's outline pass is ForwardBase only, so additional lights do not reach it. Its normal is the outward one: UniVRM negates the normal for the outline and the back-face flip negates it again. Alpha as the main pass.
- The width texture is sampled in the vertex stage with `textureSampleLevel(…, uv0, 0.0).g`, `uv0` the geometry's raw UV.

```wgsl
let width = draw.outline_width.x * textureSampleLevel(outline_width, s, in.uv0, 0.0).g;
let world_position = (draw.world * skinned_position).xyz;
let n = normalize(draw.normal_matrix * skinned_normal);
if mode == 1 {                                            // world
    clip = frame.view_projection * vec4(world_position + n * width, 1.0);
} else {                                                  // screen
    clip = frame.view_projection * vec4(world_position, 1.0);
    let n_view = normalize((frame.view * vec4(n, 0.0)).xyz);
    let n_clip = (frame.projection * vec4(n_view, 0.0)).xy;
    if dot(n_clip, n_clip) > 0.0 {
        var offset = normalize(n_clip) * 2.0 * width * min(clip.w, frame.projection[1][1]);
        offset.x *= frame.params.y;                       // canvas height / width
        clip = vec4(clip.xy + offset * saturate(1.0 - n_view.z * n_view.z), clip.zw);
    }
}
```

- [ ] **Step 1:** Tests (prefix `outline_`; `shell()`: the 1 m square facing +Z plus its twin with reversed winding facing −Z, eight vertices, every normal radial in the plane `(±1, ±1, 0)/√2`, so the outline is the square grown on every side; orthographic height 2, 64×64, 32 px a metre: the square covers x 16..48; main color green by emission, outline red, mix 0):
  - `outline_world_width_is_in_metres`: world mode, width 0.17677670 (4 px of ring): (14, 32) red, (50, 32) red, (10, 32) `CLEAR`, (32, 32) green (the depth bias keeps the draw's own face in front); with `world = scaling(0.5, 0.5, 0.5)` → (22, 32) red, (18, 32) `CLEAR`.
  - `outline_screen_width_is_a_share_of_the_height`: screen mode, width 0.08838835: the same four pixels; `scaling(0.5, 0.5, 0.5)` → (22, 32) red; orthographic height 4 (`P[1][1]` 0.5 caps `clip.w` 1; square x 24..40, ring 2 px) → (23, 32) red, (21, 32) `CLEAR`.
  - `outline_screen_width_corrects_for_aspect`: a 128×64 canvas and layer (model `canvas: [128.0, 64.0]`, drawn through a helper `scene_of(width, height)` that is `scene_at` on `empty_scene(width, height)`), screen mode, width 0.08838835 (square x 48..80, y 16..48; ring 2.53 px across, 5.06 px up) → (46, 32) and (64, 12) red, (44, 32) and (64, 9) `CLEAR`.
  - `outline_width_texture_uses_raw_uv0_green`: world mode, width 0.17677670, a 1×1 white base texture with transform offset (0.5, 0), width texture linear 2×1 nearest clamp `[(0, 255, 0) | (0, 0, 0)]` → left ring (14, 32) red, right (50, 32) `CLEAR`.
  - `outline_color_mixes_toward_the_shaded_color`: base black, emissive (0.5, 1, 0.5), outline (1, 0.5, 1): mix 0 → `[255, 188, 255, 255]`, mix 1 → `[188, 188, 188, 255]`, mix 0.5 → `[225, 188, 225, 255]`.
  - `outline_shading_sees_the_main_light_only`: base and shade white, emissive 0.25, lights `[black main, white direction (−1, 0, 0)]`, outline white, mix 1 → (50, 32) 137 (with the second light it would be 225).
  - `outline_none_draws_nothing`: mode none, width 0.17677670 → (14, 32) `CLEAR`.
  - `outline_uses_the_material_blend`: blend material, base alpha 0.5 → (14, 32) alpha within 2 of 128.
  - `outline_screen_normals_facing_the_camera_stay_put`: screen mode, the twins' normals (0, 0, ±1) → (14, 32) `CLEAR`, (32, 32) green, no NaN.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer outline_` → fail.
- [ ] **Step 3:** Implement. **Step 4:** `outline_` and `queue_` pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): MToon outlines in world and screen widths`.

### Task 8: Material color overrides

**Files:** Modify `crates/gpu-renderer/src/scene.rs` (`draw_uniform`, `invalid_scene_frame`); tests in `tests/scene.rs`.

**Decisions:** Each `Some` override replaces the material's factor for that frame (`shade_color` → `shadeColorFactor`, `emission_color` → `emissiveFactor`, `matcap_color`, `rim_color` → `parametricRimColorFactor`, `outline_color`); `base_color`, `texture_offset`, `texture_scale` act on MToon as on unlit. Non-finite overrides are refused like phase 1's (`"draw {i}'s emission color override is not finite"`).

- [ ] **Step 1:** Tests (prefix `override_`):
  - `override_replaces_mtoon_colors`, one case each, override green `[0, 1, 0]` over a red factor, expected `[0, 255, 0, 255]`: emission (base black); shade (toony 0.5, light `direction (0, 0, 1)` so shade 0, base white); matcap (white 1×1 matcap, rim mix 0, base black); rim (lift 1, power 1, rim mix 0, base black); outline (Task 7's shell, world 0.17677670, pixel (14, 32)); base color `[0, 1, 0, 1]` over red with `ambient[3] = [1; 3]`.
  - `override_mtoon_colors_on_unlit_draws_are_ignored`: an unlit white square with `emission_color` set → white, no error.
  - `override_not_finite_is_refused`: `emission_color = [NaN, 0, 0]` → message contains `"emission color override"`.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer override_` → fail.
- [ ] **Step 3:** Implement. **Step 4:** `override_` and phase 1's `overrides_replace_the_material_texture_transform_and_color` pass; then the task checks.
- [ ] **Step 5:** Commit `feat(gpu-renderer): MToon color overrides per frame`.

### Task 9: The turntable's cube in MToon

**Files:** Modify `examples/plugins/turntable/src/lib.rs` (and its tests), `examples/plugins/turntable/README.md`, `examples/plugins/turntable/demo.tsx` only if its caption describes the look.

**Decisions:**
- The cube's material becomes MToon, opaque, single-sided: the checker as `baseColorTexture` and `shadeMultiplyTexture`, shade color (0.6, 0.55, 0.7), shading shift and toony at their defaults, rim color (0.3, 0.3, 0.4) with lift 0.1, power 3, mix 1, outline `worldCoordinates` 0.01 m of (0.15, 0.1, 0.1) at lighting mix 0.5. The pane and the leaf stay unlit, so the before and after stills differ only in the cube.
- Each frame lights the scene with the plugin's reference preset: one white directional light, `direction (−0.4, −1.0, −0.7)`, intensity 1, and a flat ambient `ambient[3] = [0.25, 0.25, 0.28]`.
- The cube keeps per-face normals: its outline breaks at the corners, as MToon's does on hard edges in UniVRM; the README says so.

- [ ] **Step 1:** Unit tests in `src/lib.rs`: `the_cube_is_toon_shaded_and_outlined` (material 0 is `MaterialKind::Mtoon` with `OutlineWidthMode::WorldCoordinates` and width 0.01, base and shade textures 0; materials 1 and 2 unlit); `the_scene_is_lit_by_one_light_and_ambient` (any frame has one directional light, `ambient[3]` non-zero and every other coefficient zero). The process test `the_renderer_draws_the_process_scene` is unchanged and must still pass.
- [ ] **Step 2:** `cargo test -p celesta-plugin-turntable` → fail.
- [ ] **Step 3:** Implement; update the README's "How it works" (MToon cube, the light, the outline).
- [ ] **Step 4:** `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-plugin-turntable` passes; `node examples/plugins/build.mjs turntable`; `cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx <after>.png --frame 30 --plugin-dir examples/plugins/dist` gives the "after" still, and the same command on this branch's base (phase 1) gives the "before"; then the task checks.
- [ ] **Step 5:** Commit `feat(examples): toon-shade and outline the turntable's cube`.

### Task 10: Docs

**Files:**
- Create: `docs/handoff/2026-11-03-plugin-scene-mtoon.md` (the format of `2026-11-03-plugin-scene-content.md`: what it adds, MToon's fields and defaults, lights and the SH polynomial, queues and sorting, outlines, overrides, how Celesta draws it (bind groups, the frame resource cache), the turntable change, What landed, Not done, Checks)
- Modify: `docs/handoff/2026-11-03-plugin-scene-content.md` (its "Not done" says that MToon, lights, outlines, the normal matrix, and the per-frame caching landed in the new note), `packages/website/content/en/docs/05-(more)/05-plugins.md` and `packages/website/content/ja/docs/05-(more)/05-plugins.md` ("Write a plugin": scenes are drawn unlit or with MToon, the toon shading VRM uses, under directional lights and ambient light, with outlines; the `turntable` row; the handoff list names the new note), `skills/celesta/references/plugins.md` (the same, where it says "for now unlit" and lists the notes)

**Not done** lists at least: shadows (`atten` stays 1); glTF PBR; point and spot lights; at most 4 directional lights; the mask edge's sub-pixel difference from UniVRM's clip; `V` per fragment rather than per vertex; outline breaks at hard edges; `KHR_texture_transform` rotation, which UniVRM's `_MainTex_ST` cannot hold, is honoured here; emissive strength folded into `emissiveFactor` by the plugin; phase 1's still-open items (straight-alpha mip fringes, the 128 MiB storage binding limit, the canvas and buffer caps).

- [ ] **Step 1:** Write the docs. **Step 2:** `pnpm --dir packages/website run build` passes.
- [ ] **Step 3:** Commit `docs: MToon, lights, and outlines for scene content`.

### Finish

- [ ] Full checks once: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-plugin -p celesta-plugin-host -p celesta-gpu-renderer -p celesta-plugin-flag -p celesta-plugin-turntable`.
- [ ] Push and open the PR stacked on #657 (`gh stack`), attaching the turntable's before and after stills of frame 30 with `gh pr create --attach`.
