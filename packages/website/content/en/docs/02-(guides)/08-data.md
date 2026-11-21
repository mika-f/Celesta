## Load data before rendering

Each frame is rendered synchronously, so a component cannot wait for a file or a network request. Do that work in an `async prepare()` export instead. It runs once, before the first frame, in both the preview and exports. Store the results in module-level variables, then read them while rendering:

```tsx title="Fetch once, use on every frame"
let headlines: string[] = ['Hello from Celesta'];

export async function prepare() {
  try {
    const response = await fetch('https://example.com/headlines.json');
    headlines = (await response.json()) as string[];
  } catch (error) {
    console.error('Using the fallback headline', error);
  }
}
```

Keep a fallback value so the scene still renders offline. `loadLipSync()`, `loadPsdPreset()`, and `preloadMedia()` belong in `prepare()` for the same reason.

## Reuse components with data and props

Keep settings in ordinary objects or imported JSON data and pass them to React components as props. A data JSON file is not a .celesta.json timeline project. Type the props so your editor checks each use.

```tsx file=examples/data.tsx title="data.tsx"
```

> [!NOTE]
> **Existing JSON project integrations**
> ProjectProvider, useProjectProperty(), defineProjectProperties(), and registerComponent() still support existing JSON project integrations. They are not required for new React compositions; see [the legacy timeline guide](/en/docs/timelines/) when maintaining those files.

## Switch palettes without editing code

A palette gives every color of a film one of four roles: `ground` lies behind everything, `main` is the chief color, `second` is the ink for words, and `accent` is a touch here and there. Colors made from them, such as a card a little lighter than the ground or a see-through line, are named once in `derived`, so each palette makes its own. Draw with the role and derived names alone, and another palette recolors the whole film.

```tsx title="Two palettes, picked in the Properties panel"
import {
  Composition, Rect, Text, Theme, alpha, defineProjectProperties, definePalettes, getProjectProperty, mix,
} from '@celesta.studio/react';

const palettes = definePalettes({
  harbor: { ground: '#0F2A3D', main: '#5FD3B5', second: '#EAF4F2', accent: '#F2B544' },
  paper:  { ground: '#F4F1EA', main: '#1F4E79', second: '#111111', accent: '#D93A12' },
}, {
  derived: {
    card: mix('ground', 'main', 0.14), // 86% ground, 14% main
    veil: alpha('second', 0.2),        // second at 20% opacity
  },
});

defineProjectProperties({ palette: palettes.property({ label: 'Palette' }) });

export default function Film() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={90}>
      <Theme palette={palettes.get(getProjectProperty('palette'))}>
        <Rect width={1920} height={1080} fill="ground" />
        <Rect x={160} y={160} width={1600} height={760} fill="card" stroke="veil" strokeWidth={2} />
        <Text x={240} y={240} fontSize={96} fontWeight={700} color="second">Harbor report</Text>
        <Rect x={240} y={380} width={320} height={12} fill="accent" />
      </Theme>
    </Composition>
  );
}
```

- `<Theme palette>` names the roles and the derived colors wherever a `<Theme>` color name works: `<Text color>`, a shape's `fill` and `stroke`, and the color props of `@celesta.studio/components`. `colors` given with it win over the palette's, and a nested `<Theme>` adds to it as usual. `useTheme().palette` is the palette in effect.
- `mix(a, b, amount)` is `amount` of the way from `a` to `b`, like CSS `color-mix(in srgb, …)`; `alpha(color, amount)` keeps the color and scales its opacity. Each side is a role, a derived name defined above it, or a hex color.
- `palettes.property()` is a menu property whose choices are the palette names, so the Properties panel, a `props.json` beside the entry, and the exporter's `--props '{"palette":"paper"}'` or `--props-file` pick a palette. A name that is not a palette is refused before the first frame.
- Each role must be a `#RRGGBB` or `#RRGGBBAA` color, or `definePalettes()` throws. When `second` has less than 4.5:1 contrast on `ground`, too little for text, it prints a warning that names the palette and the ratio; `palettes.check()` returns the same findings.
