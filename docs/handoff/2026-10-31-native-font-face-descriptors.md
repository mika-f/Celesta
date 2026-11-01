# `@font-face` descriptors and family hiding in the native renderer (2026-10-31)

Builds on [`2026-10-31-native-font-face-local.md`](2026-10-31-native-font-face-local.md)
and resolves two of its "Remaining differences".

## Problem

The web renderer passes each stylesheet rule's `font-weight`, `font-style`,
and `unicode-range` to `FontFace`, and the browser matches faces by them.
The native renderer read only `font-family` and `src`:

- A face kept its font's own weight and style, and a family with several
  faces drew each character with the first face, in rule order, that had
  its glyph. `unicode-range: U+30-39; src: local(Futura-Bold)` drew all
  text in Futura natively and only the digits in a browser.
- Copies of stylesheet faces carried the CSS family name in fontdb, so a
  stylesheet family named like an installed family did not hide it:
  `font-family: "Hiragino Sans"; src: local(HiraginoSans-W8)` drew 400
  text with the installed W3. A family whose `local()` fonts were missing
  still matched the installed family of that name. A `url()` face also
  kept the family stored in its file, so text could reach it by that name.

## What CSS specifies

CSS Fonts 4, §4.4–§4.5 and §5.2:

- `font-weight`, `font-style`, and `font-stretch` accept a range. One
  value is a range with equal ends, and a decreasing range is swapped.
  `bolder` and `lighter` are not allowed. A value out of range is a parse
  error, and an invalid declaration is dropped.
- `unicode-range` is a comma list of `U+` code points, intervals, and `?`
  wildcards up to U+10FFFF. A face is never used for a code point outside
  it. The effective character map is the range intersected with the
  font's own map.
- Font matching narrows a family's faces by stretch, then style, then
  weight. For `font-style: normal`, oblique angles from 0deg up are
  checked first (normal is 0deg), then italic, then negative angles. A
  weight inside a face's range matches exactly; otherwise the nearest
  weight is chosen in the direction the specification gives (400–500:
  up to 500, then down, then up; below 400: down, then up; above 500: up,
  then down). The weight is not rounded to a multiple of 100.
- Faces left with the same descriptors form a composite face, tried for
  each character "in reverse order of @font-face rule definition".
- "If no matching face exists or the matched face does not contain a
  glyph for the character to be rendered, the next family name is
  selected … Glyphs from other faces in the family are not considered."
- A family defined by `@font-face` rules hides a platform family of that
  name.

Chromium was checked against the scene in "Screenshots": a plain HTML page
with the same rules draws the four lines as the new native export does.

## Decisions

- **Private families only.** Every stylesheet face, `local()` or `url()`,
  is added to fontdb as copies whose only family is a private
  `celesta-stylesheet-face-N`. The CSS family name never reaches fontdb
  or cosmic-text. Celesta resolves it itself and shapes text in the
  chosen copy's private family. So the installed family stays intact for
  cosmic-text's script fallback (its macOS Japanese fallback is
  "Hiragino Sans"), while text that asks for the stylesheet family gets
  only the stylesheet's faces.
- **One copy per drawn weight.** cosmic-text 0.18.2 picks a family's face
  only when its stored weight equals the requested one (`font_weight_diff
  == 0` in `default_font_match_key`), and it applies a `wght` variation
  equal to that weight. cosmic-text 0.19 adds `variable_weight_match`, so
  check whether the copies are still needed when upgrading. Each face gets a copy at 400 clamped into its range
  when it loads. `prepare_stylesheet_weights`, called at the top of
  `shaped_buffer`, adds copies for the other weights that text asks for
  inside a range before anything is shaped. A variable font with
  `font-weight: 100 900` therefore draws at the exact weight requested,
  as in a browser. Adding a copy calls `fonts_changed`, which clears the
  shaping caches and advances `fonts_revision`. `gpu-renderer`'s
  `prepare.rs` then drops every cached text texture, so the first use of
  each new weight (an animated weight, say) re-rasterizes all text once.
