# VRM plugin

A Celesta content plugin that draws VRM 1.0 avatars (`.vrm`) as 3D scenes.
Like every plugin, it is a program of its own: Celesta starts it and talks to
it over standard input and output (`celesta_plugin::serve`).

It draws the avatar in its rest pose with MToon and unlit materials (glTF
PBR materials as unlit base colour), expressions, eyes following a gaze
target, a camera framed from the humanoid, and its reference light, and
reports the model's usage terms as notices. Motion (`.vrma`), constraints,
and spring bones come later. How it works, what it leaves out, and where
it differs from UniVRM: `docs/handoff/2026-11-03-plugin-vrm.md`.

## Opening files

- A `.vrm` is a binary glTF file with the `VRMC_vrm` 1.0 extension, its
  buffers inside the file.
- A VRM 0.x file (the `VRM` extension) is refused with a message saying so:
  convert it to VRM 1.0 with UniVRM or the VRM Add-on for Blender.
- A file without `VRMC_vrm`, or whose humanoid lacks a bone VRM 1.0
  requires, is refused, naming what is wrong. As in UniVRM, the
  extensions' `specVersion` is not checked: a file is read as 1.0
  whatever its version says.

## Building

The plugin is a Cargo workspace of its own, which the repository's root
workspace excludes: `cargo build --workspace` at the root does not build it.
It depends on Celesta's `crates/plugin` by path, and on nothing else of
Celesta's.

```sh
cargo build --manifest-path plugins/vrm/Cargo.toml --release
cargo test --manifest-path plugins/vrm/Cargo.toml
cargo clippy --manifest-path plugins/vrm/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path plugins/vrm/Cargo.toml --check
```

The tests draw through Celesta's GPU renderer, which links FFmpeg, so they
need what the repository's own tests need. Without a GPU adapter the GPU
tests return early; `CELESTA_REQUIRE_GPU=1` makes them fail instead.

## The plugin directory and `<Vrm>`

`build.mjs` builds what Celesta loads: the manifest, the program, and the
module entries import as `@celesta-plugin/vrm` (`js/src/index.ts`, the
`<Vrm>` component), with its declarations.

```sh
pnpm install && pnpm run build:runtime   # once, at the repository root
node plugins/vrm/build.mjs               # writes plugins/vrm/dist/vrm
pnpm --filter celesta-plugin-vrm-js test # the component's tests
```

`--out <dir>`, `--target <triple>`, and `--skip-js` work as for
`examples/plugins/build.mjs`. Add `plugins/vrm/dist` to the app's plugin
folders, or pass it as the exporter's `--plugin-dir`.

## The demo

[`demo.tsx`](demo.tsx) shows Seed-san from the bust up, mouthing the vowels.
Download `Seed-san.vrm` (VRM Public License 1.0) from
[vrm-c/vrm-specification](https://github.com/vrm-c/vrm-specification)'s
`samples/Seed-san/vrm` into `plugins/vrm/assets/`, which Git ignores, then:

```sh
node plugins/vrm/build.mjs
cargo run -p celesta-exporter -- --react plugins/vrm/demo.tsx seed-san.mp4 --plugin-dir plugins/vrm/dist
```

## Tests and sample models

Every test runs on a small VRM 1.0 avatar built in code
([`src/synthetic.rs`](src/synthetic.rs)), so no model is committed. Other
crates can build it too with the `synthetic` feature.

Tests that also check real avatars read them from the folder
`CELESTA_VRM_SAMPLES_DIR` names, which holds `Seed-san.vrm`,
`VRM1_Constraint_Twist_Sample.vrm`,
`VRMC_materials_mtoon_UV_Animation_Test.vrm`, and `test.vrma`. Without the
variable they print `skipped: CELESTA_VRM_SAMPLES_DIR is not set` and pass.
Never commit those files.

## Licence

MIT OR Apache-2.0, like the rest of the repository. Code ported from
[UniVRM](https://github.com/vrm-c/UniVRM) (MIT) carries UniVRM's notice.
