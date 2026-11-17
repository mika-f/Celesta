import { Group, Rect, Text } from '@celesta.studio/react';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { Check, mono, rise } from './common';

// MEDIA PLUGINS: what Celesta-export --list-components printed for this
// film's export (FFmpeg and AVFoundation from plugins/dist, and the built-in
// image components), and a file routed through the decoders: the ones
// listing its extension, in order, the first that reads it keeping it.

const COMPONENTS: readonly [string, string, string][] = [
  ['builtin/png', 'videoEncoder', 'built-in'],
  ['builtin/image', 'muxer', 'built-in'],
  ['avfoundation/avfoundation', 'decoder', 'AVFoundation'],
  ['avfoundation/h264', 'videoEncoder', 'H.264 (VideoToolbox)'],
  ['avfoundation/aac', 'audioEncoder', 'AAC (AudioToolbox)'],
  ['ffmpeg/ffmpeg', 'decoder', 'FFmpeg'],
  ['ffmpeg/vp9', 'videoEncoder', 'VP9 (libvpx)'],
  ['ffmpeg/opus', 'audioEncoder', 'Opus'],
  ['ffmpeg/mp4', 'muxer', 'MP4'],
  ['ffmpeg/webm', 'muxer', 'WebM'],
];

export function MediaDemo({ frame }: DemoProps) {
  const lit = (i: number) => rise(frame, 4 + i * 3, 6);
  // Both list .mp4; FFmpeg's priority 1 puts it first.
  const decoders = ['ffmpeg/ffmpeg', 'avfoundation/avfoundation'];
  const asking = Math.floor((frame - 60) / 16);
  return (
    <Group>
      <Text x={30} y={26} style={mono(17, C.dim)}>$ Celesta-export --list-components</Text>
      {COMPONENTS.map(([id, role, name], i) => {
        const decoder = role === 'decoder';
        return (
          <Group key={id} x={30} y={64 + i * 40} opacity={lit(i)}>
            <Text style={mono(18, decoder ? C.blue : C.paper, decoder ? 700 : 400)}>{id}</Text>
            <Text x={300} style={mono(16, C.grey)}>{role}</Text>
            <Text x={440} style={mono(16, C.dim)}>{name}</Text>
          </Group>
        );
      })}
      <Group x={690} y={60} opacity={rise(frame, 44)}>
        <Rect width={280} height={56} fill={C.raised} stroke={C.line} />
        <Text x={16} y={28} anchorY={0.5} style={mono(18, C.paper, 700)}>footage.mp4</Text>
        <Text x={264} y={28} anchorX={1} anchorY={0.5} style={mono(14, C.dim)}>.mp4</Text>
        {decoders.map((id, n) => {
          const asked = n === 0 && asking >= 0;
          const kept = n === 0 && asking >= 0;
          return (
            <Group key={id} y={110 + n * 90} opacity={asked ? 1 : 0.35}>
              <Rect width={280} height={64} fill={kept ? '#5AA9FF22' : C.ink} stroke={kept ? C.blue : C.line} strokeWidth={kept ? 2 : 1} />
              <Text x={16} y={22} style={mono(16, C.paper, 700)}>{id.split('/')[0]}</Text>
              <Text x={16} y={44} style={mono(14, C.dim)}>{n === 0 ? 'lists .mp4 · priority 1' : 'lists .mp4 · only if FFmpeg fails'}</Text>
              <Check x={248} y={32} p={kept ? rise(frame, 66, 10) : 0} color={C.blue} />
            </Group>
          );
        })}
        <Text y={330} maxWidth={290} style={mono(15, C.grey)}>the first decoder that reads the file keeps it</Text>
        <Text y={380} maxWidth={290} style={mono(15, C.grey)}>each plugin a process of its own</Text>
      </Group>
    </Group>
  );
}
