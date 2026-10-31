# Native `local()` for `@font-face` stylesheets (2026-10-31)

Builds on the source selection in
[`2026-10-31-font-face-src-selection.md`](2026-10-31-font-face-src-selection.md).

## Problem

When the winning `src` of a stylesheet rule listed only `local()` fonts, the
web renderer passed it to `FontFace`, and the browser drew the installed
font. The native renderer had no `local()` lookup and skipped the rule. So
the desktop app and the exporter drew a different font from the browser
preview, and the guide and the Agent Skill documented the difference. A
stylesheet whose rules were all `local()`-only failed natively with
`InvalidFont` ("no @font-face rule in the stylesheet names a file Celesta
can load"), so its export stopped while the preview drew it.

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
- A face whose full name equals its family name, typically a regular face,
  matches by that name: the spec's own first example is `local(Gentium)`
  for "regular face of Gentium". Arial's regular face has the full name
  "Arial", so `local(Arial)` matches; `local("Hiragino Sans")` matches
  nothing, because every Hiragino Sans face's full name carries a weight.
- The spec does not say whether case matters. Celesta ignores it, as
  browsers do: Chromium compares with `kCFCompareCaseInsensitive` on macOS
  (`font_matcher_mac.mm`) and case-folds its unique-name table on Linux and
  Android; Firefox lowercases the key (`gfxPlatformFontList.cpp`).
- Browsers may hide some installed fonts from `local()`. Safari matches
  only fonts that come with the operating system, not user-installed ones
  (https://webkit.org/tracking-prevention/, WebKit bug 230156). Firefox
  does the same under Resist Fingerprinting (`layout.css.font-visibility`).
  Chromium has no such limit today. The native renderer matches every
  installed font.

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
  renderers. (Superseded in part by
  `2026-10-31-web-local-font-features.md`: `featureFamily` now copies a
  `local()` face from its names, so the bytes are no longer needed. The
  web renderer still loads the first `url()`, and reproducible exports
  still decide.)

So installed fonts are used only where the stylesheet has nothing to
download. A `local()`-only rule then tries each name in order and draws the
first installed match. When none is installed, the face draws nothing, as
in a browser, and the stylesheet does not fail. Its text falls back to
another font. Natively, `font_fallback` then reports the family, which the
app lists with the preview's warnings and the exporter prints. The web
renderer has no font fallback warning: a `FontFace` that matches nothing
fails to load, and `loadFonts` logs "Could not load a font face from …" with
`console.warn`, which reaches only the browser's developer tools.

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
    only family names (IDs 16 and 1), which are not looked up. When two faces
    have the same name, the first one in database order wins. The first
    `local()` lookup reads the name table of every installed face.
    `with_face_data` memory-maps each file, because cosmic-text's default
    `std` feature turns on `fontdb/memmap`. A project with no `local()`-only
    rule never builds the index.
  - `alias_installed_face` adds a copy of the installed face's `FaceInfo`.
    The copy keeps the same source and index. Its families are only the CSS
    family and a private `celesta-stylesheet-face-installed-…` family, so
    the installed family still has one face. As for downloaded faces, the
    private family goes to `stylesheet_faces` and to the font list's
    families, so Unicode-subset selection and `fontFallback: 'loaded'`
    behave the same. The same face under the same family is added once.
  - `fonts_changed` is the cache invalidation that `load_font_data` did
    inline. Aliasing an installed face uses it too.
  - A `local()` rule with no `font-family` adds nothing: the installed
    font already has its own family. CSS requires the descriptor, and the
    web renderer warns about the rule.
  - The `InvalidFont` reason for a stylesheet with no usable rule now
    names `local()` font names among the entries Celesta can use. It fires
    only when no rule has such an entry; whether a `local()` font is
    installed is checked later, per face.
- `packages/web/src/fonts.ts`: `supportedSources` returns `{ local: name }`
  with the same normalization as native. A `local()`-only rule builds its
  `FontFace` source from those names (`local("A"), local("B")`) instead of
  passing `src` as written, so both renderers try the same names in the
  same order.

## Remaining differences

- `fontFeatures` on a `local()` face works natively. The browser preview
  throws, as for any installed font, because `featureFamily` copies a
  face's bytes. Chromium applies a `FontFace`'s `featureSettings`
  descriptor to `local()` sources too, so the web renderer could make the
  copy from the same `local()` names; that is left for a follow-up.
  (Closed in `2026-10-31-web-local-font-features.md`: the preview applies
  the features to `local()` faces too.)
- Which fonts are installed is up to each computer, and the preview's
  browser may offer only some of them (Safari: OS fonts only). That is the
  reason for the decision above.
- The native renderer ignores a rule's `font-weight`, `font-style`, and
  `unicode-range`. Copies keep the installed font's own weight and style,
  and a family with several faces draws each character with the first face,
  in rule order, that has its glyph. The web renderer passes the
  descriptors to `FontFace`. This was already true for `url()` faces, where
  Google Fonts' subset files cover roughly their declared ranges, so it did
  not show; `local()` names whole fonts, so it does:
  `unicode-range: U+30-39; src: local(Arial)` draws all text in Arial
  natively and only the digits in a browser.
- A stylesheet family named like an installed family (`font-family:
  "Hiragino Sans"; src: local(HiraginoSans-W8)`) does not hide the
  installed family's other faces natively: font matching still sees W0–W9,
  and inside a font stack `stack_faces` drops the W8 copy. CSS hides the
  installed family. A stylesheet family with no face loaded also still
  matches an installed family of that name natively, which CSS forbids.
  Both were already true for `url()` faces.
- The native renderer has no synthetic bold, so a family whose 700 rule
  names a font that is not installed draws 700 text with its 400 face
  without a warning. That is true of every family.
- Neither parser decodes CSS escapes in `local()` names, or rejects
  unquoted CSS-wide or generic keywords such as `local(serif)`. The parsers
  behave the same, so they still pick the same face.
- `english_or_first` returns the en-US record (Windows 0x409) first, then
  the Macintosh English one (`en`), then a record with no language, then
  the first record. The spec asks for US English (either of the first two),
  then the first record, so the two differ only for a font with no English
  full name and a record with no language that is not its first.

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
  one face, a rule with no `font-family` adds none, and text in a family
  whose fonts are not installed gets a font fallback report.
- `an_installed_face_is_added_once_per_stylesheet_family`: the same face
  named twice in one stylesheet and again in another is one copy.
- `stylesheet_local_faces_do_not_match_fonts_the_project_loaded`.
- `stylesheet_local_faces_draw_like_the_installed_font`: text in the alias
  family, the same text in a fork, and `fontFallback: 'loaded'` text all
  rasterize pixel for pixel like the installed family.
- The web fixture test also checks that `loadedFamilies` lists the
  `local()` faces' families.

The tests add fonts as installed with the test-only
`TextRasterizer::install_font`, so they do not depend on the CI computer's
fonts.

## Screenshots

The PR's before and after images are frame 0 of one React scene, exported
on macOS with `celesta-exporter --react film.tsx --frame 0`. The scene's
`<Font src="./local.css">` has three rules:

```css
@font-face { font-family: Display; src: local("Futura Bold"), local(Futura-Bold) }
@font-face { font-family: Gothic; src: local(Missing-Font), local(HiraginoSans-W8) }
@font-face { font-family: Headline; src: local(Futura-Bold), url(BebasNeue-Regular.ttf) }
```

Before, the exporter warns that `Display` and `Gothic` are not loaded and
draws them with the fallback font. After, they draw Futura Bold and Hiragino
Sans W8. `Headline` draws the downloaded Bebas Neue in both images, even
though Futura Bold is installed. The `url()` rule is there so that the old
build loads the stylesheet at all.
