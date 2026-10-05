# Combining React with a .celesta.json project

React compositions can read values from a `.celesta.json` project, declare
editable project properties, draw the project's timeline, and render
registered components that JSON timeline items name. All imported from
`@celesta/react`.

## Contents

- [Which pattern to use](#which-pattern-to-use)
- [Read values from a project](#read-values-from-a-project)
- [defineProjectProperties](#defineprojectproperties)
- [Draw a JSON timeline inside React](#draw-a-json-timeline-inside-react)
- [registerComponent](#registercomponent)

## Which pattern to use

| You want… | Do this |
| --- | --- |
| A React video whose title/colors come from a JSON file | `ProjectProvider` + `useProjectProperty` |
| Inspector-visible fields for those values | `defineProjectProperties` |
| A hand-placed JSON timeline with React overlays drawn over or under it | `<ProjectTimeline />` / `<ProjectTrack id>`, exported with `--react … --project …` |
| JSON items that render a React component (lower thirds, cards) | `registerComponent` + a JSON `component` item, exported with `--react … --project …` |

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

## defineProjectProperties

Declares the Inspector fields for `properties` (call it at module level):

```ts
defineProjectProperties({
  title:  { type: 'string',  label: 'Title',  defaultValue: 'Celesta' },
  accent: { type: 'color',   label: 'Accent', defaultValue: '#ff8800' },
  size:   { type: 'number',  label: 'Size',   defaultValue: 48, min: 8, max: 200, step: 2 },
  sub:    { type: 'boolean', label: 'Subtitle', defaultValue: false },
  weight: { type: 'select',  label: 'Weight', defaultValue: 'bold', options: ['bold', 'light'] },
});
```

The app's Inspector is read-only: users change values by editing the
project file, not in the GUI.

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
