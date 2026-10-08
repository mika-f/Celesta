# Project properties and .celesta.json projects

React compositions can declare project properties (template inputs such as a
title, colors, or a data file) and receive their values from the command
line, read values from a `.celesta.json` project, draw the project's
timeline, and render registered components that JSON timeline items name.
All imported from `@celesta/react`.

## Contents

- [Which pattern to use](#which-pattern-to-use)
- [defineProjectProperties](#defineprojectproperties)
- [Pass values from the command line](#pass-values-from-the-command-line)
- [Read values from a project](#read-values-from-a-project)
- [Draw a JSON timeline inside React](#draw-a-json-timeline-inside-react)
- [registerComponent](#registercomponent)

## Which pattern to use

| You want… | Do this |
| --- | --- |
| One source rendered with different titles, colors, or data files | `defineProjectProperties` + `useProjectProperty`/`getProjectProperty`, exported with `--props-file`/`--props` |
| A React video whose title/colors come from a JSON project file | `ProjectProvider` + `useProjectProperty` |
| A hand-placed JSON timeline with React overlays drawn over or under it | `<ProjectTimeline />` / `<ProjectTrack id>`, exported with `--react … --project …` |
| JSON items that render a React component (lower thirds, cards) | `registerComponent` + a JSON `component` item, exported with `--react … --project …` |

## defineProjectProperties

Declares the entry's inputs: their type, default, and Inspector label. Call
it at module level.

```ts
defineProjectProperties({
  title:  { type: 'string',  label: 'Title',  defaultValue: 'Celesta' },
  accent: { type: 'color',   label: 'Accent', defaultValue: '#ff8800' },
  size:   { type: 'number',  label: 'Size',   defaultValue: 48, min: 8, max: 200, step: 2 },
  sub:    { type: 'boolean', label: 'Subtitle', defaultValue: false },
  weight: { type: 'select',  label: 'Weight', defaultValue: 'bold', options: ['bold', 'light'] },
  data:   { type: 'path',    label: 'Data',   defaultValue: './data/sample.json' },
});
```

Read values while rendering with `useProjectProperty(key)` and in
`prepare()` (or anywhere outside a component) with `getProjectProperty(key)`.
Both fall back to the declared default, so the second argument is only
needed for undeclared keys. Neither works at module level: values are
checked after the entry's module code has run.

```tsx
let rows: Row[] = [];
export async function prepare() {
  // A `path` value is absolute by the time it is read.
  rows = JSON.parse(await fs.promises.readFile(getProjectProperty<string>('data'), 'utf8'));
}

export default function Card() {
  // Values are fixed for the whole render, so they may set the duration.
  return <Composition width={1280} height={720} fps={30} durationInFrames={30 + rows.length * 10}>
    <Title text={useProjectProperty<string>('title')} />
  </Composition>;
}
```

`min`, `max`, and `step` are Inspector hints; they are not enforced. The
app's Inspector is read-only.

## Pass values from the command line

```sh
Celesta-export --react card.tsx spring.mp4 --props-file variants/spring.json
Celesta-export --react card.tsx autumn.mp4 --props-file variants/autumn.json --props '{"title":"Autumn"}'
celesta-editor card.tsx --props-file variants/spring.json   # preview the same values
node <skill>/scripts/inspect.mjs card.tsx --props-file variants/spring.json
```

`--props-file` is a JSON object of values; `--props` is the same inline.
Where a value comes from, highest precedence first:

1. `--props`
2. `--props-file`
3. the `--project` companion file's `properties`
4. the nearest `<ProjectProvider>`'s `properties` (`useProjectProperty` only;
   `getProjectProperty` cannot see the React tree)
5. the `defineProjectProperties()` default
6. the `defaultValue` argument

Every `--props`/`--props-file` value is checked before `prepare()` runs, and
all problems are reported together (`--json` code `invalid_properties`, one
`issues` entry per key):

- the key must be declared (with no `defineProjectProperties()` call, any
  `--props` value is an error);
- `string`/`number`/`boolean` must have that JSON type; `color` must be
  `#RRGGBB` or `#RRGGBBAA`; `select` must be one of `options`;
- `path` must be an existing file or an `http(s)` URL. Relative paths in
  `--props-file` resolve from that file's folder, in `--props` from the
  current directory, and a relative default from the entry's folder.

A `--project` file's declared keys are checked the same way; its undeclared
keys are ignored. `<ProjectProvider>` values are not checked.

The editor re-reads `--props-file` on reload and when the file changes, and
exports from the editor use the same values.

## Read values from a project

```tsx
import type { Project } from '@celesta/react';
import projectFile from './project.celesta.json';
const project = projectFile as Project;

function Title() {
  const title = useProjectProperty('title', 'Chapter 1'); // key, fallback
  return <Text style={{ fontSize: 96 }}>{title}</Text>;
}

export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={90}>
    <ProjectProvider project={project}><Title /></ProjectProvider>
  </Composition>;
}
```

`useProject()` returns the whole project. `loadProject(path)` and
`loadProjectFromString(json)` also create a `Project`. Values come from the
project's top-level `properties` object.

## Draw a JSON timeline inside React

- `<ProjectTimeline />` draws the companion project's whole timeline;
  `<ProjectTrack id="titles" />` draws one track; `useProjectTrack(id)`
  returns its evaluated layers.
- These only work when the entry is exported or previewed with a companion
  project: `Celesta-export --react entry.tsx --project project.celesta.json out.mp4`.
  Without one they throw `<ProjectTimeline /> requires evaluated project layers`.
- `scripts/inspect.mjs` cannot evaluate them (they need the Rust evaluator);
  verify with a PNG frame export that passes `--project`.

## registerComponent

`registerComponent(name, Component, schema?)` lets a JSON item
`{ "type": "component", "component": "LowerThird", "props": { … } }` render a
React component through `<ProjectTimeline />`. Call it at module level.
Props must be plain JSON values; declare them with a `type` alias, not an
`interface` (interfaces do not satisfy the JSON constraint). The item's
`range`, `transform`, and `opacity` still come from the JSON.

```tsx
type LowerThirdProps = { name: string; role: string };
function LowerThird({ name, role }: LowerThirdProps) {
  return <Text style={{ fontSize: 48 }}>{`${name} · ${role}`}</Text>;
}
registerComponent<LowerThirdProps>('LowerThird', LowerThird, {
  name: { type: 'string', defaultValue: 'Mira' },
  role: { type: 'string', defaultValue: 'Host' },
});
```

Set the project's `settings.reactEntry` to the entry path so tools can find
the component schemas. Exporting the JSON project on its own, or with a name
that does not match `component` exactly, fails with a `missing component`
error on that layer.
