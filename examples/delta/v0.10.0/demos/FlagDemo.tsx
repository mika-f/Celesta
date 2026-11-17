import { Group, Rect, Text } from '@celesta.studio/react';
import { Flag } from '@celesta-plugin/flag';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Note, mono, rise } from './common';

// CONTENT PLUGINS: a real <Flag> from examples/plugins/flag. Each frame,
// Celesta asks the plugin's own process for the layer at that time and
// draws the meshes it returns; the right side writes out that exchange.

export const PORTRAIT = '../assets/dialogue-demo/portraits/komugi-normal.png';

export function FlagDemo({ frame }: DemoProps) {
  const t = frame / 30;
  const rows: [string, string, string][] = [
    ['→', 'frame', `time ${t.toFixed(3)} s`],
    ['', 'assets', 'image  komugi-normal.png'],
    ['', 'data', '{ amplitude, waves }'],
    ['←', 'meshes', '1 mesh · 48 × 12 strips'],
    ['', 'texture', 'image  360 × 400'],
  ];
  return (
    <Group>
      <Rect x={88} y={70} width={10} height={450} fill="#8A6A4A" />
      <Rect x={84} y={60} width={18} height={14} cornerRadius={4} fill={C.grey} />
      <Flag x={98} y={50} scale={1.0} src={PORTRAIT} amplitude={0.1} waves={1.6} speed={0.9} shading={0.45} />
      <Group x={540} y={60}>
        <Rect width={420} height={64} fill={C.raised} stroke={C.line} />
        <Text x={20} y={32} anchorY={0.5} style={mono(22, C.paper, 700)}>Celesta</Text>
        <Text x={400} y={32} anchorX={1} anchorY={0.5} style={mono(16, C.dim)}>renderer</Text>
        <Group y={310}>
          <Rect width={420} height={64} fill={C.raised} stroke={C.orange} strokeWidth={2} />
          <Text x={20} y={32} anchorY={0.5} style={mono(20, C.orange, 700)}>celesta-plugin-flag</Text>
          <Text x={400} y={32} anchorX={1} anchorY={0.5} style={mono(16, C.dim)}>its own process</Text>
        </Group>
        {rows.map(([dir, key, value], i) => (
          <Group key={key} x={20} y={92 + i * 40} opacity={rise(frame, 10 + i * 5)}>
            <Text style={mono(19, dir === '←' ? C.green : C.orange, 700)}>{dir}</Text>
            <Text x={30} style={mono(19, C.grey)}>{key}</Text>
            <Text x={130} style={mono(19, C.paper)}>{value}</Text>
          </Group>
        ))}
      </Group>
      <Note x={540} y={520}>{'<Flag src={portrait} amplitude={0.1} />'}</Note>
    </Group>
  );
}
