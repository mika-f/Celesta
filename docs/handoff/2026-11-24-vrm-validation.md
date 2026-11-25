# VRM validation compatibility

[Issue #694](https://github.com/mika-f/Celesta/issues/694) covers two
loader refusals. An avatar or animation can mark a supported VRM
extension as required. Unknown avatar bone and expression preset names
are skipped, as in UniVRM, without relaxing validation of known fields
or required bones.

- `document::validated_gltf` removes only implemented VRM extensions from
  `extensionsRequired` before glTF validation. Both avatar and animation
  parsing use it. The extension data and `extensionsUsed` are left intact.
- `Document::parse` filters the two avatar maps before deserializing the
  VRM schema. `VrmModel` returns the warnings after the usage terms, with
  the usual cap per warning kind.

## Checks

The synthetic avatar and animation tests cover supported required VRM
extensions and the refusal of required Draco. The model test checks
that each unknown key is named in a warning and the avatar still opens.

```sh
node scripts/plugins/run-cargo.mts plugins test -p celesta-plugin-vrm --locked
node scripts/plugins/run-cargo.mts plugins clippy -p celesta-plugin-vrm --all-targets --locked -- -D warnings
cargo fmt --manifest-path plugins/Cargo.toml --all -- --check
```
