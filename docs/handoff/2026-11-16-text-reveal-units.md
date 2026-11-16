# `<TextReveal unit>` (2026-11-16)

Design: [`docs/superpowers/specs/2026-11-16-animation-text-helpers-design.md`](../superpowers/specs/2026-11-16-animation-text-helpers-design.md), part 4.

## Specification

`<TextReveal unit>` chooses what slides in on its own: `'line'` (default,
unchanged), `'word'`, `'phrase'`, or `'grapheme'`.

- Lines are found first, as before (`\n` and `maxWidth`). Each line is then
  split: `'word'` into runs of non-whitespace, `'phrase'` with `phrases()`,
  `'grapheme'` with `graphemes()`. Whitespace is trimmed from a piece and a
  piece of only whitespace is dropped, so spaces and empty lines make no
  unit and use no stagger step.
- Each line with units is one `Group clip` (the line's mask, as before)
  holding one `Text` per unit, placed at its measured x in the line minus
  `align` times the line's width.
- `stagger` counts units in reading order across lines.
- Spans are sliced per unit as they are per line.
- An unknown `unit` throws.

## Implementation

`packages/text/src/text-motion.ts`. `TextReveal` branches before building
its lines; the `'line'` path is the code it was. Other units render
`RevealUnits`, a component of its own so its measuring hook does not run
for `'line'`.

`RevealUnits` builds one measurement request per line that has units (the
line's text, the layout style with the line's font runs, without colors,
`align`, or `lineHeight`) and memoises all of them on their JSON, so a
frame that changes only colors or progress measures nothing. The split
into units is memoised on the text, the line ranges, and `unit`. A unit's x is
the smallest `x` of the glyphs it covers where the measurer reports glyphs; where it
does not (the browser), it is the measured width of the line's text before
the unit, with the font runs that fall in that prefix.

The units of a line share the line's mask (one clip a line, four canvas
widths wide, as for `'line'`) rather than each having a box of its own:
only the top and bottom edges of the mask matter, so the picture is the
same with far fewer clipped groups for `'grapheme'`. The design note
suggested a box one font size wider than each unit; the shared mask needs
no unit width and cannot cut an italic or an overshoot.

Units are shaped separately when drawn, so kerning, ligatures, and joining
forms across a unit boundary are lost. With glyph positions a unit's x is
its leftmost glyph, so right-to-left and mixed-direction lines are placed
correctly; the prefix-width fallback assumes left to right. The glyph rule
is tested with a stub measurer that reports `rtl` glyphs, and a Hebrew line
with a Latin word in it, exported with `unit="word"` and with
`unit="line"`, looks the same at rest.

## Checks

- `pnpm --filter @celesta.studio/text exec vitest run test/text-reveal.test.mts`:
  `'line'` equals no `unit` on several frames, positions per unit for
  words, graphemes (a ZWJ family is one), and phrases, stagger across
  lines, whitespace and empty lines, `align` 0 / 0.5 / 1, `direction="out"`,
  spans, the no-glyph fallback, wrapped lines, one measurement per line and
  none on a later frame, and the unknown-unit error.
- `pnpm --filter @celesta.studio/text test`, `pnpm run test:runtime`.
- `celesta-exporter --react packages/cli/examples/with-text-reveal.tsx --frames 12,66 frame.png`,
  looked at: words and Japanese characters sit where the unsplit line puts
  them (Inter and Noto Sans JP), the centred, wrapped phrase block is
  centred line by line, and the `direction="out"` block sinks word by word.
