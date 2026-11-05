# VRM plugin

A Celesta content plugin that draws VRM 1.0 avatars (`.vrm`) as 3D scenes.
Like every plugin, it is a program of its own: Celesta starts it and talks to
it over standard input and output (`celesta_plugin::serve`).

It draws the avatar with MToon and unlit materials (glTF PBR materials as
unlit base colour), expressions, eyes following a gaze target, a camera
framed from the humanoid, and its reference light, and reports the model's
usage terms as notices. VRM animation clips (`.vrma`) and keyframed bone
tracks move it, its node constraints follow, and its spring bones swing on
a fixed 60 Hz clock, so any frame is the same however it is reached. How
it works, what it leaves out, and where it differs from UniVRM:
`docs/handoff/2026-11-03-plugin-vrm.md` and
`docs/handoff/2026-11-03-plugin-vrm-motion.md`.

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

The plugin is a member of the first-party plugins' Cargo workspace,
`plugins/Cargo.toml`, with one `Cargo.lock` and one `target/`. The
repository's root workspace excludes `plugins`: `cargo build --workspace`
at the root does not build it. It depends on Celesta's `crates/plugin` and
`crates/composition` (for keyframed tracks) by path, and on nothing else of
Celesta's.

```sh
cargo build --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --release
cargo test --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm
cargo clippy --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --all-targets -- -D warnings
cargo fmt --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --check
```

The tests draw through Celesta's GPU renderer. Without a GPU adapter the
GPU tests return early; `CELESTA_REQUIRE_GPU=1` makes them fail instead.

## The plugin directory and `<Vrm>`

The repository's plugin build script, `scripts/plugins/build.mjs`, builds
what Celesta loads: the manifest, the program, and the module entries
import as `@celesta-plugin/vrm` (`js/src/index.ts`, the `<Vrm>` component),
with its declarations. The manifest's version is `plugin.json`'s, which
must be the Cargo package's: bump both together, or the build refuses
the plugin.

```sh
pnpm install && pnpm run build:runtime          # once, at the repository root
node scripts/plugins/build.mjs plugins/vrm      # writes plugins/dist/vrm
pnpm --filter celesta-plugin-vrm-js test        # the component's tests
```

The script also takes `--out <dir>`, `--target <triple>`, and `--skip-js`
(see its header). Add `plugins/dist` to the app's plugin folders, or pass it
as the exporter's `--plugin-dir`.

## VRM animation clips

`<Vrm>`'s `motions` play `.vrma` files (`VRMC_vrm_animation` 1.0). None is
in the repository, and none may be committed: keep them in
`plugins/vrm/assets/` (ignored by Git) or next to your composition. Two
sources:

- `test.vrma`, from [pixiv/three-vrm](https://github.com/pixiv/three-vrm)'s
  `packages/three-vrm-animation/examples/models` (MIT, © pixiv Inc.): a
  short clip, the one the tests and the demo use.
- VRoid Project's VRMA motion pack, free on BOOTH (pixiv): seven
  motion-capture clips. Its terms allow free use, commercial use included,
  but not redistributing the files, and ask for the credit
  「キャラクターアニメーション: ピクシブ株式会社 VRoidプロジェクト」 where a work
  using them is published.

## The demo

[`demo.tsx`](demo.tsx) shows Seed-san from the bust up, playing a VRM
animation on a loop, nodding once, and mouthing the vowels. It needs two
files in `plugins/vrm/assets/`, which Git ignores:

- `Seed-san.vrm` (VRM Public License 1.0), from
  [vrm-c/vrm-specification](https://github.com/vrm-c/vrm-specification)'s
  `samples/Seed-san/vrm`.
- `test.vrma`, the VRM animation the plugin's tests use, from
  [pixiv/three-vrm](https://github.com/pixiv/three-vrm)'s
  `packages/three-vrm-animation/examples/models`. Any other `.vrma` works
  too; change the demo's `motions` to its name.

Then build the plugin and the codec plugin, which writes the MP4, into
`plugins/dist`, and export with it (the codec needs the FFmpeg development
libraries, as in Celesta's source-build guide):

```sh
node scripts/plugins/build.mjs plugins/vrm plugins/ffmpeg
cargo run -p celesta-exporter -- --react plugins/vrm/demo.tsx seed-san.mp4 --plugin-dir plugins/dist
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
