import { Group, Rect, Text } from '@celesta.studio/react';
import { Delta } from '../../components/Delta';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Button, PointerPath, mono, rise } from './common';

// EXPORT PAGE: the export page after P7d, drawn after its handoff: the
// settings take the left slot, beside the viewer and the timeline, and
// components read "{name} · {plugin name}". The values are this film's
// export; its frame count is the film's.

const CLICK = 44;
const TOTAL = 2520;

export function ExportDemo({ frame }: DemoProps) {
  const running = frame >= CLICK;
  const done = Math.min(TOTAL, Math.max(0, Math.round(((frame - CLICK) / 66) * TOTAL)));
  const rows: [string, string][] = [
    ['Preset', 'Default'],
    ['Format', 'MP4 · FFmpeg'],
    ['Video', 'H.264 (VideoToolbox) · AVFoundation'],
    ['Audio', 'AAC · FFmpeg'],
    ['Loudness', 'Streaming  -14 LUFS'],
    ['Range', `0 – ${TOTAL - 1}`],
  ];
  return (
    <Group>
      <Rect width={1000} height={563} fill={C.ink} />
      <Group>
        <Rect width={430} height={563} fill={C.panel} />
        <Rect x={430} width={1} height={563} fill={C.line} />
        <Text x={18} y={18} style={mono(17, C.paper, 700)}>Export</Text>
        {rows.map(([label, value], i) => (
          <Group key={label} y={60 + i * 50} opacity={rise(frame, 2 + i * 3, 6)}>
            <Text x={18} y={16} anchorY={0.5} style={mono(15, C.grey)}>{label}</Text>
            <Rect x={110} width={300} height={32} fill={C.ink} stroke={C.line} />
            <Text x={122} y={16} anchorY={0.5} maxWidth={280} style={mono(14, C.paper)}>{value}</Text>
          </Group>
        ))}
        <Text x={18} y={384} style={mono(14, C.dim)}>Save as preset…</Text>
        <Button x={18} y={420} label={running ? (done >= TOTAL ? 'Done' : 'Exporting…') : 'Export'} primary pressed={running && done < TOTAL} width={392} />
        {running ? (
          <Group x={18} y={472}>
            <Rect width={392} height={6} fill={C.line} />
            <Rect width={(392 * done) / TOTAL} height={6} fill={done >= TOTAL ? C.green : C.orange} />
            <Text y={18} style={mono(14, C.grey)}>{`${done} / ${TOTAL} frames`}</Text>
          </Group>
        ) : null}
      </Group>
      <Group x={450} y={20}>
        <Text style={mono(14, C.dim)}>Viewer</Text>
        <Rect y={24} width={530} height={298} fill="#000000" stroke={C.line} />
        <Group x={265} y={173}>
          <Delta size={90} fill={C.orange} rotation={frame * 3} />
        </Group>
        <Text x={265} y={340} anchorX={0.5} style={mono(18, C.paper, 700)}>{`00:00:${String(Math.floor(done / 30) % 60).padStart(2, '0')}:${String(done % 30).padStart(2, '0')}`}</Text>
      </Group>
      <Group x={450} y={400}>
        <Rect width={530} height={140} fill={C.panel} stroke={C.line} />
        {[0, 1, 2].map((lane) => <Rect key={lane} x={0} y={14 + lane * 40} width={530 * (0.5 + 0.18 * lane)} height={30} fill={['#F26A2E44', '#5AA9FF44', '#9B87F544'][lane]} />)}
        <Rect x={(530 * done) / TOTAL} width={2} height={140} fill={C.orange} />
      </Group>
      <PointerPath f={frame} keys={[[0, 700, 520], [CLICK - 8, 200, 436], [CLICK + 20, 210, 440], [119, 380, 520]]} presses={[[CLICK - 4, CLICK]]} />
    </Group>
  );
}
