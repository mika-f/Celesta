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

Supported formats are `woff2`, `woff`, `truetype`, and `opentype`, written as
keywords or as strings. The four `*-variations` forms are accepted only as
strings, because CSS Fonts 4 defines them only as strings. In Chrome 152,
`src: url(a.woff2) format(woff2-variations), url(b.ttf)` serializes through the
CSSOM as `url("b.ttf")`, so the unquoted entry is dropped. Supported techs are
`features-opentype`, `features-aat`, `variations`, `color-COLRv0`,
`color-sbix`, and `color-CBDT`. Matching ignores ASCII case.

## Implementation

- Native: `supported_sources` splits a `src` value on commas outside quotes
  and parentheses, and parses each entry. A `url()` entry may be followed by
  `format()` and then `tech()`. `local()` is an entry of its own. Each `src`
  declaration with a supported entry replaces the previous choice. The face
  uses that declaration's first supported `url()`. A winning `src` made only
  of `local()` entries gives no file, so the rule is skipped.
  `declarations` now shares the splitter.
- Web: the same parser in TypeScript (`supportedSources`). Web now also reads
  the block the way native does:
  - Descriptors are read once, ignoring case, with the later declaration
    winning. Before, web matched `@font-face` and `font-family`
    case-sensitively and took the first `font-family`.
  - When the winning `src` has only `local()` entries, web skips the rule
    with a warning, as native does. Native has no `local()` lookup.
    - Before, web passed the `src` to `FontFace`. Preview and export then
      differed whenever the font was installed.
    - The difference was new in this change for a `local()`-only `src` that
      followed one with a `url()`; before it, both renderers loaded that
      earlier `url()`.

  The format and tech lists sit next to a note naming their Rust
  counterparts.
- Messages: when no rule names a loadable file, native used to say "the
  stylesheet has no @font-face url()" and web "Invalid font CSS", which is
  misleading when every entry was simply unsupported. Native's
  `InvalidFont` reason and web's warning now say that no rule names a
  TrueType, OpenType, WOFF, or WOFF2 file, or that the rule lists only
  `local()` fonts. Native has no warning channel, so a single unsupported
  rule among loadable ones is skipped without a message, as CSS skips it.
- Comments: web now strips CSS comments before reading the stylesheet, as
  native always did (`strip_comments`). Before this, a declaration that
  started after a comment was invisible to web. CSS-Tricks' bulletproof
  snippet, which puts `/* IE9 Compat Modes */` after the first `src`,
  therefore still loaded the EOT on web while native loaded the WOFF2.
- `data:` sources: web fetches them, and `fetch` decodes `data:` URLs.
  Native used to skip a chosen `data:` source, which dropped the face. Now
  `resolve_reference` keeps it as an `AssetLocation::Url`, and
  `RemoteAssetCache::fetch` decodes it into the cache with the `data-url`
  crate. That crate implements the WHATWG data: URL processor and was
  already built through `resvg`/`usvg`. An invalid `data:` URL fails with
  `RemoteAssetError::InvalidData`. The message says whether the comma is
  missing or the base64 is invalid, and shortens the URL to 64 characters.
  `UnsupportedScheme` now reads "not an http, https, or data URL".
  - Side effect: an asset in project JSON with
    `{ "type": "url", "url": "data:…" }` (`AssetSource::Url`) also reaches
    `fetch`, and is now decoded instead of failing with `UnsupportedScheme`.
  - React components cannot produce such a location: `isRemoteUrl` and the
    bridge accept only `http(s)`.
  - Each decoded entry is a file named `asset`, with no extension.
- Cache writes: `RemoteAssetCache::store` now writes both downloads and
  decoded `data:` URLs. It creates the entry directory only when a file is
  stored, so an HTTP error no longer leaves an empty directory behind.
  `http_errors_leave_nothing_cached` checks that the whole cache root stays
  empty.
- Shared fixture: `packages/web/test/fixtures/font-face-src/stylesheet.css`
  and `expected.txt` list the rules and the `family<TAB>url` each one loads.
  The Rust test `picks_the_same_sources_as_the_web_renderer` (via
  `include_str!`) and the web test both read them.
  - One list of cases covers both parsers.
  - Every supported `format()` and `tech()` value decides the file for some
    rule, so dropping a value from either side's list fails a test.

The user guide's "Text and fonts" page (`en` and `ja`) and the Agent Skill's
`react-core.md` now say which file a stylesheet rule loads. They also say that
entries without `format()` are used, that `local()` is not, and that nothing
falls back when a file fails to load.

`docs/handoff/2026-10-29-remote-url-join.md` (#605) is updated where this
change supersedes it: `data:` references, and `font_source` in place of
`first_url`.

## Not changed

- **No fallback after a failed load.** A browser moves to the next entry
  when a file fails to download or decode. Celesta loads only the chosen
  entry. An entry without `format()` that turns out to be unreadable still
  fails, as before.
- **CSS escapes.** Neither parser handles escapes such as `url(font\).woff2)`.
  Before this change both requested a wrong URL. Now both drop the entry.
  Font URLs with escapes are rare.
- **Per-face failures.** Native fails the whole stylesheet when one face does
  not load. Web warns and keeps the other faces, unless the stylesheet is
  from Google Fonts. That difference predates this change.

## Checks

- `cargo test -p celesta-remote`: 23 passed. The fixture test and
  `decodes_data_urls_into_the_cache` are new.
  - Before the source-selection change, native picked the first `src`.
  - Before the review fixes, it still picked the unquoted
    `*-variations` entry and dropped the `data:` face.
- `cargo test -p celesta-renderer`: `stylesheets_with_no_loadable_source_say_so`
  is new and failed against the old message.
- `cargo clippy -p celesta-remote -p celesta-renderer --all-targets -- -D warnings`
  and `cargo fmt --all -- --check` passed.
- `pnpm --dir packages/web test`: typecheck passed and 150 tests passed. Run
  `pnpm run build:runtime` first, because it generates
  `packages/react/src/generated/`.
  - The new fixture test, `a stylesheet face loads the file the native
    renderer picks from its last valid src`, failed before each round of
    review fixes.
    - First round: web fetched `webfont.eot`, skipped the three comment
      rules, and took the unquoted `v.woff2`.
    - Second round: web dropped the `@FONT-FACE` rule.
