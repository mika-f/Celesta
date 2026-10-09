# Rich text

`<Span>` changes the color, weight, or family of part of a `<Text>`, and the
whole text is still laid out as one paragraph: it wraps, aligns, and keeps
its baseline as one. Eight seconds at 30 fps, 1920 × 1080, in four sections:

1. An explanatory paragraph with a brand-font word and a bold, colored phrase,
   wrapped between Japanese phrases (`lineBreak: 'phrase'`).
2. A character's subtitle with an emphasized phrase (`<Dialogue>` children).
3. A `<TextReveal>` headline whose lines hold spans.
4. Typing with `useTypewriter()`: its `length` goes to `style.visibleCharacters`,
   which reveals the text without changing its layout.

The composition is `film.tsx`. From the repository root:

```sh
node skills/celesta/scripts/inspect.mjs examples/rich-text/film.tsx --frames 30,90,150,230
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/rich-text/film.tsx examples/rich-text/rich-text.mp4
```

Bebas Neue is referenced from `../afterimage/assets/fonts/` (SIL Open Font
License); the portraits come from `../assets/dialogue-demo/portraits/`.
