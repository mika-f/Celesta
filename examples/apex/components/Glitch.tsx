import { random } from '@celesta/react';
import { Label } from './Label';
import { BONE, CYAN, MAG } from '../constants';

// Chromatic-split text: two additive ghosts that tear apart on beats and random glitch frames.
export function Glitch({ children, x, y, size, frame, amount, color = BONE }: {
  children: string; x: number; y: number; size: number; frame: number; amount: number; color?: string;
}) {
  const burst = random(`g${frame}`) > 0.88 ? 1 : 0;
  const d = amount + burst * 38;
  const jy = burst * (random(`j${frame}`) - 0.5) * 24;
  return <>
    <Label x={x - d} y={y + jy} size={size} color={MAG} anchorX={0.5} anchorY={0.5} blendMode="add" opacity={0.85}>{children}</Label>
    <Label x={x + d} y={y - jy} size={size} color={CYAN} anchorX={0.5} anchorY={0.5} blendMode="add" opacity={0.85}>{children}</Label>
    <Label x={x} y={y} size={size} color={color} anchorX={0.5} anchorY={0.5}>{children}</Label>
  </>;
}
