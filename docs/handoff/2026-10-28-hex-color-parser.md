# One hex color parser (2026-10-28)

Issue #570 (parent #569); also fixes #581. Colors stay `#RRGGBB` /
`#RRGGBBAA` strings in every persisted and scene model (`Paint::Solid`,
`LayerShadow`, `LayerGlow`, `LayerFilter::Tint`, `TextColorRun`, project
effects), so JSON, the ts-rs bindings, and when and how validation reports a
bad color are unchanged. What changed is that each language now reads that
text through one parser instead of a copy per call site.

## Rust

- `celesta_composition::Color` (`crates/composition/src/color.rs`):
  `red`/`green`/`blue`/`alpha` bytes, `rgba()`, `TRANSPARENT`, `WHITE`,
  `from_hex(&str) -> Option<Color>`, `channels()`, `lerp(to, t)` (per
  channel, rounded, saturating), and `Display` as `#RRGGBBAA` in upper case.
  `from_hex` reads bytes, so text that splits a UTF-8 character at a digit
  boundary (`#aéaaa`) is rejected instead of panicking (#581).
- `celesta_renderer::Color` is now a re-export of it; the renderer's
  `parse_color(&str) -> Result<Color, RenderError>` maps a bad color to
  `RenderError::InvalidColor` and replaces `Color::from_hex` at every CPU and
  GPU renderer call site.
- Project validation (`valid_effect_color` and `Validator::color`), the
  evaluator (`parse_effect_color`, removed), `celesta-mcp`'s loop summary, and
  the editor's `parse_color_input` / `parse_color` all call
  `Color::from_hex`. The MCP summary now compares any two hex colors by
  channel, so `#FF0000` and `#FF0000FF` no longer count as a difference.
- `LayerFilter::is_identity` treats a tint as identity when its color parses
  with alpha 0, not when the text is 9 bytes ending in `00`. An unparsable
  tint is not identity, so the renderer still reports it.
- `celesta_composition::keyframe_segment(animation, time)` returns the two
  values `time` falls between and the eased progress, for any value type;
  `evaluate_f64` is built on it. `evaluate_effect_color` uses it to parse
  only those two keyframes and `Color::lerp` them, instead of parsing every
  keyframe and building and evaluating four numeric animations per frame.
  It still formats the result as text, because `LayerFilter::Tint`,
  `LayerShadow`, and `LayerGlow` keep `String` colors.

## TypeScript

- `@celesta/math` exports `isHexColor(value)` and `parseHexColor(color)`
  (`packages/math/src/color.ts`, `[r, g, b, a]` 0–255 or `undefined`), with
  the same rules as `Color::from_hex`.
- `@celesta/react` (animation, rich text, theme, render, shader filters,
  instances), `@celesta/shader`, `@celesta/text`, `@celesta/components`,
  `@celesta/cli`'s property inputs, and `@celesta/web`'s filters and
  gradients use them; no other hex regex remains. `@celesta/react`,
  `@celesta/shader`, and `@celesta/text` now depend on `@celesta/math`
  (`package.json` and tsconfig references). The chroma key still takes only
  `#RRGGBB`.
- `<Instances fill>` is now checked as a hex color, like `<Rect fill>`; it
  used to pass any string to the renderer (the composition path in #581).
- User docs: the Colors section of `math: shaping, waves & geometry`
  (en/ja) and `skills/celesta/references/math.md`.

## Not done

- Typing scene/model color fields as `Color` with string serde. It would
  move bad-color errors from render or validation time to parse time, change
  serialized text (`#ff0000` would come back `#FF0000FF`), and touch the
  generated bindings; left for a separate decision.
- The evaluator blends effect colors per straight channel, while
  `interpolateColor()` blends premultiplied; unchanged here.

## Checks

On macOS (Apple silicon), with `PKG_CONFIG_PATH` at Homebrew's FFmpeg 8:

- `cargo test --workspace`: all passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: the only error is
  `clippy::large_enum_variant` on `SourceLookup` in
  `crates/editor/src/layer_select.rs`, which this change does not touch
  (clippy 0.1.99); with that lint allowed, clean. The `codegen` clippy run
  for `celesta-composition` and `celesta-project` is clean.
- `cargo fmt --all -- --check`, `git diff --check`: clean.
- `pnpm run build:runtime`, then `pnpm test` in `math`, `react`, `shader`,
  `text`, `components`, `cli`, and `web`: all passed. `web`'s worker
  type-check, `vite build`, and declaration build succeed.
- New tests: `colors_parse_six_or_eight_hex_digits`,
  `colors_format_as_eight_upper_case_digits_and_interpolate_per_channel`,
  `a_tint_is_identity_only_when_its_color_is_transparent` (composition),
  `instances_report_a_fill_that_splits_a_character_instead_of_panicking`
  (renderer), the editor's color input test with `#aéaaa`,
  `packages/math/test/color.test.mts`, and `fill` cases in
  `packages/react/test/instances.test.mts`.
- Rendering is unchanged by design (same parsed bytes, same interpolation
  formula), so no before/after screenshots were taken.
