# VRM eye tracks and lookAt

[#696](https://github.com/mika-f/Celesta/issues/696): reject tracks that
would be overwritten by the avatar's bone-based lookAt. The lookAt stage
sets the eyes' rotation even without a `lookAt` prop. Expression-based
lookAt leaves rotations alone, including on avatars that fall back to
expressions because they do not have both eye bones.

Track validation uses the loaded lookAt's effective mode, so both the
frame and the spring request report `tracks.leftEye: the avatar's lookAt
turns this bone` (or the corresponding right-eye error). Spring steps
still reuse validated tracks. The JS track schema is unchanged because it
does not know the loaded avatar's mode.

The Rust regressions cover refusal of either eye on a bone-lookAt frame
without a gaze prop, both eye rotations with expression-based lookAt,
and refusal when a spring request validates the tracks directly.

## Validation

- `node scripts/plugins/run-cargo.mts plugins test -p celesta-plugin-vrm --locked`
  passed with lavapipe; official avatar samples were not configured.
- The same runner passed `clippy` with `--all-targets --locked -- -D warnings`
  and `build` with `--locked`.
- `cargo fmt --manifest-path plugins/Cargo.toml --all -- --check` passed.
- `pnpm --dir plugins/vrm/js test` passed all 17 tests after
  `pnpm run build:runtime`.
