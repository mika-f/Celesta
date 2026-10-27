import { Easings, Group, Rect, Text, interpolate } from '@celesta/react';
import { Panel } from '@celesta/components';
import { C, FONT, solid } from '../../components/theme';
import { BEFORE_LUFS, MEASURED } from '../loudness';
import type { DemoProps } from '../program';
import { BAR } from '../timeline';
import { mono } from './common';

// LOUDNESS: this film's own numbers. Every video export now measures its
// mix (ITU-R BS.1770 integrated loudness and the true peak), and
// --loudness streaming brings it to -14 LUFS with one gain (#479); the
// editor offers the same targets next to Export… (#480). The values are the
// export's report for this film (loudness.ts); the terminal and the picker
// are drawn.

const SCALE = { from: -36, to: -6 };
const BAR_W = 900;
const lx = (lufs: number) => ((lufs - SCALE.from) / (SCALE.to - SCALE.from)) * BAR_W;
const fmt = (v: number, digits = 1) => `${v > 0 ? '+' : v < 0 ? '−' : ''}${Math.abs(v).toFixed(digits)}`;

const COMMAND = '$ Celesta-export --json --loudness streaming --react film-0.9.0.tsx delta-0.9.0.mp4';
/** A number as the exporter's JSON writes it: whole numbers keep their ".0". */
const json = (v: number) => (Number.isInteger(v) ? v.toFixed(1) : String(v));
const REPORT = `{"event":"wrote","output":{"type":"video", … ,"loudness":{"integratedLufs":${json(MEASURED.integratedLufs)},`
  + `"truePeakDbtp":${json(MEASURED.truePeakDbtp)},"gainDb":${json(MEASURED.gainDb)}}}}`;

export function LoudnessDemo({ frame }: DemoProps) {
  const move = interpolate(frame, [BAR * 1.5, BAR * 1.5 + 22], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const lufs = BEFORE_LUFS + (MEASURED.integratedLufs - BEFORE_LUFS) * move;
  const typed = Math.max(0, Math.floor((frame - 4) * 3.2));
  const reported = Math.max(0, Math.floor((frame - BAR * 2.5) * 6));
  return (
    <Group>
      <Group x={50} y={70}>
        <Text style={mono(17, C.grey)}>integrated loudness, LUFS</Text>
        <Rect y={40} width={BAR_W} height={56} fill="raised" stroke="line" strokeWidth={2} />
        <Rect x={2} y={42} width={Math.max(0, lx(lufs) - 2)} height={52} fill={move > 0.5 ? 'orange' : 'grey'} />
        {Array.from({ length: (SCALE.to - SCALE.from) / 3 + 1 }, (_, i) => SCALE.from + i * 3).map((v) => (
          <Group key={v} x={lx(v)}>
            <Rect y={100} width={1} height={10} fill="dim" />
            <Text y={116} anchorX={0.5} style={mono(14, C.dim)}>{String(v)}</Text>
          </Group>
        ))}
        {[[-23, 'broadcast'], [-14, 'streaming']].map(([v, name]) => (
          <Group key={name} x={lx(v as number)}>
            <Rect x={-1} y={30} width={3} height={76} fill="paper" />
            <Text y={22} anchorX={0.5} style={mono(15, C.paper, 700)}>{`${name} ${v}`}</Text>
          </Group>
        ))}
        <Text y={170} style={{ fontFamily: FONT.mono, fontSize: 54, fontWeight: 700, fill: solid(move > 0.5 ? C.orange : C.paper) }}>
          {`${fmt(lufs)} LUFS`}
        </Text>
        <Text x={420} y={180} style={mono(20, C.grey)}>{`gain ${fmt(MEASURED.gainDb * move)} dB`}</Text>
        <Text x={680} y={180} style={mono(20, C.grey)}>{`true peak ${fmt(MEASURED.truePeakDbtp)} dBTP`}</Text>
      </Group>

      <Panel x={50} y={330} width={600} height={190} fill="#08090B" stroke="line" strokeWidth={2} cornerRadius={6} padding={16}>
        <Text maxWidth={568} style={{ ...mono(15, C.paper), lineHeight: 22, visibleCharacters: typed }}>{COMMAND}</Text>
        <Text y={56} maxWidth={568} style={{ ...mono(14, C.green), lineHeight: 21, visibleCharacters: reported }}>{REPORT}</Text>
      </Panel>

      <Panel x={680} y={330} width={270} height={190} fill="raised" stroke="line" strokeWidth={2} cornerRadius={6} padding={16}>
        <Text style={mono(15, C.grey, 700)}>Export…</Text>
        <Text y={40} style={mono(14, C.dim)}>Loudness</Text>
        {['Off', 'Streaming  −14', 'Broadcast  −23'].map((label, i) => {
          const on = i === (frame >= BAR ? 1 : 0);
          return (
            <Group key={label} y={66 + i * 34}>
              <Rect width={238} height={28} cornerRadius={3} fill={on ? 'orange' : 'panel'} stroke={on ? 'orange' : 'line'} strokeWidth={1} />
              <Text x={10} y={14} anchorY={0.5} style={mono(14, on ? C.ink : C.grey, on ? 700 : 400)}>{label}</Text>
            </Group>
          );
        })}
      </Panel>
    </Group>
  );
}
