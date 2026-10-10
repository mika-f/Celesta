import { createContext, useContext, type ReactNode } from 'react';
import { BAR } from '../constants';

// What a demo needs to know about time: the film's absolute frame (scenes
// are Sequences, so useCurrentFrame() there is local) and when each of its
// stages started. The hook scene passes stages that started long ago, so
// every demo draws its finished, running state without knowing about it.
type Clock = { frame: number; stageAt: readonly number[]; accent: string };

const DemoClockContext = createContext<Clock>({ frame: 0, stageAt: [], accent: '#FFFFFF' });

export const SETTLED = -BAR * 100;

export function DemoClock({ children, ...clock }: Clock & { children: ReactNode }) {
  return <DemoClockContext.Provider value={clock}>{children}</DemoClockContext.Provider>;
}

export function useAccent(): string {
  return useContext(DemoClockContext).accent;
}

/** Absolute film frame. */
export function useFilmFrame(): number {
  return useContext(DemoClockContext).frame;
}

/** Frames since stage `index` (0-based) started; negative before it. */
export function useStage(index: number): number {
  const { frame, stageAt } = useContext(DemoClockContext);
  return frame - (stageAt[index] ?? SETTLED);
}

/**
 * A loop that restarts on every downbeat, from the first downbeat after stage
 * `index` started: `t` is the frame within the bar, `bar` counts bars since
 * then. `null` until that downbeat.
 */
export function useBarLoop(index: number): { t: number; bar: number } | null {
  const { frame, stageAt } = useContext(DemoClockContext);
  const first = Math.ceil((stageAt[index] ?? SETTLED) / BAR) * BAR;
  if (frame < first) return null;
  return { t: (frame - first) % BAR, bar: Math.floor((frame - first) / BAR) };
}
