import type { ReactNode } from 'react';
import { Easings, cueAt, progress } from '@celesta.studio/react';

// Swaps between strings on the given frames; each new one rises into place.
export function Swap({ f, cues, render }: {
  f: number;
  cues: [number, string][];
  render: (text: string, p: number) => ReactNode;
}) {
  const active = cueAt(cues.map(([at, text]) => ({ at, text })), f);
  if (!active) return null;
  return <>{render(active.cue.text, active.index === 0 ? 1 : progress(active.frame, 0, 10, Easings.easeOutExpo))}</>;
}
