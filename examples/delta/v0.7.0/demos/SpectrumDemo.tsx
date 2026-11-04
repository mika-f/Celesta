import { Group, Instances, Rect, Text } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { BANDS, scoreAt } from '../score';
import { chapterAt } from '../timeline';

// The score you are hearing, analyzed in prepare(): 32 log-spaced bands and
// the loudness at this frame, and below them the last three seconds as a
// scrolling spectrogram, one <Instances> cell per band per frame.
const AT = chapterAt(8);
const BAR_W = STAGE.width / BANDS;
const HISTORY = 90;
const CELL_W = STAGE.width / HISTORY;
const CELL_H = 7;
const COUNT = HISTORY * BANDS;
const xs = new Float32Array(COUNT);
const ys = new Float32Array(COUNT);
const colors = new Uint8Array(COUNT * 4);
for (let i = 0; i < COUNT; i++) {
  xs[i] = (i % HISTORY) * CELL_W;
  ys[i] = (BANDS - 1 - Math.floor(i / HISTORY)) * CELL_H;
}

function heat(level: number, out: Uint8Array, at: number) {
  // ink → orange → paper
  const a = Math.min(1, level * 1.6);
  const b = Math.max(0, level * 1.6 - 1) / 0.6;
  out[at] = Math.round(18 + (242 - 18) * a + (239 - 242) * b);
  out[at + 1] = Math.round(19 + (106 - 19) * a + (236 - 106) * b);
  out[at + 2] = Math.round(23 + (46 - 23) * a + (228 - 46) * b);
  out[at + 3] = 255;
}

export function SpectrumDemo({ frame }: DemoProps) {
  const now = AT + frame;
  const { bands, rms } = scoreAt(now);
  for (let h = 0; h < HISTORY; h++) {
    const past = scoreAt(now - (HISTORY - 1 - h)).bands;
    for (let b = 0; b < BANDS; b++) heat(past[b], colors, (b * HISTORY + h) * 4);
  }
  const mono = { fontFamily: FONT.mono, fontSize: 18 } as const;
  const top = 360;
  return (
    <Group>
      {bands.map((level, i) => {
        const h = 6 + level * (top - 40);
        return (
          <Rect key={i} x={i * BAR_W + 3} y={top} anchorY={1} width={BAR_W - 6} height={h} cornerRadius={3}
            fill={level > 0.8 ? C.orange : level > 0.55 ? C.amber : C.grey} />
        );
      })}
      <Text x={0} y={top + 30} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>20 Hz</Text>
      <Text x={STAGE.width} y={top + 30} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>16 kHz</Text>
      <Text x={STAGE.width / 2} y={top + 30} anchorX={0.5} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>
        {`bands[${BANDS}]   ·   t = ${(now / 30).toFixed(2)} s`}
      </Text>
      <Group y={top + 62}>
        <Text anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.paper) }}>rms</Text>
        <Rect x={60} y={-8} width={STAGE.width - 160} height={16} cornerRadius={8} fill={C.line} />
        <Rect x={60} y={-8} width={(STAGE.width - 160) * Math.min(1, rms * 1.6)} height={16} cornerRadius={8} fill={C.orange} />
        <Text x={STAGE.width} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.paper) }}>{rms.toFixed(3)}</Text>
      </Group>
      <Group y={top + 100}>
        <Instances count={COUNT} x={xs} y={ys} width={CELL_W + 0.5} height={CELL_H - 1} color={colors} />
        <Rect x={STAGE.width - 2} width={2} height={BANDS * CELL_H} fill={C.paper} />
      </Group>
    </Group>
  );
}
