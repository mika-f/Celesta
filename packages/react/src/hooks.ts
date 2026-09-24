import * as React from 'react';

import type { Time } from './scene';

export interface VideoConfig {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
}

export interface CompositionRuntimeContextValue extends VideoConfig {
  time: Time;
  preview: boolean;
}

// Populated by render.ts around the entry's default export on every frame
// request; `time` is the only field that varies between requests, so a
// component that only reads `useVideoConfig()` does not re-render per frame.
export const CompositionRuntimeContext = React.createContext<CompositionRuntimeContextValue | null>(
  null,
);

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