- **Only stylesheets in the font list.** Fonts stay loaded across
  reloads, and `load_fonts` loads each path once. Each face records the
  stylesheets that declare it, and `load_fonts` marks the stylesheets in
  the list it is given as active (`activate_stylesheets`). Only those
  stylesheets' families hide installed fonts, and only their faces are
  matched, so removing a `<Font>` stylesheet shows the installed family
  again without a restart. A font file, by contrast, still stays
  drawable after it leaves the list, as before.
- **Upright text.** Celesta text has no `fontStyle`, so it always asks
  for `normal`, and an italic face draws only when the family has nothing
  upright.
- **`auto`** reads as `normal` (400, or upright) for matching. CSS also
  leaves a variable font's axis unclamped for `auto`; natively it is drawn
  at 400.
- **Rules without `font-family`** are dropped by the parser, as CSS drops
  them. The web renderer already warned and skipped them, except in a
  Google Fonts stylesheet, where any failed rule rejects the whole
  stylesheet. A stylesheet
  whose rules all lack one fails with `InvalidFont`, whose reason now says
  that a rule needs a `font-family` and a usable `src`.
- **Rule order in the web renderer.** `loadFonts` added each face to the
  `FontFaceSet` when it finished loading. Chromium tries faces added with
  the FontFace API in the order they were added, the last one first, so
  which of two overlapping faces drew a character depended on the
  network. Faces still load in parallel, but are now added in rule order.
  The cost: a stylesheet's faces now appear together once the slowest
  has loaded, rather than one by one.
- **A span at another weight.** A `<Span>` that sets only `fontWeight`,
  in text whose family is a stylesheet family, is given the copy for its
  own weight. Without that it would take the text's copy, and a weight
  range would draw it at the text's weight.
- **Stylesheets in list order.** When two stylesheets declare one family,
  their rules are ordered as the stylesheets are in the font list, then
  by rule, as a document orders its stylesheets' rules. Each face records
  its rule index per stylesheet, and `ActiveStylesheets` maps each listed
  stylesheet to its position. So a reordered list takes effect in a
  running app as in a fresh export.
- **Spaces and `unicode-range`.** A cluster is drawn with a face only when
  the face's range holds its whitespace too, so "1 2" in a digits-only
  family takes the space from another font, as in a browser.
- **Private names win.** A stylesheet family that happens to be named
  like a copy's private family (`celesta-stylesheet-face-0`) is not
  treated as a stylesheet family; otherwise looking up its faces would
  recurse.
- **Stylesheet files apart from font files.** A file a stylesheet's
  `url()` names is cached in `stylesheet_files`, not `loaded_fonts`, so
  the same file listed as a `<Font>` of its own still loads under the
  family stored in it. `loaded_font_count` counts both.

## Implementation

- `crates/remote/src/stylesheet.rs`: `StylesheetFontFace` has `family:
  String` and `descriptors: FontFaceDescriptors { weight: (u16, u16),
  style: FontFaceStyle, unicode_range: Option<Vec<RangeInclusive<u32>>> }`.
  `font_weight`, `font_style`, and `unicode_range` parse the values. Each
  returns `None` for an invalid value, so the earlier declaration stays.
  Weights round to integers. Oblique angles accept `deg`, `grad`, `rad`,
  and `turn`. Ranges are sorted and merged, and a list covering every code
  point becomes `None`. The derived `PartialEq` includes the descriptors,
  so two rules that differ only in them are two faces.
- `crates/renderer/src/stylesheet_fonts.rs` (new):
  - `StylesheetFace` keeps the face's `FaceInfo` as a template (the
    installed face, or the first face of the file a `url()` names), its
    descriptors, and `instances`: the weights it has copies at, with
    their private families.
  - `match_faces` is §5.2 for upright text: style rank, then weight, then
    the remaining faces in reverse rule order.
  - `family_faces(name, weight)` returns the faces each character tries,
    as `FamilyFace { name, font, unicode_range }`. For a family that is
    not from a stylesheet, it is the one face `matched_face` finds.
  - `drawing_family` returns the private family that text in a
    stylesheet family is shaped in, or `NO_FACE_FAMILY`
    (`celesta-no-face`), a name no face has, when the family has no face.
    cosmic-text then goes straight to its fallback fonts.
  - `face_in_family` decides whether a drawn face belongs to a requested
    family. `missing_characters` uses it, so a character an installed
    font of the same name draws is reported as missing from the
    stylesheet family.
