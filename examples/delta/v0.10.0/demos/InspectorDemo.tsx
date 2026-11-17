import { Easings, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import { Panel } from '@celesta.studio/components';
import { Path } from '@celesta.studio/shapes';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { PointerPath, mono } from './common';

// INSPECTOR: the redesigned Inspector, drawn after the P7b handoff:
// collapsible sections with a band header, rows in three lanes (the label,
// the value, a trailing lane for its unit), and values dragged by their
// label. The card on the canvas is a real Panel at the values shown.

const DRAG = [36, 86] as const;

export function InspectorDemo({ frame }: DemoProps) {
  const x = Math.round(interpolate(frame, [...DRAG], [120, 330], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }));
  const dragging = frame >= DRAG[0] - 2 && frame < DRAG[1] + 2;
  const rows: [string, string, string][] = [['x', String(x), 'px'], ['y', '150', 'px'], ['rotation', '-4', '°'], ['scale', '1', ''], ['opacity', '1', '']];
  const fill: [string, string, string][] = [['fill', '#F26A2E', ''], ['cornerRadius', '10', 'px']];
  const lane = { label: 26, value: 180, unit: 380 };
  const section = (title: string, y: number, list: [string, string, string][], active = -1) => (
    <Group y={y}>
      <Rect width={410} height={34} fill={C.raised} />
      <Rect y={34} width={410} height={1} fill={C.line} />
      <Path points={[[14, 13], [19, 19], [24, 13]]} stroke={C.grey} strokeWidth={2} cap="round" join="round" />
      <Text x={36} y={17} anchorY={0.5} style={mono(15, C.paper, 700)}>{title}</Text>
      {list.map(([label, value, unit], i) => (
        <Group key={label} y={35 + i * 34}>
          {i === active && dragging ? <Rect width={410} height={34} fill="#F26A2E1C" /> : null}
          <Text x={lane.label} y={17} anchorY={0.5} style={mono(15, i === active && dragging ? C.orange : C.grey)}>{label}</Text>
          <Rect x={lane.value - 8} y={4} width={186} height={26} fill={C.ink} stroke={i === active && dragging ? C.orange : C.line} />
          <Text x={lane.value} y={17} anchorY={0.5} style={mono(15, C.paper)}>{value}</Text>
          <Text x={lane.unit} y={17} anchorY={0.5} style={mono(15, C.dim)}>{unit}</Text>
        </Group>
      ))}
    </Group>
  );
  return (
    <Group>
      <Rect width={560} height={563} fill={C.ink} />
      <Text x={16} y={14} style={mono(15, C.dim)}>Canvas</Text>
      <Panel x={x} y={150} width={240} height={150} rotation={-4} fill={C.orange} cornerRadius={10}>
        <Text x={20} y={20} style={mono(22, C.ink, 700)}>PATCH</Text>
        <Text x={20} y={96} style={mono(16, C.ink)}>{`x={${x}}`}</Text>
      </Panel>
      <Group x={575}>
        <Rect width={425} height={563} fill={C.panel} />
        <Rect width={425} height={44} fill={C.ink} />
        <Text x={14} y={22} anchorY={0.5} style={mono(16, C.paper, 700)}>Inspector</Text>
        <Text x={410} y={22} anchorX={1} anchorY={0.5} style={mono(14, C.dim)}>Panel · Card.tsx:12</Text>
        <Group x={8} y={56}>
          {section('Transform', 0, rows, 0)}
          {section('Panel', 220, fill)}
        </Group>
      </Group>
      <PointerPath f={frame} keys={[[0, 420, 500], [DRAG[0] - 6, 612, 106], [DRAG[0], 612, 106], [DRAG[1], 780, 106], [119, 800, 160]]}
        presses={[[DRAG[0] - 2, DRAG[1] + 2]]} />
    </Group>
  );
}
