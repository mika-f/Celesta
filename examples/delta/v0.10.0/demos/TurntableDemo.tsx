import { Group, Rect, Text } from '@celesta.studio/react';
import { Turntable } from '@celesta-plugin/turntable';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { mono, rise } from './common';

// 3D SCENES: a real <Turntable> from examples/plugins/turntable. The
// plugin's process returns a scene (a camera, a light, a cube with its own
// WGSL toon material and outline, cut-out and see-through panes) and
// Celesta draws it with depth. The labels name what the scene carries.

const PARTS: readonly [string, string][] = [
  ['camera', 'eye (0, 1.2, 3) · fov 0.8'],
  ['light', 'directional'],
  ['material', 'toon + outline, WGSL'],
  ['unlit', 'cut-out leaf, see-through pane'],
  ['depth', 'drawn by Celesta'],
];

export function TurntableDemo({ frame }: DemoProps) {
  return (
    <Group>
      <Turntable x={10} y={40} speed={0.4} />
      <Text x={30} y={420} style={mono(18, C.grey)}>{'<Turntable speed={0.4} />'}</Text>
      <Text x={30} y={452} style={mono(18, C.dim)}>640 × 360 · a 3D scene from a plugin process</Text>
      <Group x={680} y={60}>
        {PARTS.map(([key, value], i) => (
          <Group key={key} y={i * 88} opacity={rise(frame, 12 + i * 8)}>
            <Rect width={4} height={62} fill={C.orange} />
            <Text x={18} y={4} style={mono(22, C.paper, 700)}>{key}</Text>
            <Text x={18} y={36} maxWidth={290} style={mono(16, C.grey)}>{value}</Text>
          </Group>
        ))}
      </Group>
    </Group>
  );
}
