# VRM tangent generation load limits

[Issue #693](https://github.com/mika-f/Celesta/issues/693) bounds MikkTSpace
memory and work when opening an MToon primitive with a normal texture. If
the geometry itself fits but tangent generation does not, the primitive
uses default tangents and the model opens with a warning identifying it.

The estimate reserves 384 bytes per triangle for MikkTSpace 0.3.0's main
buffers, edges and hash entries, plus the 24 KiB fixed hash arrays and the
largest face group's arrays and member clones. It must fit in the model's
remaining geometry budget after reserving the primitive's output. Scratch
is freed before processing the next primitive, so only the output remains
charged. Compaction holds original vertex indices as `u32` and drops that
list before the tangent step.

Each primitive is capped at 131,072 triangles (48 MiB of base scratch)
and 256 corners at any one position. The latter bounds the quadratic face
group work even if MikkTSpace welds vertices that use different indices.

Checks from the repository root:

- `cargo test --manifest-path plugins/Cargo.toml --locked -p celesta-plugin-vrm`
- `cargo clippy --manifest-path plugins/Cargo.toml --locked -p celesta-plugin-vrm --all-targets -- -D warnings`
- `cargo fmt --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm -- --check`

The geometry tests cover a normal-mapped primitive that fits its output
budget but cannot reserve scratch, and a triangle fan on both sides of
the face-group limit. The model test checks that the warning reaches the
model's open notices under a tight geometry budget.
