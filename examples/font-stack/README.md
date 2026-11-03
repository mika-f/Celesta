# Font stacks (和欧混植)

`fontFamily` takes a list of families, and each character is drawn with the
first family in the list that has it. Latin comes from Inter or JetBrains
Mono and Japanese from Noto Sans JP, in one `<Text>`. Six seconds at 30 fps,
1920 × 1080, in three sections:

1. A title and a wrapped paragraph in `['Inter', 'Noto Sans JP']`, with a
   `<Span>` inside. The lines keep Inter's baselines, so line spacing stays
   even.
2. The same line with `fontFamily: 'Inter'`, where the Japanese falls back to
   a system font even though Noto Sans JP is loaded, and with the stack.
3. `@celesta.studio/code`'s `<Code>` typing TSX with Japanese comments and strings
   in `['JetBrains Mono', 'Noto Sans JP']`; the highlighted lines cover them.

The composition is `film.tsx`. The fonts come from Google Fonts, so the first
export needs a network connection. From the repository root:

```sh
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/font-stack/film.tsx --frame 30,90,170 examples/font-stack/frame.png
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/font-stack/film.tsx examples/font-stack/font-stack.mp4
```

The export prints no font warning for the stacked text; the warnings it
prints for section 2's `fontFamily: 'Inter'` line show what the stack fixes.
