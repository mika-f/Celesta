# React `<Font>` loads local font files (2026-09-25)

`<Font src name?>` used to render an ignored `asset-font` host node, so a
React entry could only use installed fonts. `mount().renderAt`
(`packages/react/src/render.ts`, `collectFonts`) now gathers every
`asset-font` in the tree — inside `<Assets>` or not, and regardless of
whether an enclosing `<Sequence>` is active — into `Scene.fonts` (omitted
when empty; identical id+location repeats are merged). `name` overrides the
id; `fontFamily` still matches the family name inside the file. Relative
paths resolve against the renderer's `asset_root` (the entry's directory in
both the editor preview and `celesta-exporter --react`); URLs fail like any
other remote asset. `TextRasterizer::load_fonts` now dedupes by the resolved
file path instead of the asset id, so a reload that changes a `<Font>`'s
`src` but keeps its `name` loads the new file. Covered by
`packages/react/test/font.test.mjs`.
