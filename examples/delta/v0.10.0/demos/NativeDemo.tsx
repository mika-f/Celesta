import { Group, Rect, Text } from '@celesta.studio/react';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { mono, rise } from './common';

// NATIVE CODECS: the two system plugins side by side, their components as
// HANDOFF.md lists them, and what this film's own export used on macOS (the
// encoders the export's JSON report named).

const SYSTEMS: readonly { os: string; plugin: string; lib: string; rows: readonly [string, string][] }[] = [
  {
    os: 'macOS', plugin: 'avfoundation', lib: 'VideoToolbox · AudioToolbox',
    rows: [['h264', 'video'], ['hevc', 'video'], ['prores', 'video'], ['aac', 'audio'], ['mp4 · mov', 'muxers'], ['avfoundation', 'decoder']],
  },
  {
    os: 'Windows', plugin: 'mediafoundation', lib: 'Media Foundation MFTs',
    rows: [['h264', 'video · hardware first'], ['hevc', 'video · hardware first'], ['aac', 'audio'], ['mediafoundation', 'decoder'], ['→ ffmpeg/mp4', 'muxer']],
  },
];

export function NativeDemo({ frame }: DemoProps) {
  return (
    <Group>
      {SYSTEMS.map((system, s) => (
        <Group key={system.os} x={30 + s * 485} y={26}>
          <Text style={mono(26, C.paper, 700)}>{system.os}</Text>
          <Text y={38} style={mono(17, C.blue, 700)}>{system.plugin}</Text>
          <Text y={64} style={mono(15, C.dim)}>{system.lib}</Text>
          {system.rows.map(([id, kind], i) => {
            const p = rise(frame, 8 + s * 4 + i * 6, 6);
            return (
              <Group key={id} y={104 + i * 46} opacity={p}>
                <Rect width={455} height={38} fill={i === 0 ? '#5AA9FF1E' : C.ink} stroke={i === 0 ? C.blue : C.line} strokeWidth={1} />
                <Text x={14} y={19} anchorY={0.5} style={mono(18, C.paper, 700)}>{id}</Text>
                <Text x={440} y={19} anchorX={1} anchorY={0.5} style={mono(15, C.grey)}>{kind}</Text>
              </Group>
            );
          })}
        </Group>
      ))}
      <Group x={30} y={440} opacity={rise(frame, 56, 10)}>
        <Rect width={940} height={92} fill={C.raised} stroke={C.line} />
        <Text x={18} y={16} style={mono(16, C.dim)}>this film, exported on macOS</Text>
        <Text x={18} y={48} style={mono(22, C.paper, 700)}>delta-0.10.0.mp4</Text>
        <Text x={922} y={48} anchorX={1} style={mono(20, C.blue, 700)}>avfoundation/h264 · ffmpeg/aac</Text>
      </Group>
    </Group>
  );
}
