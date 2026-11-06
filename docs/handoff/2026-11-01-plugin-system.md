# Content plugins (2026-11-01)

Later notes change parts of what this one describes: where plugins are
found (`2026-11-02-plugin-search-paths.md`), project trust and plugin
approval (`2026-11-02-plugin-trust.md`), the editor's Plugins tab
(`2026-11-02-plugins-panel.md`), the manifest's `components`
(`2026-11-02-plugin-add-menu.md`), 3D scene content, notices, and the
current `ContentProvider` (`2026-11-03-plugin-scene-content.md`), and the
manifest's `roles` with codec plugins (`2026-11-05-codec-plugin.md`).

The goal: content that needs code Celesta cannot ship (a proprietary
runtime, or anything whose license must stay apart from the GPL build of
the official binaries) runs in a plugin process of its own, and Celesta
itself keeps only generic parts and a published protocol.

## Principles

- **Celesta knows nothing plugin-specific.** No plugin's type, crate, or
  package is a dependency of the core crates, the renderer, the exporter,
  the editor, or the bundled runtime.
- **Arm's length.** A plugin is a separate executable that talks to Celesta
  over its stdin/stdout with the documented protocol below. It never links
  Celesta's binaries; it may use the MIT/Apache `celesta-plugin` crate,
  which depends on nothing of Celesta's (serde, serde_json, thiserror,
  and libc).
- **Deterministic content.** What a plugin returns for a frame is a function
  of the layer's content alone (its assets, data, and time). Celesta may ask
  for frames in any order, more than once, and from several processes.
- **Celesta draws.** A plugin returns geometry, not pixels: textured
  triangle meshes with the per-mesh state Celesta's GPU renderer draws (or,
  since `2026-11-03-plugin-scene-content.md`, a 3D scene). Rendering
  quality, platforms, and GPU backends stay Celesta's.

## Content model

`LayerContent::Plugin`:

```jsonc
{
  "type": "plugin",
  "plugin": "example",                // the plugin's id
  "assets": { "model": { /* ResolvedAsset */ } },
  "time": 2.5,                        // seconds on the content's own clock
  "data": { /* plugin-defined JSON */ }
}
```

- `assets` are resolved like any other asset (relative to the entry,
  remote downloads, absolutized for companion projects); the plugin
  receives absolute local paths.
- `data` is opaque to Celesta and part of what a frame depends on.
- The layer's size is the content's canvas, which the plugin reports.
- Only the GPU renderer draws it. The CPU reference renderer and
  `@celesta/web` refuse it.

## Meshes

A model (`MeshModel`, sent once per opened asset set):

- `canvas: [width, height]` — the layer's size in pixels.
- `textures` — image files (absolute paths) or inline RGBA, straight alpha.
  Celesta premultiplies and builds mipmaps. `PluginHost` refuses a model
  with a relative texture path (a `Failed` that names the texture) and
  closes it in the plugin: the plugin runs in its own folder, but Celesta
  would open the path against its own working directory. See
  [`2026-11-06-plugin-paths-model-eviction.md`](2026-11-06-plugin-paths-model-eviction.md).
- `meshes` — for each: `texture`, `blend` (`normal`, `additive`,
  `multiplicative`), `doubleSided`, optional `mask { meshes, inverted }`
  (the union of those meshes' texture alpha clips this one), `uvs`
  (`v` counting up from the texture's bottom row), and `indices`
  (triangles).

A frame (`MeshFrame`, per request): meshes in drawing order, each with its
`index`, `visible`, `opacity`, `multiply` and `screen` RGB, and `positions`
in canvas pixels (y down). Every mesh is listed, visible or not, as hidden
meshes can still clip others.

Celesta draws them onto a transparent canvas of the model's size,
premultiplied, which composites as the layer's texture:

- color = texture × `multiply`, then `screen` over it (`c + s·a − c·s`),
  then × `opacity`;
- `normal` is source over; `additive` adds color; `multiplicative`
  multiplies the canvas by the color (`dst·src + dst·(1 − a)`); the last
  two leave the canvas's alpha;
- a masked mesh is multiplied by the masks' coverage (or one minus it when
  `inverted`): the masking meshes' texture alpha alone, whatever their
  visibility, opacity, or colors;
- a single-sided mesh draws only triangles whose vertices turn
  counter-clockwise as seen on the canvas (top-left, bottom-left,
  bottom-right).

## Provider interface (in-process)

`celesta_plugin::ContentProvider`:

```rust
fn open(&mut self, plugin: &str, assets: &BTreeMap<String, PathBuf>)
    -> Result<Opened, PluginError>;
fn frame(&mut self, model: ModelId, time: f64, data: &serde_json::Value)
    -> Result<ContentFrame, PluginError>;
fn close(&mut self, _model: ModelId) {}
```

`Opened` is the model's id, its `content` (`ContentModel::Meshes(MeshModel)`
for the meshes above, or `ContentModel::Scene(SceneModel)`), and its
`notices`; a frame is `ContentFrame::Meshes(MeshFrame)` or
`ContentFrame::Scene(SceneFrame)`, of the kind the model was opened as.
`MeshModel` and `MeshFrame` convert with `.into()`. (Before
`2026-11-03-plugin-scene-content.md`, `open` returned
`(ModelId, MeshModel)` and `frame` a `MeshFrame`.)

