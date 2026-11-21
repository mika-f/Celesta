# Role palettes

## Specification

A whole-film palette, after Motion's palette model: every color of a film
comes from four roles and blends of them, so another palette recolors the
whole film.

- Roles: `ground` (behind everything), `main` (the chief color), `second`
  (ink for words), `accent` (a touch here and there). Each is `#RRGGBB` or
  `#RRGGBBAA`.
- `definePalettes(palettes, { derived?, default? })` declares a film's
  palettes. `derived` names colors made from the roles, the same for every
  palette: `mix(a, b, t)` (CSS `color-mix(in srgb, a (1-t), b t)`,
  premultiplied) and `alpha(c, t)` (`color-mix(in srgb, c t, transparent)`).
  Each side is a role, a derived name defined before it, or a hex color.
- `<Theme palette={palette}>` adds the palette's roles and derived colors to
  the theme's color names, so they work wherever a `<Theme>` color name
  works today. `colors` given with it win; a nested `<Theme colors>` adds to
  it; an inner `palette` replaces the outer one in `useTheme().palette`.
  `<Theme colors>` without a palette is unchanged.
- Switching without editing code: `palettes.property({ label })` returns a
  plain `select` project property field (options = palette names, default =
  the default palette). The Properties panel's menu, `props.json`,
  `--props`, and `--props-file` pick a palette through the existing project
  property path; `palettes.get(getProjectProperty('palette'))` resolves it.
- Validation, at palette level only: `definePalettes()` throws on a missing
  role, a role that is not a hex color, an unknown key, a derived color that
  names something not defined above it, an amount outside 0 to 1, and a
  derived name that is a role. It `console.warn`s (stderr, so the exporter's
  terminal and the editor's log) for each palette whose `second` is below
  4.5:1 WCAG contrast on `ground`; `palettes.check()` returns the same
  findings. The rendered, layer-level contrast lint is a separate task.

The user chose the `select` route (no Rust changes) over a dedicated
`palette` field type with swatches in the Properties panel. Swatches remain
a possible follow-up: a new `ComponentPropertyField` variant in
`crates/react-bridge/src/types.rs`, its row in the editor's property panel,
the CLI's validation in `packages/cli/src/property-inputs.ts`, and
`registry.ts`'s union.

## Implementation

- `packages/react/src/palette.ts`: `definePalettes`, `mix`, `alpha`,
  `contrastRatio` (WCAG, a see-through foreground laid over the background),
  `PALETTE_ROLES`, `MIN_TEXT_CONTRAST`, and the types. Derived colors are
  computed once per palette at definition and stored as upper-case hex
  (`#RRGGBBAA` only when not opaque). Exported from `index.ts` and
  `browser.ts`.
- `packages/react/src/theme.ts`: `ThemeValues.palette`, `ResolvedTheme.palette`;
  `mergeTheme` layers `outer.colors`, then `palette.colors`, then `colors`;
  `checkEntries` checks a palette's colors like `colors`.
- `packages/cli/examples/with-theme.tsx` now draws from two palettes
  (`ink`, the old look, and `paper`) with a palette property.
- Docs: `skills/celesta/references/text.md#palettes`,
  `project-data.md`, `SKILL.md`; the website's data guide (en/ja) and the
  reference table.

## Checks

- `pnpm --filter @celesta.studio/react test` (`test/palette.test.mts`:
  roles, derived colors, premultiplied mixing, errors, contrast warnings,
  `<Theme palette>` merging, picking through project property values, the
  browser entry).
- `packages/cli/test/palette-props.test.mts` (after `pnpm run build:runtime`):
  `inspect.mjs --props '{"palette":"paper"}'` draws the chosen palette, the
  default draws without one, an unknown name is refused, and the
  low-contrast warning reaches stderr.
- Rendered frame 0 of `with-theme.tsx` before and after, and the `paper`
  palette, with `celesta-exporter --react … --frame 0`.
