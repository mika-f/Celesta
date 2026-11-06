# Plugin paths and closing undrawn plugin models (2026-11-06)

Issue #690: three gaps in how Celesta resolves paths a plugin gives it, and
in how long it keeps plugin models open.

## Specification

### Relative texture paths are refused

- The protocol gives a model's texture files as absolute paths
  (`2026-11-01-plugin-system.md`), but nothing enforced it. A plugin runs
  with its own folder as its working directory, so `textures/skin.png`
  reads naturally to it, while the renderer opened the path against
  Celesta's working directory (`/` for a macOS app started from the
  Finder): the texture failed to load, or another file of the same name
  loaded.
- `PluginHost::open_remote` (`crates/plugin-host/src/host.rs`), which both
  the first open and a reopen after a restart go through, checks the
  decoded model's textures, meshes' and scenes' alike. The first relative
  `TextureSource::Path` makes the open fail with
  `PluginError::Failed { message: "texture <n>'s path \"<path>\" is relative; a plugin gives texture files as absolute paths" }`,
  and the model is closed in the plugin's process, as a reopen of the wrong
  kind already was. Resolving the path against the plugin's folder was the
  other option; refusing keeps the protocol as written.
- The check is in the host rather than in `decode_opened`: `celesta-plugin`'s
  round-trip tests send `C:/a.png`, which is relative outside Windows, and
  the protocol crate does not know the platform a path is for.

### Manifest paths are split on both separators

- `is_contained_path` reads `module`, `types`, and each `executable` with
  either separator, since one manifest serves every platform, but the paths
  were then joined as written. On macOS and Linux `js\index.js` was one
  file name: the fingerprint hashed it as missing, approval succeeded, and
  bundling failed with "Could not resolve". On Windows a manifest's
  `bin/windows-x86_64/flag.exe` gave a path of mixed separators, which
  showed in messages and as a console window's title.
- `celesta_plugin_host::manifest_path(directory, path)` splits on `/` and
  `\`, leaves out empty and `.` parts, and pushes the rest onto the
  directory, so the result uses the platform's separator.
  `PluginEntry::executable`, `module`, and `types` go through it (via
  `PluginEntry::file`), and so do the containment check's `canonicalize`,
  `fingerprint`, and the from-the-internet check, so what is checked is
  what is used. The fingerprint still hashes each name as written.
- `packages/cli/src/plugins.ts` keeps its own discovery; its
  `manifestPath` does the same with `path.join`.
- A plugin whose manifest used `\` gets a different fingerprint now that its
  files are found, so the app asks about it once more.

### Plugin models no layer draws are closed

- `MeshState::models` was emptied only when the content provider was
  replaced, so every model a renderer opened kept its GPU textures and
  buffers in Celesta, and its parsed model in the plugin process, until the
  project was reopened: changing a VRM layer's `src` in the editor, or an
  export that switches models between scenes, held them all.
- `MeshState` counts renders (`begin_render`, from `prepare_draws`), and
  `plugin_texture` stamps the model it draws with the count right after
  opening it, before the frame is asked for and checked, so a frame that
  fails still keeps its model. After the walk, also when it failed part
  way, `close_idle_models` closes on the provider and drops each model not
  drawn in the last `MODEL_IDLE_RENDERS` (300) renders. Dropping a loaded
  scene releases its hold on shaders and pipelines, which the materials
  cache can then drop.
- 300 renders is about five seconds of playback or export at 60 fps (ten
  at 30). Scene canvases are evicted after one render, but reopening a model
  means the plugin reads and parses its file again (a VRM avatar can take a
  second or more), so scrubbing or cutting back to a model shortly keeps
  it. A byte-bounded LRU was the other option; the render count needs no
  estimate of each model's size. An editor that is idle renders nothing,
  so a paused preview keeps its models.

## Checks

- `celesta-plugin-host`: `host::tests::a_relative_texture_path_is_refused_by_name`
  (meshes and scenes; the message names the texture's index and path; an
  absolute path and inline pixels pass) and
  `manifest::tests::manifest_paths_use_this_platforms_separator` (`js\\index.js`
  and `./js//index.d.ts` resolve to the files on disk; an executable
  written with `/` uses the platform's separator, with no `/` on Windows).
- `celesta-gpu-renderer`: `tests::plugin::models_no_layer_draws_for_a_while_are_closed`
  renders model `a` once, then only `b`: `a` is still open after
  `MODEL_IDLE_RENDERS - 1` renders and closed at the next; `b`, drawn in
  every render, stays; an empty render counts; `a` opens again when drawn.
- `packages/cli`: `plugins.test.mts` resolves a `module` written as
  `js\\index.ts` to the file on disk.
- Run on macOS (Apple Silicon, Metal): `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check`, and
  `pnpm --dir packages/cli exec vitest run test/plugins.test.mts`. The
  Windows-only assertion (no `/` in `executable()`) runs in CI.