`GpuRenderer::with_content_provider` installs one, as
`with_video_decoder` installs the decoder. The renderer keeps each opened
model, with its GPU textures, by plugin and asset paths. Installing
another provider closes those models on the provider that opened them and
drops them, so the new provider opens its own
(`2026-11-03-plugin-scene-materials.md`). The plugin host implements the
trait by routing each plugin id to its process; tests may implement it
directly.

## Protocol (version 1)

Messages in both directions:

```text
u32 LE  header length   │ header: UTF-8 JSON object
u32 LE  payload length  │ payload: raw little-endian arrays the header describes
```

Celesta sends one request at a time and waits for its answer.

| Request | Answer |
| --- | --- |
| `{"type":"hello","protocol":1}` | `{"type":"hello","protocol":1,"plugin":"example","version":"0.1.0","roles":["content"]}` |
| `{"type":"open","assets":{"model":"C:/…/model.json"}}` | `{"type":"opened","kind":"meshes","model":1,"canvas":[w,h],"textures":[{"path":…}],"meshes":[{"texture","blend","doubleSided","mask","vertices","indices"}],"notices":[…]}` + payload: every mesh's UVs (`f32` pairs), then every mesh's indices (`u32`), then inline textures' pixels |
| `{"type":"frame","model":1,"time":2.5,"data":{…}}` | `{"type":"frame","meshes":[{"index","visible","opacity","multiply","screen","vertices"}]}` + payload: the positions (`f32` pairs) of each listed mesh, in list order |
| `{"type":"close","model":1}` | `{"type":"closed"}` |
| any | `{"type":"error","message":"…"}` |

A `frame` answer must depend only on the model, `time`, and `data`, never
on the requests before it: the same frame can be asked for again (by a
later render, an export, or another process) and times in any order.

`kind` is `"meshes"` or `"scene"`; an `opened` without it is meshes. A
frame's header does not say its kind: it is the kind its model was opened
as. `notices` (`[{"level":"info"|"warning","message"}]`) is there only when
the plugin has any. The `scene` kind's `opened` and `frame` are in
`2026-11-03-plugin-scene-content.md`.

