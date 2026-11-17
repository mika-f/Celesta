import { Group, Rect, Text } from '@celesta.studio/react';
import { Path } from '@celesta.studio/shapes';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { mono } from './common';

// TIMELINE: the timeline and transport after P7c, drawn after its handoff:
// one timecode, under the viewer; a thinner ruler with full timecodes and
// ticks at three heights; fixed lanes (video 4rem, audio 5rem) with a name
// strip on each clip; icon buttons for Mark In, Mark Out and zoom to fit.
// The film's own first chapters are the clips.

const FPS = 30;
const PX = 2.2; // pixels a frame
const HEADER = 150;
const timecode = (f: number) => {
  const s = Math.floor(f / FPS);
  return `00:00:${String(s).padStart(2, '0')}:${String(f % FPS).padStart(2, '0')}`;
};

const CLIPS: readonly [string, number, number, string][] = [
  ['PATCH', 0, 240, C.grey],
  ['CONTENT PLUGINS', 240, 120, C.orange],
  ['3D SCENES', 360, 120, C.orange],
];

export function TimelineDemo({ frame }: DemoProps) {
  const playhead = 150 + frame * 1.5;
  const x = (f: number) => HEADER + (f - 120) * PX;
  return (
    <Group>
      <Rect width={1000} height={563} fill={C.ink} />
      <Group x={500} y={22}>
        <Text anchorX={0.5} style={mono(26, C.paper, 700)}>{timecode(Math.floor(playhead))}</Text>
      </Group>
      <Group x={360} y={66}>
        {[0, 1, 2, 3].map((i) => <Rect key={i} x={i * 70} width={52} height={36} fill={i === 1 ? C.orange : C.raised} stroke={C.line} />)}
        {/* Mark In (ArrowRightFromLine), Play, Mark Out (ArrowRightToLine), zoom to fit (UnfoldHorizontal). */}
        <Path points={[[16, 9], [16, 27]]} stroke={C.grey} strokeWidth={2} cap="round" />
        <Path points={[[22, 18], [38, 18]]} stroke={C.grey} strokeWidth={2} cap="round" />
        <Path points={[[32, 12], [38, 18], [32, 24]]} stroke={C.grey} strokeWidth={2} cap="round" join="round" />
        <Path points={[[89, 8], [89, 28], [105, 18]]} closed fill={C.ink} />
        <Path points={[[154, 18], [170, 18]]} stroke={C.grey} strokeWidth={2} cap="round" />
        <Path points={[[164, 12], [170, 18], [164, 24]]} stroke={C.grey} strokeWidth={2} cap="round" join="round" />
        <Path points={[[176, 9], [176, 27]]} stroke={C.grey} strokeWidth={2} cap="round" />
        <Path points={[[226, 18], [246, 18]]} stroke={C.grey} strokeWidth={2} cap="round" />
        <Path points={[[231, 13], [226, 18], [231, 23]]} stroke={C.grey} strokeWidth={2} cap="round" join="round" />
        <Path points={[[241, 13], [246, 18], [241, 23]]} stroke={C.grey} strokeWidth={2} cap="round" join="round" />
        <Path points={[[236, 8], [236, 11]]} stroke={C.grey} strokeWidth={2} cap="round" />
        <Path points={[[236, 25], [236, 28]]} stroke={C.grey} strokeWidth={2} cap="round" />
      </Group>
      <Group y={126}>
        <Rect width={1000} height={36} fill={C.panel} />
        {Array.from({ length: 60 }, (_, i) => {
          const f = 120 + i * 6;
          const major = f % 60 === 0;
          const mid = f % 30 === 0;
          return (
            <Group key={i} x={x(f)}>
              <Rect y={major ? 0 : mid ? 18 : 28} width={1} height={major ? 36 : mid ? 18 : 8} fill={major ? C.grey : C.dim} />
              {major ? <Text x={6} y={4} style={mono(13, C.grey)}>{timecode(f)}</Text> : null}
            </Group>
          );
        })}
        <Rect width={HEADER} height={36} fill={C.panel} />
        <Rect y={36} width={1000} height={1} fill={C.line} />
      </Group>
      {[['Video 1', 52, 163], ['Video 2', 52, 216], ['Score', 65, 269]].map(([name, h, y]) => (
        <Group key={name as string} y={y as number}>
          <Rect width={1000} height={(h as number) - 1} fill={C.panel} />
          <Rect y={(h as number) - 1} width={1000} height={1} fill={C.line} />
          <Text x={14} y={14} style={mono(14, C.grey)}>{name}</Text>
        </Group>
      ))}
      <Group clip={{ x: HEADER, y: 160, width: 1000 - HEADER, height: 200 }}>
        {CLIPS.map(([name, from, length, color]) => (
          <Group key={name} x={x(from)} y={168}>
            <Rect width={length * PX - 2} height={42} fill={`${color}33`} stroke={color} strokeWidth={1} />
            <Rect width={length * PX - 2} height={18} fill={color} />
            <Text x={8} y={9} anchorY={0.5} style={mono(12, C.ink, 700)}>{name}</Text>
          </Group>
        ))}
        <Group x={x(0)} y={274}>
          <Rect width={1600} height={54} fill="#9B87F522" stroke={C.violet} strokeWidth={1} />
          <Rect width={1600} height={18} fill={C.violet} />
          <Text x={8} y={9} anchorY={0.5} style={mono(12, C.ink, 700)}>score-0.10.0.wav</Text>
          {Array.from({ length: 160 }, (_, i) => {
            const h = 6 + 20 * Math.abs(Math.sin(i * 0.9) * Math.cos(i * 0.23));
            return <Rect key={i} x={i * 10} y={36 - h / 2 + 6} width={6} height={h} fill={C.violet} opacity={0.6} />;
          })}
        </Group>
      </Group>
      <Group x={x(playhead)} y={126}>
        <Path points={[[-7, 0], [7, 0], [7, 8], [0, 15], [-7, 8]]} closed fill={C.orange} />
        <Rect x={-1} width={2} height={230} fill={C.orange} />
      </Group>
      <Text x={20} y={400} maxWidth={960} style={mono(17, C.grey)}>One timecode under the viewer, a thinner ruler, lanes at fixed heights.</Text>
      <Text x={20} y={432} maxWidth={960} style={mono(17, C.grey)}>Mark In, Mark Out and zoom to fit as icon buttons.</Text>
    </Group>
  );
}
