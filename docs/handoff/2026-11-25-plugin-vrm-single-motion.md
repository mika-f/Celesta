# Single-clip VRMA playback (2026-11-25)

`<Vrm src="./avatar.vrm" motion="./wave.vrma" />` is shorthand for
`motions={[{ src: './wave.vrma' }]}`. The clip starts at local frame 0,
plays once, and holds its final pose. Use `motions` instead for loops,
delays, and transitions. Supplying both props throws, even if `motions`
is empty.

The TypeScript wrapper normalizes the props before assigning clip asset
names or building frame data. Neither the plugin protocol nor the Rust
playback code changes. The Seed-san demo uses the shorthand with full
framing so the arm movement fits in view. It keeps the existing sample,
nod, and mouth shapes, and ends one second after the clip does.

## Checks

- `pnpm run build:runtime` and `pnpm --filter celesta-plugin-vrm-js test`:
  equivalent assets and motion data, defaults, conflicting props, and
  identical preview and export evaluation on a nested sequence's clock.
- `node scripts/plugins/build.mts plugins/vrm`: built the plugin and its
  component bundle and declarations.
- The VRM plugin's `seed_san_draws` and `looping_and_holding` tests passed.
- Exported Seed-san frames 15, 75, and 105 at 30 fps with both spellings.
  The paired PNGs were byte-identical, including after the clip ended.

Fade easing and VRMA gaze playback are separate work.
