The installed app already includes the React runtime. [Create a project](/en/docs/create-project/) from the app or CLI to get a starter `film.tsx` and TypeScript configuration. The starter previews and exports without a package installation or build step. Install any additional npm dependencies in your project folder.

## Make your first scene

Create `first-scene.tsx` in your own project folder. This example creates a 1920 × 1080 composition at 30 fps. The title fades in over 30 frames, then holds until the five-second composition ends.

```tsx file=examples/first-scene.tsx title="first-scene.tsx"
```

In Celesta, choose **File → Open…** and select `first-scene.tsx`. Press `Space` to play.

Change the text or colors and save. The native preview reloads the composition. Celesta components describe video layers: use `Text`, `Rect`, and `Group` rather than HTML elements or CSS classes.

## How a composition is put together

- The file’s **default export** returns one or more root `<Composition>` elements with its size, frame rate, and length.
- Everything inside it is drawn in order: later layers appear on top of earlier ones.
- Your own components are ordinary React function components. Use props, `.map()`, and conditionals as you would anywhere else.
- An optional named export, `prepare()`, runs once before the first frame. See [Data, properties & components](/en/docs/data/).

## Register multiple compositions

Give each root a unique, non-empty `id` and return them in a fragment. Each can have its own dimensions, fps, and duration. A single existing composition can still omit `id` (its ID is `default`). Without a selection, Celesta uses the first root in declaration order; an unknown ID is an error.

```tsx
import { Composition, Text } from '@celesta.studio/react';

export default () => <>
  <Composition id="landscape" width={1920} height={1080} fps={30} durationInFrames={150}>
    <Text>Landscape</Text>
  </Composition>
  <Composition id="portrait" width={1080} height={1920} fps={30} durationInFrames={150}>
    <Text>Portrait</Text>
  </Composition>
</>;
```

Choose **Composition: landscape** in the app's title bar to switch the preview and the composition that **Export…** writes. You can also open a specific ID with `Celesta film.tsx --composition portrait`.

```sh
Celesta-export --react film.tsx --composition portrait output.mp4 --json
node skills/celesta/scripts/inspect.mjs film.tsx --composition portrait --json
```

Both JSON outputs include `compositionId` and `compositions` (the inspector places these inside `ready`). The list includes each root's ID, dimensions, frame rate, and frame count. `defineProjectProperties`, `props.json`, `--props`, and `prepare()` are shared by the entry; only the selected composition's children are mounted.

## Set up TypeScript in your editor

With a React composition open, choose **File → Set Up TypeScript**. Celesta copies matching declarations for every `@celesta.studio/*` package, React, and Node.js into a local `.celesta/` directory.

Celesta puts `.celesta/` and `tsconfig.json` in the project root: the nearest folder, from the composition's own up, that holds `celesta.config.json`, or else the composition's own folder. A `package.json` or `tsconfig.json` in a folder above does not count, so Celesta never writes outside the project. Projects created with `--init` or **Create New Project** already have `celesta.config.json` beside the composition.

Starting a new project with no composition yet, or keeping the composition in a subfolder such as `src/`? Choose **File → Set Up TypeScript in Folder…** and pick the project folder. Celesta sets up types there and adds an empty `celesta.config.json` if the folder has none, so compositions in its subfolders use it as their root. If you set up TypeScript in the project folder of such a project with an earlier version, do this once so Celesta keeps refreshing that `.celesta/`.

If there is no `tsconfig.json`, Celesta creates one. If you already have a configuration, add this `extends` property while preserving your other settings:

```json title="tsconfig.json"
{
  "extends": "./.celesta/tsconfig.json"
}
```

You do not need to install those declaration packages from npm for this editor setup. Celesta refreshes `.celesta/` when you open the project in a newer app version; that directory ignores itself in Git.

## Projects that import `@celesta/*`

The Celesta packages used to be named `@celesta/*` and are now `@celesta.studio/*`, such as `@celesta.studio/react`. Compositions that still import the old names keep working: the app, the CLI, and the web editor resolve `@celesta/react` to `@celesta.studio/react`, and `.celesta/tsconfig.json` maps both names to the same declarations. Each file that imports an old name gets a warning, such as `scenes/Intro.tsx imports @celesta/react, an old name; import @celesta.studio/react instead`: in the app's **Problems** tab, in `Celesta-export` output (and its `--json` report's `warnings`), and in the web editor's browser console. New code, the templates, and the app's own edits use `@celesta.studio/*`.

To rename the imports in a project at once, run the migration script from the Celesta skill. It also renames `@celesta/*` dependencies in a `package.json` next to the files it changes.

```sh
node skills/celesta/scripts/migrate-packages.mjs path/to/project --dry-run
node skills/celesta/scripts/migrate-packages.mjs path/to/project
```
