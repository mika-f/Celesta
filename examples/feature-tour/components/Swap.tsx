import type { ReactNode } from 'react';
import { Easings } from '@celesta/react';
import { progress } from '../helpers';

// Swaps between strings on the given frames; each new one rises into place.
export function Swap({ f, cues, render }: {
  f: number;
  cues: [number, string][];
  render: (text: string, p: number) => ReactNode;
}) {
  const index = cues.reduce((found, [at], i) => (f >= at ? i : found), -1);
  if (index < 0) return null;
  const [at, text] = cues[index];
  return <>{render(text, index === 0 ? 1 : progress(f, at, 10, Easings.easeOutExpo))}</>;
}
