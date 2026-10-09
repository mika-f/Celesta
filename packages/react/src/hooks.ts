import * as React from 'react';

import type { Layer, Time } from './scene';

export interface VideoConfig {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
}

export interface CompositionRuntimeContextValue extends VideoConfig {
  time: Time;
  preview: boolean;
  lang?: string;
}

// Populated by render.ts around the entry's default export on every frame
// request; `time` is the only field that varies between requests, so a
// component that only reads `useVideoConfig()` does not re-render per frame.
export const CompositionRuntimeContext = React.createContext<CompositionRuntimeContextValue | null>(
  null,
);

/**
 * @internal The root composition's runtime, which `<Sequence>` never
 * overrides: `<FreezeFrame>` reads the composition's clock and duration here.
 */
export const RootRuntimeContext = React.createContext<CompositionRuntimeContextValue | null>(null);

/**
 * @internal Inside a `<FreezeFrame>`, an object unique to that freeze (and
 * stable across frames), which packages key per-freeze state on; null outside.
 */
export const FreezeFrameContext = React.createContext<object | null>(null);

// `<ProjectTimeline />` does not evaluate the project itself: celesta-evaluator
// (Rust) already does that, once per frame, for whatever project a project-
// aware export/preview call was given. render.ts wraps each render pass in
// this Provider with those pre-evaluated layers so `@celesta/project` only has
// to hand them to the reconciler. There is currently no way to ask Rust to
// evaluate on demand mid-render — the Rust side is synchronously blocked
// waiting for this render's response, so any such round trip would deadlock.
/** @internal */
export const ProjectLayersContext = React.createContext<Layer[] | null>(null);

// Rust evaluates every track's layers alongside the whole-project ones on
// every project-aware frame (see celesta-exporter's render_react_video) —
// it cannot know in advance which track ids, if any, a <ProjectTrack />
// or useProjectTrack() call in the entry will ask for, so there is no
// negotiation step; the request just always carries all of them. An id
// with no matching track (typo, or a track disabled at the project level)
// evaluates to no layers on the Rust side (celesta-evaluator's
// `layers_for_track`), so it is absent from this map rather than present
// with an empty array.
/** @internal */
export const ProjectTrackLayersContext = React.createContext<Record<string, Layer[]> | null>(null);

/**
 * @internal Asks render.ts to reconcile the current frame once more, for a
 * component that read a ref this commit had not attached yet (a
 * `<Dialogue>` mounted together with its `<CharacterView>`, say).
 */
export const RerenderRequestContext = React.createContext<() => void>(() => {});

/** @internal Shared language inheritance for React providers and the scene walker. */
export function resolveTextLanguage(lang: unknown, inherited?: string): string | undefined {
  if (lang === undefined) return inherited;
  if (typeof lang !== 'string') {
    throw new Error('Celesta components require a string `lang` prop');
  }
  return lang;
}

function useRuntimeContext(hookName: string): CompositionRuntimeContextValue {
  const value = React.useContext(CompositionRuntimeContext);
  if (!value) {
    throw new Error(`${hookName} must be called from within a Celesta <Composition>`);
  }
  return value;
}

export function useVideoConfig(): VideoConfig {
  const { width, height, fps, durationInFrames } = useRuntimeContext('useVideoConfig');
  return { width, height, fps, durationInFrames };
}

export function useCurrentTime(): Time {
  return useRuntimeContext('useCurrentTime').time;
}

export function useCurrentFrame(): number {
  const { time, fps } = useRuntimeContext('useCurrentFrame');
  return Math.round((time.value / time.timescale) * fps);
}

/** True only while the GPUI editor resolves a component for its preview. */
export function useIsPreview(): boolean {
  return useRuntimeContext('useIsPreview').preview;
}
