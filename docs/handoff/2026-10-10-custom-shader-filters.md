# Custom shader filters (2026-10-10)

A layer's `shader` prop runs a WGSL function the author writes over the
layer's composited pixels, before `blur`, `shadow`, and `glow`. The design
is `docs/superpowers/specs/2026-10-10-custom-shader-filter-design.md`; this
note records what landed and where it differs.

- Scene model (`celesta-composition`): `Scene.shaders` lists each source
  once (`ShaderSource { id, name?, wgsl, params }`), and
  `LayerEffects.shader` (`LayerShader { id, params, padding }`) refers to it
  with every parameter's components flattened in declared order. Project
  timelines never produce shaders.
- GPU (`crates/gpu-renderer/src/shader.rs`): the author's code goes between
  `shader_prelude.wgsl` (with a generated `Params` struct, every member
  `@align(16)`) and `shader_entry.wgsl`. naga parses and validates the
  module first; declared entry points and bindings are rejected, and
  errors inside the author's code are reported as `line:column` of that
  code. Pipelines and compile errors are cached by id and compared by
  source, and dropped after 300 frames unused. `draw_group` runs the pass on
  the effect canvas after the mask, before the other effects, writing the
  content plus `padding` (at most 512).
- Content box: `uv` and `celesta.content` use the box of the layers'
  shapes (`PreparedLayer::shape`, tracked as `GroupPlan::shape`), not the
  conservative canvas bounds, which are 2px wider for rounding. A nested
  effect's blur does not spread it. A mask intersects it with the mask's
  box, and an inverted mask keeps the children's box; when the intersected
  shape boxes miss but the pixel boxes meet, the pixel box stands in.
- CPU reference renderer: `RenderError::UnsupportedShader`. GPU tests
  (`crates/gpu-renderer/src/tests/shader.rs`) compare against expected
  pixels instead.
- Core (`@celesta/react`): `CommonProps.shader?: ShaderEffect`, an opaque
  class. `@celesta/react/internal` exports `shaderSource()` (checks the
  shape, flattens strings, computes the id as 64-bit FNV-1a over
  `JSON.stringify([wgsl, [[name, type], …]])`) and `shaderEffect()`. The
  walker collects each frame's sources in `frameState` as a list: keying a
  `Map` by the id internalizes it into a thin string, which the flat-string
  check rejects.
- `@celesta/shader` (`packages/shader`): `defineShader`, parameter types
  (`f32`, `vec2`–`vec4`, `color`), defaults, author-facing validation, and
  `wgsl.d.ts`, which `index.d.ts` references with `preserve="true"` (TS 5.5+
  drops reference directives otherwise). The CLI bundles `.wgsl` as text,
  and the editor's reload watcher includes `.wgsl`.
- Not supported, and failing loudly instead of drawing without the shader:
  the browser renderer (`@celesta/web` throws in `SceneCanvas`; the worker
  does not serve `@celesta/shader`), and components resolved for project
  timelines (`createResolver` throws). Component resolutions carry only
  layers, so supporting the latter needs the bridge protocol to carry
  sources and the editor to merge them into the project scene.
- Performance: `docs/performance/custom-shaders.md` and the `shaders`
  bench workload.