- `crates/renderer/src/fonts.rs`: `stylesheet_faces` maps each CSS family
  to its `StylesheetFace`s, and `stylesheet_privates` maps each private
  family back to its CSS family. `stylesheet_files` caches the template
  per `url()` file, so one file under several rules is read once.
  `stylesheet_families` records every family a stylesheet declares,
  including families whose fonts are missing, so those still hide the
  installed family. `active_stylesheets` and `active_stylesheet_families`
  hold the stylesheets in the current list. `matched_weight`
  answers a stylesheet family from its descriptors. `load_font_data` no
  longer takes an alias. The font list's fallback families for a
  stylesheet are now its CSS families, which have a face, rather than
  private names.
- `crates/renderer/src/font_stack.rs`: `primary_family` returns the
  family that text is shaped in (`Arc<str>`). That is a private family for
  a stylesheet family. `<Span>` runs are mapped the same way, including
  runs with no family of their own in text in a stylesheet family.
  `needs_selection` covers a stylesheet family with several faces or a
  `unicode-range`. `stack_faces` and `fallback_faces` use `family_faces`,
  so a stylesheet family keeps its copies inside a stack too.
  `cluster_family` checks `unicode-range` as well as the glyph. A cluster
  that no face draws gets `NO_FACE_FAMILY` whenever the primary face has
  a `unicode-range`, so that face cannot draw a character its range leaves
  out through cosmic-text.
- `crates/renderer/src/shaping.rs`: the emoji check maps a private family
  to its CSS family before it compares against `COLOR_EMOJI_FAMILIES`.
- `packages/web/src/fonts.ts`: the rule-order change above.

## Remaining differences

- An invalid descriptor value: natively, the declaration is dropped and
  an earlier one or the default applies, as in CSS. The web renderer
  passes the last value as written to `FontFace`, which rejects the face,
  so the preview skips the rule with a console warning (a Google Fonts
  stylesheet fails as a whole).
- A stylesheet family whose fonts all fail to load: natively it hides the
  installed family of its name and its text falls back, as CSS asks. The
  web renderer adds no `FontFace` for a face that fails to load, so the
  preview's browser draws the installed family.
- The preview's browser synthesizes bold for text at 600 or more in a
  face whose `font-weight` is lower; the native renderer has no synthetic
  bold, so `font-family: "Hiragino Sans"; src: local(HiraginoSans-W8)`
  draws plain W8 for 700 text natively and emboldened W8 in the preview.
- Natively, nothing reports a rule that is dropped (no `font-family`, no
  usable `src`, an invalid descriptor) or a `local()` face that is not
  installed while its family has other faces; the web renderer warns in
  the console. The only native signals are `InvalidFont` when no rule is
  usable, and the font fallback and missing-glyph warnings, whose wording
  ("is not installed or loaded") does not say that a stylesheet declared
  the family.
- Two stylesheets declaring one family with overlapping faces: the web
  renderer orders faces within a stylesheet, but adds each stylesheet's
  faces when that stylesheet finishes loading, so across stylesheets the
  preview's order still depends on the network. Natively it follows the
  font list.
- `font-stretch` is ignored by both renderers, since the web renderer does
  not pass it to `FontFace`. A family with condensed and normal faces can
  therefore draw a condensed face in both.
- cosmic-text's last-resort fallback tries every face in the database,
  exact-weight faces first, with no weight or family filter. When neither
  the script fallback nor the common fallback fonts have a character, it
  can therefore be drawn by a stylesheet copy at another weight or style,
  by a face outside its `unicode-range`, by an inactive stylesheet's copy,
  or by the installed family a stylesheet family hides.
- A font file loaded directly with `<Font src="….ttf">` whose own family
  equals a stylesheet's `font-family` is hidden natively by that
  stylesheet family. In a browser, both are `FontFace`s in one family.
