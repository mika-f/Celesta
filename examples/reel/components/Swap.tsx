import { Easings, Text, cueAt, progress } from '@celesta.studio/react';
import { C } from '../constants';

// A headline that swaps on the given frames, each new line sliding up.
export function Swap({ f, cues, x, y, size, color = C.paper, accentLast = false }: {
  f: number; cues: [number, string][]; x: number; y: number; size: number; color?: string; accentLast?: boolean;
}) {
  const active = cueAt(cues.map(([at, text]) => ({ at, text })), f);
  if (!active) return null;
  const p = progress(active.frame, 0, 10, Easings.easeOutExpo);
  const last = accentLast && !active.next;
  return <Text x={x} y={y + 40 * (1 - p)} fontFamily="display" fontSize={size} fontWeight={700} color={last ? C.accent : color} opacity={p}>{active.cue.text}</Text>;
}
