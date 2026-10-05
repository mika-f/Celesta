# Drawing subtitles with a render prop (2026-10-05)

`CharacterSubtitle.render` (`packages/react/src/components.ts`) lets a
character draw its own subtitle, e.g. a band behind the text and a name
plate (issue #85). `<Dialogue>` expands it as React: it resolves the view
ref to the character, measures the line with the subtitle's `style` /
`maxWidth` through the synchronous measurer (`useTextMetrics()`'s), and
renders the callback's output, as a component inside
`<Group {...placement}>`, as the `dialogue` host's only child. render.ts's `dialogue` branch walks that child instead of
building the `text` layer when the character's subtitle has `render`.

- The render gets `{ text, character: { id, name, displayName }, metrics,
  style, maxWidth, held, frame, durationInFrames }`. `<Character
  displayName>` defaults to `name`.
- A `<CharacterView>` mounted in the same commit as its `<Dialogue>` has no
  ref yet during render. `<Dialogue>` then calls the internal
  `RerenderRequestContext`, and `renderFrame` reconciles once more (the same
  pass that picks up late `<Font>` declarations) before walking;
  `createResolver()` does the same for editor previews.
- `<Dialogue held>` keeps the subtitle up after its line: `render` gets
  `held: true`, a plain subtitle draws nothing.
- `<DialogueSeries holdSubtitle>` adds a held `<Dialogue>` (no audio or
  expression) in each line's gap, and splits the plan into runs at lines
  with a lead-in. The internal `SubtitleRunContext` makes `frame` /
  `durationInFrames` count over the run, so a fade plays once per run.
- `examples/with-dialogue-series.tsx` uses it.
