# 2D custom shaders: device refusals and the `celesta` prefix (2026-11-07)

Fixes mika-f/Celesta#663. 2D custom shaders (`defineShader`, compiled in
`crates/gpu-renderer/src/shader.rs`) shared the WGSL declaration checks
with plugin scene materials (`wgsl.rs`) but not their hardened compile
path, and reserved the `celesta` prefix only in lowercase.

## Specification

- **A device refusal is the shader's error, never a panic.** The module and
  pipeline of a custom shader, and of the built-in color filter and
  stylize effect pipelines, are made inside validation, internal, and
  out-of-memory error scopes together (`wgsl::scoped`), as a scene
  material's are. Before, only a validation scope was pushed, so an
  internal error (a backend's compiler refusing what naga accepted, such
  as Metal refusing a fragment function past its stack) reached the
  device's uncaptured-error handler, which panics.
- **Custom shaders:** a refusal fails the frame with
  `invalid custom shader <name>: <the device's message>` and is cached
  like a compile error: the shader is not compiled again until its source
  changes. Running out of memory fails the frame the same way but is not
  cached; the next frame that lists the shader compiles it again.
- **Built-in filters:** a device failure fails the frame with
  `invalid <filter> filter: the device could not compile it: …` and the
  pipeline is tried again at its next use. A built-in filter's WGSL that
  does not pass naga is Celesta's bug and still panics.
- **The prefix rule matches materials'.** A 2D shader may not declare a
  name, or a parameter, that starts with `celesta` in any letter case:
  `Celesta_x`, `CELESTA_k`, and `CeLeStA_t` are refused as `celesta_x`
  was (``declares `Celesta_x`; names starting with `celesta` belong to
  Celesta``). This is user-visible: such names were accepted before.
  `defineShader` in `@celesta.studio/shader` refuses such parameter names
  too, so the error comes before the render.

## Implementation

- `scoped` moved from `scene.rs` to `wgsl.rs`, which now also holds
  `reserved`, the case-insensitive prefix test (formerly
  `scene_material::material_reserved`). `wgsl::Rules` lost its `reserved`
  field: both callers use the one rule.
- `ShaderProcessor::compile` returns a `CompileFailure`: `Invalid` (the
  WGSL breaks the rules or naga refuses it) or `Device(wgpu::Error)`.
  `begin_scene` caches every failure but `Device(OutOfMemory)`, which also
  drops the shader's cache entry. `compile_builtin` turns `Device` into the
  filter's error and panics on `Invalid`.
- `assemble` uses `wgsl::reserved` for parameter names. The JS check in
  `packages/shader/src/index.ts` uses `/^celesta/i`.
- Test hook: `ShaderProcessor::refuse_pipeline` (`#[cfg(test)]`) makes the
  next shader that passes naga fail on the device with the given error,
  as `ScenePipelines::refuse_pipeline` does for materials, so the tests
  run on every backend without depending on a driver's refusal.

## Checks

- New tests:
  `tests::shader::a_shader_the_device_refuses_is_an_error_kept_until_it_changes`,
  `tests::shader::a_shader_the_device_runs_out_of_memory_compiling_is_tried_again`,
  and `tests::shader::a_built_in_filter_the_device_refuses_is_an_error_tried_again`
  (they need the hook, so they do not compile before the fix); new cases in
  `tests::shader::reports_errors_in_the_authors_code_by_line_and_column`
  (`Celesta_x`, `CELESTA_k`, `CeLeStA_t`, and a `Celesta_x` parameter),
  which fail against the case-sensitive rule; and two cases in
  `packages/shader/test/shader.test.mts`, which failed before the fix.
- `cargo test -p celesta-gpu-renderer` (including the GPU tests),
  `cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check`.
- `pnpm --filter @celesta.studio/shader test`.
- No rendering change: shaders that compiled before draw the same pixels.
