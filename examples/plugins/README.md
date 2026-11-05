# Sample content plugins

Three plugins to start a plugin of your own from:

| Plugin | Kind | What it does |
| --- | --- | --- |
| [`flag`](flag/) | A program and components | `<Flag>` draws an image waving like a flag. The plugin's program (Rust) returns textured meshes for each time, and Celesta draws them. |
| [`turntable`](turntable/) | A program and components | `<Turntable>` draws a checkered cube turning on a turntable. The plugin's program (Rust) returns a 3D scene, with a camera, a light, a cube toon-shaded and outlined by its own WGSL material, and cut-out and see-through unlit materials, and Celesta draws it with depth. |
| [`beep-voice`](beep-voice/) | Components only | `prepareBeepNarration()` voices dialogue lines with game-style beeps, in place of a speech engine such as VOICEVOX, with lip sync. |

The manifest, the protocol, and how Celesta draws meshes are described in
[`docs/handoff/2026-11-01-plugin-system.md`](../../docs/handoff/2026-11-01-plugin-system.md).

## Build and try them

```sh
pnpm install && pnpm run build:runtime   # once, at the repository root
node scripts/plugins/build.mjs examples/plugins/flag examples/plugins/turntable examples/plugins/beep-voice
cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx flag.mp4 --plugin-dir examples/plugins/dist
cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx turntable.mp4 --plugin-dir examples/plugins/dist
cargo run -p celesta-exporter -- --react examples/plugins/beep-voice/demo.tsx beep.mp4 --plugin-dir examples/plugins/dist
```

The repository's plugin build script, `scripts/plugins/build.mjs`, takes
the plugin directories to build (one, or several) and writes one plugin
directory for each under `dist/` beside them:

```text
dist/flag/
  celesta-plugin.json
  bin/windows-x86_64/celesta-plugin-flag.exe   one per platform built
  js/index.js                                  the module entries import
  js/index.d.ts                                its declarations
```

Each run adds this platform's executable (or `--target <triple>`'s) and
keeps those already there, so builds on several platforms can fill one
directory; later builds may pass `--skip-js`. To use the plugins in the
desktop app, add `dist/` with **Add Folder…** in its Plugins tab (or copy a
plugin's folder into your plugin folder) and allow them when it asks, or
with **Allow** in the Plugins tab.

Each sample's `plugin.json` gives its id, version, and module source.
flag's and turntable's also name the Cargo package that builds the program,
whose version must be the same: bump both together, since the build refuses
a plugin whose versions differ. flag's also lists the `components` the app's Add menu
offers, which the build copies into the manifest.

## Moving a sample out of this repository

The samples build inside Celesta's workspace. A plugin kept elsewhere
depends on the crates by version instead (`celesta-plugin = "0.9"`), has a
`[workspace]` table of its own, and points those names at a Celesta checkout
from its `.cargo/config.toml` until the crates are published:

```toml
[patch.crates-io]
celesta-plugin = { path = "/path/to/celesta/crates/plugin" }
```

Its build script then runs `cargo build` in its own directory and takes the
runtime's declarations, `tsc`, and `esbuild` from the checkout's
`packages/cli`, as `scripts/plugins/build.mjs` does.
