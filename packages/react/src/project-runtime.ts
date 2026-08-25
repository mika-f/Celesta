import * as React from 'react';
import type { ReactNode } from 'react';

import type { Project } from './generated/Project';
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
  return React.createElement('rawLayers', { layers });
}
