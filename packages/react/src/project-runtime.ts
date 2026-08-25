import * as React from 'react';
import type { ReactNode } from 'react';

import type { Project } from './generated/Project';
import type { JsonValue } from './generated/serde_json/JsonValue';
import { resolveComponent } from './registry';
import type { Layer } from './scene';

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
// There is no schema here yet — no declared type, default, or validation
// beyond "this call's own `defaultValue`" — so a property renamed or
// retyped in the editor silently falls back to `defaultValue` here rather
// than erroring. A schema-based API (`defineProjectProperties`, generating
// the Inspector fields) is intentionally deferred; see HANDOFF.md.
export function useProjectProperty<T extends JsonValue = JsonValue>(key: string, defaultValue: T): T {
  const project = useProject();
  const value = project.properties[key];
  return value === undefined ? defaultValue : (value as T);
}

// `<ProjectTimeline />` does not evaluate the project itself: mikan-evaluator
// (Rust) already does that, once per frame, for whatever project a project-
// aware export/preview call was given. render.ts wraps each render pass in
// this Provider with those pre-evaluated layers so this component only has
// to hand them to the reconciler. There is currently no way to ask Rust to
// evaluate on demand mid-render — the Rust side is synchronously blocked
// waiting for this render's response, so any such round trip would deadlock.
export const ProjectLayersContext = React.createContext<Layer[] | null>(null);

export function ProjectTimeline(): ReturnType<typeof React.createElement> {
  const layers = React.useContext(ProjectLayersContext);
  if (!layers) {
    throw new Error(
      '<ProjectTimeline /> requires evaluated project layers; export with a companion project (mikan-exporter --react <entry> --project <project.json>) to provide them',
    );
  }
  return renderProjectLayers(layers);
}

// Rust evaluates every track's layers alongside the whole-project ones on
// every project-aware frame (see mikan-exporter's render_react_video) —
// it cannot know in advance which track ids, if any, a <ProjectTrack />
// or useProjectTrack() call in the entry will ask for, so there is no
// negotiation step; the request just always carries all of them. An id
// with no matching track (typo, or a track disabled at the project level)
// evaluates to no layers on the Rust side (mikan-evaluator's
// `layers_for_track`), so it is absent from this map rather than present
// with an empty array — both `useProjectTrack` and `<ProjectTrack />`
// treat "absent" and "empty" the same way, as "nothing to show".
export const ProjectTrackLayersContext = React.createContext<Record<string, Layer[]> | null>(null);

function useProjectTrackLayers(hookName: string): Record<string, Layer[]> {
  const tracks = React.useContext(ProjectTrackLayersContext);
  if (!tracks) {
    throw new Error(
      `${hookName} requires evaluated project layers; export with a companion project (mikan-exporter --react <entry> --project <project.json>) to provide them`,
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
// (mikan-evaluator has no component registry of its own) to a
// `LayerContent::MissingComponent { component, props }` layer. This is
// where that name actually gets resolved against registerComponent()'s
// registry, on the Node side. Resolved, the registered component renders as
// a real subtree — its own hooks and state work normally — wrapped in a
// `group` that carries the transform/opacity mikan-evaluator already
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
        { id: layer.id, rawTransform: layer.transform, rawOpacity: layer.opacity },
        React.createElement(definition, layer.content.props),
      );
    }
  }
  return React.createElement('rawLayers', { layers: [layer] });
}
