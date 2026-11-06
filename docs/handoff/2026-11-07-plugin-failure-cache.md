# Plugin failures: not retried on every preview frame (2026-11-07)

Fixes mika-f/Celesta#687. The preview draws every frame, and a plugin
layer whose plugin could not start, or whose model could not be opened or
loaded, asked again on each one. Each frame paid the plugin's start-up and
model load on the preview thread for the same error, which is the reason
the editor already caches a failed React bridge and the renderer caches
refused material pipelines. An export stops at the first error, so only
the preview was affected.

## Specification

- **A plugin whose process cannot start is not started again** until
  `PluginHost::stop`. A start fails when the program cannot be spawned (or
  has no executable for this platform), its `hello` fails, or its `hello`
  names another plugin id. Later requests return the same
  `PluginError::Unavailable` without spawning. Reload Plugins builds a new
  host (through `EditorView::from_document`), so it tries again.
- **Only a process that started is restarted.** `retry` restarts the
  process and tries once more only when the `Unavailable` came from a
  running process (it exited, missed its deadline, or fell out of step),
  not from a failed start.
- **A model that could not be opened or loaded is not asked for again.**
  The renderer (`MeshState::failed`) keeps the error by `ModelKey` (plugin
  and asset paths) and returns it for the next frames without asking the
  provider. This covers a failed `open` (any `PluginError`) and a refused
  load (`InvalidPluginContent`, or `ImageDecode` for a texture file). Each
  frame still fails with the same error and the same text, naming the
  frame's layer, so the editor's Problems shows it on every frame.
- **Running out of memory is not kept.** A scene whose shader or pipeline
  ran out of GPU memory is loaded again on the next frame, which may not,
  as `ScenePipelines` already tries such a shader or pipeline again.
- **Fixing an asset tries again.** With the error, the renderer keeps each
  asset file's length and modification time. When one differs (a file
  replaced in place at the same path), the next frame opens the model
  again. A layer whose asset paths change has another key. Replacing the
  provider clears the cache.
- **A model reopened as another kind stays refused.** When a restarted
  process reopens a model as another kind (meshes, then a scene), the host
  closes the reopened model and remembers the refusal on its `OpenModel`.
  Later frames return it without reopening, until `stop`.

## Implementation

- `PluginError` (`celesta-plugin`) derives `Clone`, so both caches can
  return the error they kept.
- `PluginHost` (`crates/plugin-host/src/host.rs`): `failed` holds each
  failed start's message by plugin id. `process` returns it before
  starting, and records a failed start. `retry` skips the restart for a
  plugin in `failed`. `OpenModel::refused` holds the wrong-kind refusal;
  `frame` returns it first. `stop` clears both for the plugin.
- `MeshState` (`crates/gpu-renderer/src/mesh.rs`): `failed` maps a
  `ModelKey` to a `FailedModel` (asset stamps and a `FailedReason`).
  `FailedReason::new` keeps what an error needs to be made again: the
  `PluginError`, an `ImageDecode`'s asset and message, or an
  `InvalidPluginContent` message (any other error's text, which a load
  does not return today). `FailedReason::error` makes it again for a
  layer. `ImageDecode`'s `image::ImageError` is not `Clone`, so it is made
  again as an `IoError` with the first error's text, which reads the same.
  `plugin_texture` checks the cache before `open`, and records a failed
  `open` or load, except a scene load after which
  `ScenePipelines::ran_out_of_memory` is set. `prepare` clears that flag
  as it begins and sets it where it returns an out-of-memory error (a
  shader compile or a pipeline). Without it, the existing
  `material_shaders_out_of_memory_is_tried_again` and
  `material_pipelines_the_device_refuses_are_not_made_again` failed.
- `celesta-gpu-renderer` gains `celesta-plugin-host` as a dev-dependency,
  for a test that runs a real `PluginHost` with fake plugin programs.

Not changed: a model that opens but whose process crashes on every
`frame` is still restarted and reopened once per frame, as before. A
plugin that crashes on one model must keep working for the others, so a
crash is not remembered across requests.

## Checks

Unix tests in `crates/plugin-host/src/host.rs`, with shell-script plugins
that print answers encoded by the test and count their starts:

- `a_plugin_that_cannot_start_is_not_started_again_until_stopped`: a
  plugin whose `hello` names another id starts once over two `open`s
  (four times before), and again after `stop`.
- `a_plugin_that_exits_in_open_is_started_again_once`: one `open` of a
  plugin that exits starts it twice (unchanged).
- `a_model_reopened_as_another_kind_is_not_reopened_again`: the second
  frame after a wrong-kind reopen returns the same refusal and starts
  nothing. Before, it reopened the model and got the plugin's next answer.

GPU tests in `crates/gpu-renderer/src/tests/plugin.rs`:

- `a_model_that_fails_to_open_is_not_opened_again`: a counting provider
  whose `open` fails is asked once over two renders, with the same error.
- `a_model_that_fails_to_load_is_not_opened_again`: an undecodable texture
  file (`ImageDecode`) and a `0x0` texture (`InvalidPluginContent`) each
  open once over two renders, with the same error and variant.
- `a_failed_model_is_opened_again_once_its_assets_change`: rewriting the
  asset file in place opens the model again, once.
- `a_plugin_that_cannot_open_is_not_started_for_every_frame` (Unix):
  through `PluginHost`, two frames start a plugin whose `hello` names
  another id once, and one that exits in `open` twice (four times before).

Each new test except `a_plugin_that_exits_in_open_is_started_again_once`
was run against the previous `host.rs` or `mesh.rs` and failed. The fake
plugin tests passed 15 runs (host) and 8 runs (GPU, `CELESTA_REQUIRE_GPU=1`)
in a row on macOS (Apple GPU).

Also run on macOS: `CELESTA_REQUIRE_GPU=1 cargo test --workspace
--no-fail-fast` (all pass, none skipped), `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo fmt --all -- --check`,
`git diff --check`, and `cargo check --manifest-path plugins/Cargo.toml
--workspace --all-targets --locked` (for `PluginError: Clone`).
