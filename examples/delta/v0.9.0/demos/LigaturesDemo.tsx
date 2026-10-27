import { Group, Rect, Span, Text } from '@celesta/react';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { mono } from './common';

// LIGATURES: fontFeatures turns OpenType features on and off (#459), shaped
// the same when measured and drawn. JetBrains Mono joins === and -> through
// its contextual alternates; the top line switches them every two beats,
// the second keeps them off but turns them back on inside one <Span>
// (merged tag by tag, so the span sets both). Below,
// Space Grotesk's figures counting, proportional against tabular (tnum).

const CODE = 'x => x !== 0 && x >= -1';
const FEATURES_OFF = { calt: false, liga: false };
const FEATURES_ON = { calt: true, liga: true };

/** The labels are code too: drawn as written. */
const label = (color: string) => ({ ...mono(17, color), fontFeatures: FEATURES_OFF });

export function LigaturesDemo({ frame }: DemoProps) {
  const on = Math.floor(Math.max(0, frame) / BAR) % 2 === 1;
  const code = { fontFamily: FONT.mono, fontSize: 60, fill: solid(C.paper) } as const;
  const count = (Math.max(0, frame) * 7919) % 100000 / 10 + 1000;
  const figures = count.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const num = { fontFamily: FONT.sans, fontSize: 76, fontWeight: 700, fill: solid(C.paper) } as const;
  return (
    <Group>
      <Group x={50} y={44}>
        <Text style={label(on ? C.orange : C.grey)}>{on ? 'fontFeatures: { calt: true, liga: true }' : 'fontFeatures: { calt: false, liga: false }'}</Text>
        <Text y={34} style={{ ...code, fontFeatures: on ? FEATURES_ON : FEATURES_OFF }}>{CODE}</Text>
      </Group>
      <Group x={50} y={176}>
        <Text style={label(C.grey)}>{'off; a <Span style={{ fontFeatures: { calt: true, liga: true } }}> turns them on'}</Text>
        <Text y={34} style={{ ...code, fontFeatures: FEATURES_OFF }}>
          {'x => x'}<Span style={{ fontFeatures: FEATURES_ON, fill: solid(C.orange) }}>{' !== 0 && '}</Span>{'x >= -1'}
        </Text>
      </Group>
      <Rect x={50} y={318} width={STAGE.width - 100} height={1} fill="line" />
      <Group x={50} y={350}>
        <Text style={mono(17, C.grey)}>proportional figures</Text>
        <Text x={420} y={34} anchorX={1} style={num}>{figures}</Text>
        <Rect x={420} y={30} width={2} height={96} fill="orange" />
      </Group>
      <Group x={520} y={350}>
        <Text style={label(C.orange)}>{'fontFeatures: { tnum: true }'}</Text>
        <Text x={420} y={34} anchorX={1} style={{ ...num, fontFeatures: { tnum: true } }}>{figures}</Text>
        <Rect x={420} y={30} width={2} height={96} fill="orange" />
      </Group>
      <Text x={50} y={500} style={mono(16, C.dim)}>tabular figures are all one width: the left edge stops jittering</Text>
    </Group>
  );
}
