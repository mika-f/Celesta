# Native `local()` for `@font-face` stylesheets (2026-10-31)

Builds on the source selection in
[`2026-10-31-font-face-src-selection.md`](2026-10-31-font-face-src-selection.md).

## Problem

When the winning `src` of a stylesheet rule listed only `local()` fonts, the
web renderer passed it to `FontFace`, and the browser drew the installed
font. The native renderer had no `local()` lookup and skipped the rule. So
the desktop app and the exporter drew a different font from the browser
preview, and the guide and the Agent Skill documented the difference.

## What CSS specifies

CSS Fonts 4, §4.3 and §4.3.3.1 ("Local font fallback"):

- The user agent tries the `src` entries in order and uses the first one it
  can parse and activate. A `local()` face that is not found is ignored, and
  the next entry is tried.
- `local(<font-family-name>)`: a quoted string, or identifiers that are
  joined by a single space.
- For OpenType and TrueType, the name matches only the PostScript name
  (name ID 6) or the full font name (name ID 4) of an installed font. For
  the full name, the US English record is used (Windows language 0x409,
  Macintosh language 0), or the first localization when there is none.
  Matching other localized full names, or family plus style names, is
  non-conformant. Platform substitutions must not be used.
- The quoted text does not say whether case matters. Chromium and Firefox
  both compare case-insensitively, and so does Celesta.

## Decision: a `url()` wins over an earlier `local()`

A browser tries `local(A), url(b)` in order, so it draws an installed A and
downloads b only when A is missing. Celesta still takes the `url()`:

- **Reproducible exports.** With a file to download, the export is the same
  on every computer. Trying `local()` first would make it depend on which
  fonts, and which versions of them, the exporting computer has, even
  though the stylesheet gives a file.
- **Parity.** The web renderer already loaded the first `url()` after a
  `local()`. It needs the file's bytes for `featureFamily`, which copies a
  face with `featureSettings`; a `FontFace` made from `local()` has no
  bytes to copy. The shared fixture's `AfterLocal` rule pins this for both
  renderers.

So installed fonts are used only where the stylesheet has nothing to
download. A `local()`-only rule then tries each name in order and draws the
first installed match. When none is installed, the face draws nothing, as
in a browser. Its text falls back to another font, with the usual font
fallback warning. The stylesheet does not fail.

## Implementation

- `crates/remote/src/stylesheet.rs`: `StylesheetFontFace` now carries
  `source: FontFaceSource`, which is `File(AssetLocation)` or
  `Local(Vec<String>)`. `chosen_source` takes the winning `src`'s first
  url, or all its `local()` names in order. A quoted name is unquoted. An
  unquoted name's words are joined by one space. `local()` and `local('')`
  are invalid entries.
- `crates/renderer/src/fonts.rs`:
  - `installed_faces` is the set of database ids when the rasterizer is
    made, taken after `load_directwrite_system_fonts` on Windows, which
    `initial_db` does not include. Fonts the project loads later never
    match `local()`, as a browser's `local()` never matches a web font.
    `fork` copies the set.
  - `installed_face(name)` looks names up in an index built on first use.
    The index maps the lowercased PostScript name (`FaceInfo` stores it)
    and the full name to the face id. The full name is read with
    `skrifa`, which `cosmic-text` re-exports, through
    `localized_strings(FULL_NAME).english_or_first()`. `fontdb` stores
    only family names (IDs 16 and 1), which must not match. When two faces
    have the same name, the first one in database order wins.
  - `alias_installed_face` adds a copy of the installed face's `FaceInfo`.
    The copy keeps the same source and index. Its families are only the CSS
    family and a private `celesta-stylesheet-face-installed-…` family, so
    the installed family still has one face. As for downloaded faces, the
    private family goes to `stylesheet_faces` and to the font list's
    families, so Unicode-subset selection and `fontFallback: 'loaded'`
    behave the same. The same face under the same family is added once.
  - `fonts_changed` is the cache invalidation that `load_font_data` did
    inline. Aliasing an installed face uses it too.
  - The `InvalidFont` reason for a stylesheet with no usable rule now
    mentions installed fonts.
- `packages/web/src/fonts.ts`: `supportedSources` returns `{ local: name }`
  with the same normalization as native. A `local()`-only rule builds its
  `FontFace` source from those names (`local("A"), local("B")`) instead of
  passing `src` as written, so both renderers try the same names in the
  same order.

## Remaining differences

- `fontFeatures` on a `local()` face works natively. The browser preview
  throws, because the browser cannot copy an installed font. This is the
  same rule as for any installed font, and the guide says so.
- Which fonts are installed is up to each computer. That is the reason for
  the decision above.
- Neither parser decodes CSS escapes in `local()` names, or rejects
  unquoted CSS-wide or generic keywords such as `local(serif)`. The parsers
  behave the same, so they still pick the same face.
- `english_or_first` prefers en-US, then any `en`, then a record with no
  language, then the first record. The spec asks for US English, then the
  first record. The two differ only for fonts that have no en-US full name.

## Checks

```sh
cargo test -p celesta-remote -p celesta-renderer
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
pnpm run build:runtime
pnpm --dir packages/web test
```

New tests:

- `a_src_with_no_url_names_its_installed_fonts_in_order` (remote):
  parsing, name normalization, `url()` preference, and invalid empty names.
- The shared fixture `packages/web/test/fixtures/font-face-src/` now lists
  `local("…")` lines for `OnlyLocal` and `LocalNames`, and both suites
  check them.
- `stylesheet_local_faces_draw_installed_fonts_by_full_or_postscript_name`
  (renderer): matching by full name and by PostScript name, ignoring case.
  A family name, a French full name, and a missing font do not match. A
  `url()` wins over an installed `local()`. The installed family keeps
  one face.
- `stylesheet_local_faces_do_not_match_fonts_the_project_loaded`.
- `stylesheet_local_faces_draw_like_the_installed_font`: text in the alias
  family rasterizes pixel for pixel like the installed family.

The tests add fonts as installed with the test-only
`TextRasterizer::install_font`, so they do not depend on the CI computer's
fonts.