A plugin exits when its stdin closes. Its stderr goes to Celesta's log.
`celesta_plugin::protocol::serve` runs this loop over a `ContentProvider`;
it answers through a duplicate of stdout and points stdout at stderr (on
Windows, the C runtime's descriptor 1 and `STD_OUTPUT_HANDLE` too), so
`println!` and native libraries that print cannot corrupt the protocol. A
header is at most 16 MiB and a payload at most 1 GiB; a part's buffer grows
as its bytes arrive (`2026-11-07-plugin-host-robustness.md`).

## Discovery and lifecycle

- A plugin is a directory with `celesta-plugin.json`:

  ```json
  {
    "id": "example",
    "version": "0.1.0",
    "protocol": 1,
    "celesta": ">=0.9.0, <0.10.0",
    "executable": {
      "windows-x86_64": "bin/windows-x86_64/celesta-plugin-example.exe",
      "linux-x86_64": "bin/linux-x86_64/celesta-plugin-example"
    },
    "module": "js/index.js",
    "types": "js/index.d.ts"
  }
  ```

- `executable` maps platforms to the plugin process, relative to the
  directory: `"<os>-<arch>"` (`windows`, `linux`, `macos`; Rust's
  architecture names such as `x86_64` and `aarch64`) or the OS alone for any
  architecture. The host takes this platform's exact entry, else its OS's
  (`celesta_plugin_host::platform`). One directory can carry every
  platform's build.
- `celesta` (required) is the Celesta versions the plugin works with, as a
  Cargo version requirement; `protocol` is the wire format alone. A
  manifest without a readable range is skipped and reported as invalid,
  and one whose range leaves the running Celesta out is skipped and
  reported as `DiscoveryError::Incompatible` (see
  [`2026-11-06-plugin-celesta-range.md`](2026-11-06-plugin-celesta-range.md)).
- `id` is lowercase ASCII letters, digits, `-`, `_`, and `.`, starting with
  a letter or digit (`is_valid_id`), since it becomes one specifier segment
  and one path component (`.celesta/plugins/<id>/`).
- Every path the manifest names (`executable`'s values, `module`, `types`)
  is relative and stays inside the plugin directory: no leading `/` or `\`,
  no `..` part, no `:` (drives and NTFS streams), with either separator
  counting (`is_contained_path`), and a file that exists must not lead
  outside through a link. A manifest that breaks either rule is skipped and
  reported, the Rust host and the CLI alike, so a later plugin with that id
  can still be found.
- Plugin directories are found under each path of `CELESTA_PLUGIN_PATH`
  (the OS path-list separator) and each `--plugin-dir`; a path is a plugin
  directory itself or a directory of them. A relative `CELESTA_PLUGIN_PATH`
  entry is skipped and reported, since its folder would depend on the
  current directory (as `.` on `PATH` does); `--plugin-dir` is made
  absolute against the current directory when it is given.
- `celesta-plugin-host` starts a plugin's process the first time a layer
  needs it, one per renderer (the exporter's, the editor's preview), and
  restarts it once if it dies. A plugin the scene names but no directory
  provides is a scene error.
- The process inherits Celesta's environment, so a plugin can take its own
  settings (such as where a runtime library is) from environment variables.

## JavaScript side

- A plugin's `module` is the component library entries import as
  `@celesta-plugin/<id>`. The CLI resolves that specifier through the same
  plugin directories and bundles the module into the entry, so its own
  `react` and `@celesta/*` imports resolve to the runtime's instances. It
  emits `{ type: 'plugin', plugin: '<id>', … }` through
  `registerHostElement`.
- Project TypeScript support maps `@celesta-plugin/*` to the plugins' `types`,
  copied into `.celesta/plugins/<id>/` with the rest.

## Building a plugin against a checkout

A plugin lives outside Celesta's workspace and needs only a Celesta
checkout to build:

- Its `Cargo.toml` declares `celesta-plugin = "0.9"` and the like, as if the
  crates were on crates.io, and has a `[workspace]` table of its own. A
  `[patch.crates-io]` table in the plugin's `.cargo/config.toml` points those
  names at the checkout's crates; Cargo accepts patches for crates the
  registry does not have. Only the versions have to match the checkout.
- Its JavaScript can take the checkout's built runtime: the staged project
  TypeScript support (`packages/cli/dist/project-types`) for `react` and
  `@celesta/*` declarations, and the CLI's `typescript` and `esbuild`.
- Each platform's executable is built on (or cross-compiled for) that
  platform and added under its `executable` key.

## What landed

| Where | What |
| --- | --- |
| `crates/composition` | `LayerContent::Plugin { plugin, assets, time, data }`. |
| `crates/plugin` (`celesta-plugin`) | Meshes, `ContentProvider` (with `close`), and `protocol`: framing, model/frame encoding, `serve`/`serve_on`. Depends on serde, serde_json, thiserror, and libc. |
| `crates/plugin-host` (`celesta-plugin-host`) | `discover`, `plugin_paths`, the manifest types, `platform`, and `PluginHost` (a `ContentProvider`; `stop` restarts a plugin). |
| `crates/gpu-renderer/src/mesh.rs`, `mesh.wgsl` | The mesh drawing; `with_content_provider`; models and textures cached by plugin and asset paths; models and frames that do not fit (bad indices, masks, vertex counts) are `InvalidPluginContent`; plugin content without a provider is `MissingContentProvider`. |
| `crates/renderer` | Refuses plugin content (`UnsupportedPlugin`). |
| `crates/exporter`, `crates/editor` | Install `PluginHost::discover(&[])`; the exporter's `--plugin-dir` (repeatable) goes before `CELESTA_PLUGIN_PATH` for both the host and the React runtime. |
| `crates/react-bridge/src/project_types.rs` | Copies installed plugins' declarations to `.celesta/plugins/<id>/index.d.ts` on set-up and on every check; the staged tsconfig maps `@celesta-plugin/*` there. |
| `packages/cli/src/plugins.ts` | Resolves `@celesta-plugin/<id>` to the plugin's `module` and bundles it into the entry. |
| `examples/plugins` | Three sample plugins and `build.mjs`, which builds each into `examples/plugins/dist/<id>` (git-ignored). `flag` (a workspace member, `celesta-plugin-flag`) is a program and a component: an image waving as 48 single-sided strips, shaded with multiply and screen colors. `beep-voice` is a module only: a speech channel that voices `planDialogue()` lines with beeps in `prepare()`, as `@celesta/voicevox` does with an engine. `turntable` (`celesta-plugin-turntable`, a workspace member) is a program and a component drawing 3D scene content (`2026-11-03-plugin-scene-content.md`). |
| `packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md` | The user guide's Content plugins chapter. |
| `skills/celesta/references/plugins.md` | The skill's page; `inspect.mjs` lists plugin layers with their assets and data. |

Not done: an editor UI to list, enable, and reload plugins (the editor reads
`CELESTA_PLUGIN_PATH` from its environment); plugin signing or an allowlist; `@celesta/web` and
the CPU renderer; plugins that make sound at render time (the protocol has
frames to draw only; speech channels synthesize in `prepare()` instead); a
published SDK (below). The samples' demos are not type-checked in CI, as
`typecheck:examples` covers `examples/<name>` projects only.
Request timeouts, once also not done here, came with
`2026-11-05-codec-plugin.md` (`celesta_plugin_host::PluginChild`), and
killing a plugin from another thread with
`2026-11-06-plugin-kill-switch.md`.

## Publishing an SDK (not done)

What a plugin author would need from registries instead of a checkout:

- **crates.io: `celesta-plugin`.** It already depends on nothing of
  Celesta's. Path dependencies on it gain versions; `celesta-plugin-host`
  stays unpublished (Celesta's side only). Its semver is separate from the
  protocol version, which the manifest and `hello` carry.
- **npm: a public API for plugin layers.** Plugin modules now call
  `registerHostElement` from `@celesta/react/internal`, which says it is not
  stable. A public `<PluginLayer plugin assets time data>` (or a
  `definePluginLayer()`) should replace it before any plugin is published.
- **npm: declarations for development.** Plugin modules type-check against
  the checkout's staged `packages/cli/dist/project-types`. Either publish the
  `@celesta/*` packages plugins import (as `peerDependencies`; the CLI keeps
  them external at run time) or a types-only package made from the same
  staging. `react` must match the runtime's.
- **npm: a plugin SDK package** with the manifest's type and JSON Schema and
  a build CLI generalizing `examples/plugins/build.mjs` (per-platform
  executables merged into one manifest, `--target`, a handshake check).
- **Language-neutral docs**: the protocol and manifest sections above as a
  versioned specification, and which protocol versions each Celesta accepts.
- **Optionally**, a TypeScript implementation of protocol v1, so a program can
  be written for Node.js; the manifest would then need a command with
  arguments, not only an executable path.
- **Distribution**: a package format (a folder or an archive) and a default
  per-user plugin folder searched without `CELESTA_PLUGIN_PATH`.

## Checks

- The GPU renderer's tests (`crates/gpu-renderer/src/tests/plugin.rs`) draw
  made-up meshes through a fake provider: masks and inverted masks, colors,
  opacity, and meshes that move with the time; frames that do not fit the
  model, and plugin content without a provider, fail cleanly.
- `celesta-plugin`'s tests round-trip models (inline and file textures,
  masks) and frames, reject short payloads, and serve a whole session.
- `celesta-plugin-host`'s tests discover plugin directories from both
  layouts and prefer a platform's exact `<os>-<arch>` entry over its OS's.
  They also skip and report relative paths and manifests with a bad id, a
  path outside their directory (`..`, absolute, a drive), or a file that
  links outside it, and make `--plugin-dir` paths absolute.
- `project_types` copies plugin declarations into `.celesta/` and removes
  stale ones.
- `celesta-plugin-flag`'s tests (`examples/plugins/flag/tests/process.rs`)
  start the built program through `PluginHost` from a plugin directory: its
  answers equal the in-process provider's for out-of-order times, its errors
  come back, and the GPU renderer draws its single-sided strips (which caught
  `MeshInfo::double_sided`'s documented winding being backwards).
- `packages/cli/test/plugins.test.mts`: plugin discovery and resolution
  (with the same skipped ids, paths, and links as the host's, named in the
  missing-plugin error), a
  `<Flag>` layer's content on a sequence's clock, the error for a plugin no
  directory provides, and `beep-voice`'s WAV, cache, and lip sync.
- Both demos exported with `celesta-exporter --plugin-dir examples/plugins/dist`
  on Windows 11 (Vulkan): 150 flag frames, and the beep dialogue with sound
  and moving mouths.
- An out-of-tree plugin (not part of this repository) built against this
  code with the `[patch.crates-io]` set-up above, on Windows 11 (Vulkan) and
  on Linux x86_64, served real content: exported through the plugin process
  from a `--plugin-dir`, its frames were byte-identical to the same content
  drawn in-process, for out-of-order times and after a restart.
