import { Group, Rect } from '@celesta/react';
import { Label } from './Label';
import { C } from '../constants';

// A section tag: accent square, number, and name.
export function Tag({ x, y, n, name, opacity = 1, color = C.paper }: {
  x: number; y: number; n: string; name: string; opacity?: number; color?: string;
}) {
  return (
    <Group x={x} y={y} opacity={opacity}>
      <Rect y={-7} width={14} height={14} fill={C.accent} />
      <Label x={30} y={0} size={22} font="mono" weight={700} color={C.accent} ay={0.5}>{n}</Label>
      <Label x={80} y={0} size={22} font="mono" weight={400} color={color} ay={0.5}>{name}</Label>
    </Group>
  );
}
