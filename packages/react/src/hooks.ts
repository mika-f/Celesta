import * as React from 'react';

import type { FontFallbackMode, Layer, Time } from './scene';

/** @internal undefined outside a mount, null while discovering roots. */
export const CompositionSelectionContext = React.createContext<string | null | undefined>(undefined);

export interface VideoConfig {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
}

export interface CompositionRuntimeContextValue extends VideoConfig {
  time: Time;
  preview: boolean;
  /**
   * True when the caller asked for debug guides outside the preview: PNG
   * exports with `--debug-guides`. Never set for video exports.
   */
  debugGuides?: boolean;
  lang?: string;
  /** Where text's missing characters are drawn from; see `TextStyle.fontFallback`. */
  fontFallback?: FontFallbackMode;
  /**
   * True while a frame past the composition's end is evaluated as if the
   * video went on (inspect.mjs --loop compares frame N with frame 0), and
   * this window ends with the composition: a `<Sequence>` here that also
   * ends with it stays open.
   */
  openEnd?: boolean;
  /**
   * Inside a `<MotionBlur>` sample or a `<Trail>` copy, how far its time is
   * from a whole frame, in frames: `time` is that moment rounded to a
   * microsecond, and `useCurrentFrame()` adds this to the whole frame
   * instead of rounding, so it reports the sample's frame exactly. Absent
   * elsewhere.
   */
  frameOffset?: number;
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
 * Inside a `<FreezeFrame>`, the clock is the frozen one.
 */
export const RootRuntimeContext = React.createContext<CompositionRuntimeContextValue | null>(null);

/**
 * @internal Inside a `<FreezeFrame>`, a `<MotionBlur>` sample, or a past
 * `<Trail>` copy (each a copy of the composition drawn at a time other than
 * the frame's), an object unique to that copy (and stable across frames),
 * which packages key per-copy state on; null outside.
 */
export const FreezeFrameContext = React.createContext<object | null>(null);

// `<ProjectTimeline />` does not evaluate the project itself: celesta-evaluator
// (Rust) already does that, once per frame, for whatever project a project-
// aware export/preview call was given. render.ts wraps each render pass in
// this Provider with those pre-evaluated layers so `@celesta.studio/project` only has
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

/** @internal Shared `fontFallback` inheritance for React providers and the scene walker. */
export function resolveFontFallback(value: unknown, inherited?: FontFallbackMode): FontFallbackMode | undefined {
  if (value === undefined) return inherited;
  if (value !== 'system' && value !== 'loaded') {
    throw new Error("Celesta components require `fontFallback` to be 'system' or 'loaded'");
  }
  return value;
}

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

/**
 * The frame being drawn: a whole number, except inside a `<MotionBlur>` or
 * a `<Trail>` copy, where it is the exact frame of the sample being drawn
 * (12.25, say).
 */
export function useCurrentFrame(): number {
  const { time, fps, frameOffset } = useRuntimeContext('useCurrentFrame');
  return runtimeFrame(time, fps, frameOffset);
}

/** @internal The frame `useCurrentFrame()` reports for a runtime context. */
export function runtimeFrame(time: Time, fps: number, frameOffset = 0): number {
  return Math.round((time.value / time.timescale) * fps - frameOffset) + frameOffset;
}

/** True only while the GPUI editor resolves a component for its preview. */
export function useIsPreview(): boolean {
  return useRuntimeContext('useIsPreview').preview;
}

/**
 * True where debug guides are drawn: in the editor preview, and in PNG
 * frames and contact sheets exported with `--debug-guides`. Never in a video.
 */
export function useDebugGuides(): boolean {
  const { preview, debugGuides } = useRuntimeContext('useDebugGuides');
  return preview || debugGuides === true;
}
