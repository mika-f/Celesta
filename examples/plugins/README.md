# Sample content plugins

Two plugins to start a plugin of your own from. Each shows one kind:

| Plugin | Kind | What it does |
| --- | --- | --- |
| [`flag`](flag/) | A program and components | `<Flag>` draws an image waving like a flag. The plugin's program (Rust) returns textured meshes for each time, and Celesta draws them. |
| [`beep-voice`](beep-voice/) | Components only | `prepareBeepNarration()` voices dialogue lines with game-style beeps, in place of a speech engine such as VOICEVOX, with lip sync. |

The manifest, the protocol, and how Celesta draws meshes are described in
[`docs/handoff/2026-11-01-plugin-system.md`](../../docs/handoff/2026-11-01-plugin-system.md).

## Build and try them

```sh
pnpm install && pnpm run build:runtime   # once, at the repository root
node examples/plugins/build.mjs          # or: build.mjs flag
cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx flag.mp4 --plugin-dir examples/plugins/dist
cargo run -p celesta-exporter -- --react examples/plugins/beep-voice/demo.tsx beep.mp4 --plugin-dir examples/plugins/dist
```

`build.mjs` writes one plugin directory per sample under `dist/`:

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
desktop app's preview, start it with `CELESTA_PLUGIN_PATH` set to `dist/`.

Each sample's `plugin.json` gives its id, version, module source, and, for a
plugin with a program, the Cargo package that builds it.

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
`packages/cli`, as `build.mjs` does.
