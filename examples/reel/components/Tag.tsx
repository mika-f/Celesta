import { Group, Rect, Text } from '@celesta.studio/react';
import { C } from '../constants';

// A section tag: accent square, number, and name.
export function Tag({ x, y, n, name, opacity = 1, color = C.paper }: {
  x: number; y: number; n: string; name: string; opacity?: number; color?: string;
}) {
  return (
    <Group x={x} y={y} opacity={opacity}>
      <Rect y={-7} width={14} height={14} fill={C.accent} />
      <Text x={30} y={0} fontFamily="mono" fontSize={22} fontWeight={700} color="accent" anchorY={0.5}>{n}</Text>
      <Text x={80} y={0} fontFamily="mono" fontSize={22} fontWeight={400} color={color} anchorY={0.5}>{name}</Text>
    </Group>
  );
}
