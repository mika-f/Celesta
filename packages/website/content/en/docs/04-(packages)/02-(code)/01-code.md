`@celesta-plugin/code` draws syntax-highlighted source code in a composition. It tokenizes with [twinkleplop](https://twinkleplop.pngwn.at) and draws each line as one of Celesta’s own `Text` layers with color runs, and line highlights as `Rect`s, so code scales, fades, blurs, and exports like any other layer. It is optional: `@celesta.studio/react` does not depend on it.

> [!NOTE]
> **Add the plugin**
> `@celesta-plugin/code` is a first-party plugin from Celesta's [plugin registry](/en/docs/plugins/#registry-plugins). Add it to the project with the `celesta` command, which declares it in `celesta.config.json`, locks it in `celesta.lock`, and downloads it:
>
> ```sh
> celesta plugin add code
> ```
>
> The plugin's versions follow Celesta's. On a pre-release of Celesta, name that version, as in `celesta plugin add code@0.10.0-alpha.4`: a requirement without a pre-release matches none. The desktop app does not read `celesta.lock` yet, so for its preview and exports also install the plugin for every project with `celesta plugin install --global code` (with the same version on a pre-release).
>
> A composition that imports `@celesta.studio/code` keeps working: Celesta still includes that package, and warns with the import to use and the `celesta plugin add` command to run.

## Your first code block

```tsx title="code-block.tsx"
import { Assets, Composition, Font, Rect } from '@celesta.studio/react';
import { Code, codeThemes } from '@celesta-plugin/code';

const source = `const message: string = 'Hello, Celesta.';
console.log(message);`;

const style = { fontFamily: 'IBM Plex Mono', fontSize: 40, lineHeight: 60 };

export default function Root() {
  return (
    <Composition width={1280} height={480} fps={30} durationInFrames={90}>
      <Assets>
        <Font src="./fonts/IBMPlexMono-Regular.ttf" />
      </Assets>
      <Rect width={1280} height={480} fill="#161418" />
      <Code x={64} y={64} language="ts" style={style} theme={codeThemes.dark}>
        {source}
      </Code>
    </Composition>
  );
}
```

`Code` takes the source as its only child, as a string. Keep indentation and trailing newlines inside the string: they are part of the picture.

## Fonts

Use a **monospaced** font. `Code` defaults to JetBrains Mono at 24 px with a line height of 1.5 × the font size. Load the font file with `<Font src="…" />`, as above, or install it on the machine that renders. If the font is missing, Celesta falls back as it does for any text.

## Props

**Code props**

| API | Use it for |
| --- | --- |
| `children` | The source, as a string. |
| `language` | `tsx`, `ts`, `json`, `bash`, or `text` (the default, which applies no coloring). |
| `style` | A Celesta `TextStyle`: `fontFamily`, `fontSize`, `lineHeight`, and so on. Text is left aligned and never wraps, and the theme supplies each run’s fill color. Other `align` values throw. In a font with coding ligatures, `fontFeatures: { liga: false, calt: false }` shows `===` and `->` as written. |
| `theme` | Token colors, a foreground fallback, and the line highlight color. `codeThemes.dark` by default. |
| `tabSize` | Width of a tab stop, counted in characters. Defaults to `2`. |
| `highlightLines` | One-based line numbers to mark with a highlight band. |
| `highlightWidth` | Band width in pixels. Defaults to the width of the source; set it to reach the edge of a surrounding panel. |
| `visibleCharacters` | How much of the source to show, for typing effects. See [code: typing, carets & tokens](/en/docs/code-typing/). |
| `x`, `y`, `scale`, `rotation`, `anchorX`/`anchorY`, `opacity`, `blendMode`, `blur`, `shadow`, `glow` | The usual layer props apply to the whole block. |

## Line highlights

`highlightLines` draws a band behind the lines you name, full width and full line height, even while the code is still being typed:

```tsx title="Mark one line"
<Code x={64} y={64} language="ts" style={style} highlightLines={[2]} highlightWidth={1152}>
  {source}
</Code>
```

Animate the highlight by changing the list as the frame advances; each band is cheap, so highlighting a line per beat or per cue is fine.

## Themes

`codeThemes.dark` and `codeThemes.light` are ready to use. A theme does not draw a panel background: place a `Rect` behind the code, sized to your design. To customize, spread a built-in theme and override what you need:

```tsx title="A custom theme"
const theme = {
  ...codeThemes.dark,
  highlightLine: '#a68bbf30',
  tokens: { ...codeThemes.dark.tokens, keyword: '#e0b7ff' },
};
```

Token names are twinkleplop’s, such as `keyword`, `string`, `comment`, `number`, `function`, `type`, `tag_name`, `attr_name`, `punctuation`, and `operator`. In JSON, object keys are `property`; string values keep `string` and `string_escape`. In bash, hyphens, dots, and slashes inside a word stay in its `identifier` (`Celesta-export`, `scripts/migrate-packages.mjs`), so `keyword` and `builtin` only mark whole words, and a flag such as `--bit-depth` is one `parameter`. A name your theme does not list uses `foreground`.

Continue with [typing effects, carets, and tokens](/en/docs/code-typing/).