- `color_emoji_family` looks fonts up by family name, so a color emoji
  font loaded only through a stylesheet is no longer the emoji fallback.
  Text whose family is that stylesheet family still draws its emoji.
- Unchanged from the `local()` note: no synthetic bold natively, and the
  preview's browser may expose only some installed fonts.

## Checks

```sh
cargo test -p celesta-remote -p celesta-renderer
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
pnpm run build:runtime
pnpm --dir packages/web test
```

`cargo clippy` still reports `SourceLookup`'s `large_enum_variant` in
`celesta-editor`, which PR #607 fixes and which this stack does not
include.

New tests:

- remote: `reads_font_weight_as_a_range`, `reads_font_style`,
  `reads_unicode_range` (wildcards, merging, and each invalid form
  dropping the declaration), and
  `rules_differing_only_in_descriptors_are_separate_faces`.
  `skips_comments_and_duplicates` now also drops a rule with no family.
- renderer, `stylesheet_fonts::tests`: §5.2 weight directions on the
  specification's example family, style ranks, reverse rule order, and
  faces of inactive stylesheets.
- renderer, `stylesheet_font_tests.rs`, which installs Bebas Neue and IBM
  Plex Mono with `install_font`:
  - `unicode_range_limits_what_a_face_draws`
  - `a_stylesheet_family_hides_the_installed_family_of_its_name`, alone
    and in a stack, and when its font is missing (also first in a stack)
  - `font_weight_and_font_style_replace_the_fonts_own`
  - `a_weight_matched_face_does_not_borrow_glyphs_from_other_weights`
  - `overlapping_faces_try_the_last_rule_first`
  - `a_weight_range_draws_at_the_weight_asked_for`, in a fork too
  - `one_file_under_several_rules_is_several_faces`
  - `a_stylesheet_left_out_of_the_font_list_stops_drawing_and_hiding`:
    reloading without one of two stylesheets reveals the installed family
    and drops that stylesheet's faces from a family that both declare.
  - `a_face_two_stylesheets_declare_draws_while_either_is_listed`
  - `a_weight_span_in_a_weight_range_draws_at_its_weight`
  - `a_stylesheet_file_listed_as_a_font_too_keeps_its_own_family`
  - `spans_features_and_loaded_fallback_reach_stylesheet_faces`
  - `loaded_fallback_takes_a_stylesheet_familys_face_at_its_weight`
  - `stylesheets_declaring_one_family_take_the_font_list_order`
  - `a_space_outside_the_unicode_range_is_not_the_faces`
  - `a_family_named_like_a_private_family_does_not_recurse`
- web: `a stylesheet adds its faces in rule order, whichever loads first`.

Existing tests changed with the behaviour: a `url()` face no longer
answers to the family stored in its file
(`loads_stylesheet_faces_under_their_css_family`), and the font list
names a stylesheet's CSS families (`a_stylesheet_lists_its_families`).

## Screenshots

The PR's before and after images are frame 0 of one React scene, exported
on macOS with `celesta-exporter --react film.tsx --frame 0` from the base
branch and from this one. Its `<Font src="./descriptors.css">` is:

```css
@font-face { font-family: Digits; src: local(Futura-Bold); unicode-range: U+30-39 }
@font-face { font-family: "Hiragino Sans"; src: local(HiraginoSans-W8) }
@font-face { font-family: Mix; src: local(Futura-Bold) }
@font-face { font-family: Mix; src: local(HiraginoSans-W8) }
@font-face { font-family: Leaning; font-style: italic; src: local(Futura-Bold) }
@font-face { font-family: Leaning; src: local(HiraginoSans-W8) }
```

All text is weight 400. Before: "ROOM 2048" is all Futura; "Hiragino Sans"
is the installed W3; "Mix" is Futura for Latin and digits (the first rule);
"Leaning" is Futura (weight 700 is nearer to 400 than W8's 800).
After: only "2048" is Futura; "Hiragino Sans" is W8; "Mix" is all W8 (the
last rule first); "Leaning" is W8 (upright before italic). The third image
is the same rules in a plain HTML page in Chromium, which matches the
after image.
