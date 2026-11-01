# Content plugins (2026-11-01)

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
  which depends on nothing but serde (and libc on Unix).
- **Deterministic content.** What a plugin returns for a frame is a function
  of the layer's content alone (its assets, data, and time). Celesta may ask
  for frames in any order, more than once, and from several processes.
- **Celesta draws.** A plugin returns geometry, not pixels: textured
  triangle meshes with the per-mesh state Celesta's GPU renderer draws.
  Rendering quality, platforms, and GPU backends stay Celesta's.

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
  Celesta premultiplies and builds mipmaps.
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
- a single-sided mesh draws only triangles whose vertices turn clockwise on
  the canvas.

## Provider interface (in-process)

`celesta_plugin::ContentProvider`:

```rust
fn open(&mut self, plugin: &str, assets: &BTreeMap<String, PathBuf>)
    -> Result<(ModelId, MeshModel), PluginError>;
fn frame(&mut self, model: ModelId, time: f64, data: &serde_json::Value)
    -> Result<MeshFrame, PluginError>;
```

`GpuRenderer::with_content_provider` installs one, as
`with_video_decoder` installs the decoder. The renderer caches a model's
GPU textures by plugin and asset paths. The plugin host implements the
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
| `{"type":"hello","protocol":1}` | `{"type":"hello","protocol":1,"plugin":"example","version":"0.1.0"}` |
| `{"type":"open","assets":{"model":"C:/…/model.json"}}` | `{"type":"opened","model":1,"canvas":[w,h],"textures":[{"path":…}],"meshes":[{"texture","blend","doubleSided","mask","vertices","indices"}]}` + payload: every mesh's UVs (`f32` pairs) then every mesh's indices (`u32`) |
| `{"type":"frame","model":1,"time":2.5,"data":{…}}` | `{"type":"frame","meshes":[{"index","visible","opacity","multiply","screen"}]}` + payload: the positions (`f32` pairs) of each listed mesh, in list order |
| `{"type":"close","model":1}` | `{"type":"closed"}` |
| any | `{"type":"error","message":"…"}` |

A plugin exits when its stdin closes. Its stderr goes to Celesta's log.
`celesta_plugin::protocol::serve` runs this loop over a `ContentProvider`;
on Unix it answers through a duplicate of stdout and points stdout at
stderr, so native libraries that print cannot corrupt the protocol.

## Discovery and lifecycle

- A plugin is a directory with `celesta-plugin.json`:

  ```json
  {
    "id": "example",
    "version": "0.1.0",
    "protocol": 1,
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
- Plugin directories are found under each path of `CELESTA_PLUGIN_PATH`
  (the OS path-list separator) and each `--plugin-dir`; a path is a plugin
  directory itself or a directory of them.
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
| `crates/plugin` (`celesta-plugin`) | Meshes, `ContentProvider` (with `close`), and `protocol`: framing, model/frame encoding, `serve`/`serve_on`. Depends on serde, and libc on Unix. |
| `crates/plugin-host` (`celesta-plugin-host`) | `discover`, `plugin_paths`, the manifest types, `platform`, and `PluginHost` (a `ContentProvider`; `stop` restarts a plugin). |
| `crates/gpu-renderer/src/mesh.rs`, `mesh.wgsl` | The mesh drawing; `with_content_provider`; models and textures cached by plugin and asset paths; models and frames that do not fit (bad indices, masks, vertex counts) are `InvalidPluginContent`; plugin content without a provider is `MissingContentProvider`. |
| `crates/renderer` | Refuses plugin content (`UnsupportedPlugin`). |
| `crates/exporter`, `crates/editor` | Install `PluginHost::discover(&[])`; the exporter's `--plugin-dir` (repeatable) goes before `CELESTA_PLUGIN_PATH` for both the host and the React runtime. |
| `crates/react-bridge/src/project_types.rs` | Copies installed plugins' declarations to `.celesta/plugins/<id>/index.d.ts` on set-up and on every check; the staged tsconfig maps `@celesta-plugin/*` there. |
| `packages/cli/src/plugins.ts` | Resolves `@celesta-plugin/<id>` to the plugin's `module` and bundles it into the entry. |

Not done: an editor UI to list, enable, and reload plugins (the editor reads
`CELESTA_PLUGIN_PATH` from its environment); request timeouts (a plugin that
hangs blocks the frame); plugin signing or an allowlist; `@celesta/web` and
the CPU renderer; a sample plugin in this repository; the user guide and
the Celesta skill.

## Checks

- The GPU renderer's tests (`crates/gpu-renderer/src/tests/plugin.rs`) draw
  made-up meshes through a fake provider: masks and inverted masks, colors,
  opacity, and meshes that move with the time; frames that do not fit the
  model, and plugin content without a provider, fail cleanly.
- `celesta-plugin`'s tests round-trip models (inline and file textures,
  masks) and frames, reject short payloads, and serve a whole session.
- `celesta-plugin-host`'s tests discover plugin directories from both
  layouts and prefer a platform's exact `<os>-<arch>` entry over its OS's.
- `project_types` copies plugin declarations into `.celesta/` and removes
  stale ones.
- An out-of-tree plugin (not part of this repository) built against this
  code with the `[patch.crates-io]` set-up above, on Windows 11 (Vulkan) and
  on Linux x86_64, served real content: exported through the plugin process
  from a `--plugin-dir`, its frames were byte-identical to the same content
  drawn in-process, for out-of-order times and after a restart.
