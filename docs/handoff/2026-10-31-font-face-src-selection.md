# `@font-face` source selection (2026-10-31)

## Problem

A `<Font src>` can be a web-font stylesheet. Each of its `@font-face` rules
gives a `src` list, and Celesta loads one file per rule. The two renderers
chose that file differently, and neither chose the way browsers do:

- Native (`font_face_rules` in `crates/remote/src/stylesheet.rs`) took the
  first `src` declaration and the first `url()` in it. It ignored `format()`
  and `tech()`.
- Web (`loadFonts` in `packages/web/src/fonts.ts`) also took the first `src`
  declaration. Within it, it skipped `url()`s whose `format()` was not WOFF2,
  WOFF, TrueType, or OpenType, and ignored `tech()`.

Font Squirrel's "bulletproof" CSS breaks both:

```css
src: url('webfont.eot');
src: url('webfont.eot?#iefix') format('embedded-opentype'),
     url('webfont.woff2') format('woff2'), …, url('webfont.svg#Name') format('svg');
```

Both renderers picked `webfont.eot`, which neither can read. Native failed
the whole stylesheet with `InvalidFont`. Web warned and dropped the face.

## What CSS specifies

CSS Fonts 4, section 4.3 (the `src` descriptor):

- While parsing a `src` value, entries that fail to parse, or whose
  `format()` or any `tech()` the user agent does not support, are dropped.
  If none remain, the value is a parse error.
- So among several `src` declarations in one rule, the last one with a
  supported entry wins, as with any declaration. An invalid later one
  leaves the earlier one in effect.
- Formats: `collection`, `embedded-opentype`, `opentype`, `svg`,
  `truetype`, `woff`, `woff2`. The legacy strings `"woff2-variations"`,
  `"woff-variations"`, `"truetype-variations"`, and `"opentype-variations"`
  mean the format plus `tech(variations)`.
- Techs: `features-opentype`, `features-aat`, `features-graphite`,
  `color-COLRv0`, `color-COLRv1`, `color-SVG`, `color-sbix`, `color-CBDT`,
  `variations`, `palettes`, `incremental`.
- An entry without `format()` is downloaded. If it does not load, the user
  agent tries the next entry.
- A collection must load only the face named by the `#PostScriptName`
  fragment.

## What the renderers support

The supported lists follow from what the native renderer can draw. The web
renderer uses the same lists, so both load the same file.

- **Files:** `TextRasterizer::load_font_data` reads TrueType and OpenType
  through `fontdb`, and WOFF and WOFF2 through `wuff`. It cannot read EOT or
  SVG fonts. It reads collections, but it adds every face, ignoring the
  fragment, so collections are not supported.
- **Shaping:** `cosmic-text` 0.18 shapes with `harfrust` 0.5, which applies
  OpenType GSUB/GPOS and AAT `morx`/`kerx`. There is no Graphite.
- **Variations:** `cosmic-text` sets the `wght` axis from the requested
  weight.
- **Color:** `cosmic-text` rasterizes with `swash` 0.2, using color outlines
  (COLR v0 layers with CPAL), then color bitmaps (`sbix`, `CBDT`), then
  plain outlines. There is no COLRv1 and no OpenType-SVG. Celesta has no
  `font-palette`, so `palettes` is unsupported.
- **Loading:** files are downloaded whole, so `incremental` is unsupported.

Supported formats: `woff2`, `woff`, `truetype`, `opentype`, and the four
`*-variations` strings. Supported techs: `features-opentype`,
`features-aat`, `variations`, `color-COLRv0`, `color-sbix`, `color-CBDT`.
Keywords and strings both match, ignoring ASCII case.

## Implementation

- Native: `supported_sources` splits a `src` value on commas outside quotes
  and parentheses, and parses each entry. A `url()` entry may be followed by
  `format()` and then `tech()`. `local()` is an entry of its own. Each `src`
  declaration with a supported entry replaces the previous choice. The face
  uses that declaration's first supported `url()`. A winning `src` made only
  of `local()` entries gives no file, so the rule is skipped, as before.
  `declarations` now shares the splitter.
- Web: the same parser in TypeScript (`supportedSources`), applied to every
  `src` declaration in the block. When the winning `src` has only `local()`
  entries, its text still goes to `FontFace`, as before. The format and tech
  lists sit next to a note naming their Rust counterparts.

The user guide's "Text and fonts" page (`en` and `ja`) and the Agent Skill's
`react-core.md` now say which file a stylesheet rule loads.

## Not changed

- **No fallback after a failed load.** A browser moves to the next entry
  when a file fails to download or decode. Celesta loads only the chosen
  entry. An entry without `format()` that turns out to be unreadable still
  fails, as before.
- **`data:` URLs.** Native skips a chosen `data:` source, and web loads it.
  That difference predates this change.
- **Per-face failures.** Native fails the whole stylesheet when one face does
  not load. Web warns and keeps the other faces, unless the stylesheet is
  from Google Fonts. That difference predates this change.

## Checks

- `cargo test -p celesta-remote`: 22 passed. The new tests
  `the_last_valid_src_wins` and
  `picks_the_first_source_with_a_supported_format_and_tech` failed before
  the change.
- `cargo clippy -p celesta-remote --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` passed.
- `pnpm run build:runtime`, which generates `packages/react/src/generated/`,
  then `pnpm --dir packages/web test`: typecheck passed, and 150 tests
  passed. The new test,
  `a stylesheet face loads the file the native renderer picks from its last
  valid src`, uses the same rules as the two Rust tests. It failed before the
  change: web fetched `brand.eot`, `c.woff2` (COLRv1), `m.ttf` (Graphite),
  and `remote.woff2`.
