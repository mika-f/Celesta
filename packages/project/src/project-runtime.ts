import * as React from 'react';
import type { ReactNode } from 'react';

import type { JsonValue, Layer } from '@celesta/react';
import {
  FreezeFrameContext,
  ProjectLayersContext,
  ProjectTrackLayersContext,
  declaredDefault,
  inputProjectProperty,
  resolveComponent,
} from '@celesta/react/internal';
import type { Project } from '@celesta/react/internal';

// `Project` here is GUI-editor-owned data the entry chooses to load and wrap
// its tree with (see project.ts's loadProject); `useProject()` gives plain
// read access to it, no different from any other context value.
export const ProjectContext = React.createContext<Project | null>(null);

export interface ProjectProviderProps {
  project: Project;
  children?: ReactNode;
}

export function ProjectProvider(props: ProjectProviderProps): ReturnType<typeof React.createElement> {
  return React.createElement(ProjectContext.Provider, { value: props.project }, props.children);
}

export function useProject(): Project {
  const project = React.useContext(ProjectContext);
  if (!project) {
    throw new Error('useProject must be called from within a <ProjectProvider>');
  }
  return project;
}

// `Project.properties` (`Record<string, JsonValue>`) is the GUI-editable
// value store: the editor writes plain JSON values there, and React reads
// them back with this hook rather than hardcoding them into the entry.
// Values from outside the source (`--props`, `--props-file`, the companion
// project) win over the nearest `<ProjectProvider>`, which wins over the
// `defineProjectProperties()` default and then `defaultValue`. Only the
// outside values are validated against the schema; a Provider's value is
// returned as stored. Without a Provider the hook still reads the outside
// values and defaults.
export function useProjectProperty<T extends JsonValue = JsonValue>(key: string, defaultValue?: T): T {
  const project = React.useContext(ProjectContext);
  // A Provider may store `null`, which is a value and must not fall through.
  let value = inputProjectProperty(key);
  if (value === undefined && project && Object.prototype.hasOwnProperty.call(project.properties, key)) {
    value = project.properties[key];
  }
  if (value === undefined) value = declaredDefault(key) ?? defaultValue;
  if (value === undefined) {
    throw new Error(`useProjectProperty("${key}"): the property is not declared with defineProjectProperties() and has no default value`);
  }
  return value as T;
}

// `<ProjectTimeline />` does not evaluate the project itself: the Rust side
// evaluates it once per frame and the core renderer provides the layers (see
// `ProjectLayersContext`), so this only hands them to the reconciler.

export function ProjectTimeline(): ReturnType<typeof React.createElement> {
  useOutsideFreezeFrame('<ProjectTimeline />');
  const layers = React.useContext(ProjectLayersContext);
  if (!layers) {
    throw new Error(
      '<ProjectTimeline /> requires evaluated project layers; export with a companion project (celesta-exporter --react <entry> --project <project.json>) to provide them',
    );
  }
  return renderProjectLayers(layers);
}

// Every track's layers arrive with each project-aware frame. A track id with
// no layers is absent from the map; `useProjectTrack` and `<ProjectTrack />`
// treat "absent" and "empty" the same way, as "nothing to show".

/** Project layers are evaluated at the current frame only, so a frozen copy would show the wrong moment. */
function useOutsideFreezeFrame(name: string): void {
  if (React.useContext(FreezeFrameContext)) {
    throw new Error(`${name} cannot be drawn inside <FreezeFrame>: project layers are evaluated at the current frame only`);
  }
}

function useProjectTrackLayers(hookName: string): Record<string, Layer[]> {
  useOutsideFreezeFrame(hookName);
  const tracks = React.useContext(ProjectTrackLayersContext);
  if (!tracks) {
    throw new Error(
      `${hookName} requires evaluated project layers; export with a companion project (celesta-exporter --react <entry> --project <project.json>) to provide them`,
    );
  }
  return tracks;
}

/** The given project track's evaluated layers, or `[]` if it has none (including an unknown track id). */
export function useProjectTrack(trackId: string): Layer[] {
  const tracks = useProjectTrackLayers('useProjectTrack');
  return tracks[trackId] ?? [];
}

export interface ProjectTrackProps {
  id: string;
}

export function ProjectTrack(props: ProjectTrackProps): ReturnType<typeof React.createElement> {
  const tracks = useProjectTrackLayers('<ProjectTrack />');
  return renderProjectLayers(tracks[props.id] ?? []);
}

function renderProjectLayers(layers: Layer[]): ReturnType<typeof React.createElement> {
  return React.createElement(
    React.Fragment,
    null,
    layers.map((layer) => React.createElement(ResolvedProjectLayer, { key: layer.id, layer })),
  );
}

// A project.json `TimelineContent::Component` item always evaluates
// (celesta-evaluator has no component registry of its own) to a
// `LayerContent::MissingComponent { component, props }` layer. This is
// where that name actually gets resolved against registerComponent()'s
// registry, on the Node side. Resolved, the registered component renders as
// a real subtree — its own hooks and state work normally — wrapped in a
// `group` that carries the transform/opacity celesta-evaluator already
// computed for that timeline item (see the `rawTransform`/`rawOpacity`
// escape hatch in render.ts), so its authored position on the timeline is
// preserved regardless of what the component itself renders. Unresolved
// (no matching registerComponent() call), the layer passes through as-is:
// `GpuRenderer` errors on `missingComponent` content, which is the honest
// outcome for a name the entry never registered.
function ResolvedProjectLayer({ layer }: { layer: Layer }): ReturnType<typeof React.createElement> {
  if (layer.content.type === 'missingComponent') {
    const definition = resolveComponent(layer.content.component);
    if (definition) {
      return React.createElement(
        'group',
        {
          id: layer.id,
          rawTransform: layer.transform,
          rawOpacity: layer.opacity,
          blendMode: layer.blendMode,
        },
        React.createElement(definition, layer.content.props),
      );
    }
  }
  return React.createElement('rawLayers', { layers: [layer] });
}
