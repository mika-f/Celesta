# VRM plugin

A Celesta content plugin that draws VRM 1.0 avatars (`.vrm`) as 3D scenes.
Like every plugin, it is a program of its own: Celesta starts it and talks to
it over standard input and output (`celesta_plugin::serve`).

It is a work in progress: the program opens no model yet.

## Opening files

- A `.vrm` is a binary glTF file with the `VRMC_vrm` 1.0 extension, its
  buffers inside the file.
- A VRM 0.x file (the `VRM` extension) is refused with a message saying so:
  convert it to VRM 1.0 with UniVRM or the VRM Add-on for Blender.
- A file without `VRMC_vrm`, with a `specVersion` other than 1.0, or whose
  humanoid lacks a bone VRM 1.0 requires is refused, naming what is wrong.

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
