import { Easings } from '@celesta/react';
import { Label } from './Label';
import { C } from '../constants';
import { progress } from '../helpers';

// A headline that swaps on the given frames, each new line sliding up.
export function Swap({ f, cues, x, y, size, color = C.paper, accentLast = false }: {
  f: number; cues: [number, string][]; x: number; y: number; size: number; color?: string; accentLast?: boolean;
}) {
  const index = cues.reduce((found, [at], i) => (f >= at ? i : found), -1);
  if (index < 0) return null;
  const [at, text] = cues[index];
  const p = progress(f, at, 10, Easings.easeOutExpo);
  const last = accentLast && index === cues.length - 1;
  return <Label x={x} y={y + 40 * (1 - p)} size={size} opacity={p} color={last ? C.accent : color}>{text}</Label>;
}
