import { Rect } from '@celesta/react';
import { C, MONO_ADVANCE } from '../constants';

export function Caret({ x, size, visible }: { x: number; size: number; visible: boolean }) {
  return visible ? <Rect x={x} y={-size * 0.55} width={size * MONO_ADVANCE} height={size * 1.1} fill={C.mint} /> : null;
}
