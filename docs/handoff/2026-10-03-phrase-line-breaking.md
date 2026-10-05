# Phrase line breaking (2026-10-03)

`TextStyle.lineBreak` (`'normal'`, the default, or `'phrase'`) chooses where
`maxWidth` may wrap (issue #84). `'phrase'` keeps Japanese phrases (文節)
together, so `フレーム` or a trailing `の。` is not split across lines.

- `crates/budoux` (`celesta-budoux`, Apache-2.0 like upstream) ports
  google/budoux's parser (`budoux/parser.py`, by Unicode scalar value) and
  bundles upstream's `ja.json` model unchanged. It matched the Python
  parser on upstream's `tests/quality/ja.tsv` and on 5,000 random strings
  over the model's characters.
- `TextRasterizer::shaped_buffer` applies it, so the CPU renderer, the GPU
  renderer (which rasterizes text with `TextRasterizer`), `measureText()`
  through the React bridge, and `.celesta.json` text all wrap alike.
  `phrase_segments` splits each line into keep-together segments: BudouX
  phrases, cut again at breaks the text asks for (`is_explicit_break`, like
  CSS `word-break: keep-all`: after UAX #14 classes SP, ZW, BA, HY, and B2,
  such as a space, ZWSP, (soft) hyphen, ideographic space, or em dash, and
  before BB and B2), so
  `第一章　はじめに` is measured as `第一章　` and `はじめに`. A U+2060 WORD
  JOINER goes at each UAX #14 break opportunity (`unicode-linebreak`, as
  cosmic-text uses) inside a segment; Latin words keep their spacing breaks
  and get no joiners. With a width, every segment is joined before
  `Buffer::set_text` (lines split with cosmic-text's `LineIter`, as
  `set_text` does), so the text is shaped once, joined; the lines are laid
  out unwrapped and each segment is measured as the joined text is shaped (one word: kerning and fallback fonts can differ from the
  unjoined text by a few pixels); a segment wider than the line loses its
  joiners and wraps as `normal` text (UAX #14, so `、`/`。` still never
  start a line). Only lines whose joiners change are reshaped. Joiners get
  an attrs span with zero `letter_spacing`, which does not split cosmic-text's
  shaping run, so `letterSpacing` adds nothing for them.
  `Wrap::WordOrGlyph` was tried and rejected: its glyph fallback left `。`
  alone on a line. Without a width nothing is joined.
- The joiners never leave the rasterizer: `measure` drops them from
  `glyphs`, and the missing-glyph warning already skips format characters.
  A family without a U+2060 glyph draws it (zero width) from a fallback
  font, which is not reported.
- Only the Japanese model ships; Chinese and Thai (upstream's other models)
  would need a language choice in the style. The browser canvas renderer
  (`packages/web`) ignores `lineBreak`; it wraps only at whitespace.
