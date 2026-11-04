import { Group, Rect, Text } from '@celesta.studio/react';
import type { BlendMode } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { BEAT } from '../timeline';

// Every blend mode at once: the same backdrop, the same two discs in each
// mode. The four non-separable modes (hue to luminosity) light up last.
const MODES: readonly BlendMode[] = [
  'normal', 'multiply', 'screen', 'overlay', 'add', 'difference',
  'darken', 'lighten', 'colorDodge', 'colorBurn', 'hardLight', 'softLight',
  'exclusion', 'hue', 'saturation', 'color', 'luminosity',
];
const COLS = 6;
const CELL = { width: 132, height: 132, gap: 13 };
const NON_SEPARABLE = new Set<BlendMode>(['hue', 'saturation', 'color', 'luminosity']);

function Cell({ mode, frame, lit }: { mode: BlendMode; frame: number; lit: boolean }) {
  const a = (frame / 30) * Math.PI;
  const { width, height } = CELL;
  return (
    <Group>
      {/* Each cell's opaque backdrop is all its discs blend with. */}
      <Group clip={{ width, height, cornerRadius: 8 }}>
        <Rect width={width} height={height} fill={{
          type: 'linear', start: { x: 0, y: 0 }, end: { x: width, y: height },
          stops: [{ offset: 0, color: '#F26A2E' }, { offset: 0.5, color: '#5AA9FF' }, { offset: 1, color: '#16181D' }],
        }} />
        <Rect y={height * 0.62} width={width} height={height * 0.38} fill="#EFECE4" />
        <Circle x={width / 2 + 22 * Math.cos(a)} y={height / 2 + 14 * Math.sin(a)} anchorX={0.5} anchorY={0.5}
          radius={40} fill="#3DDC84" blendMode={mode} />
        <Circle x={width / 2 - 22 * Math.cos(a)} y={height / 2 - 14 * Math.sin(a)} anchorX={0.5} anchorY={0.5}
          radius={34} fill="#9B87F5" blendMode={mode} />
      </Group>
      {lit ? <Rect x={-3} y={-3} width={width + 6} height={height + 6} cornerRadius={10} stroke={C.orange} strokeWidth={3} /> : null}
      <Text y={height + 20} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 16, fontWeight: lit ? 700 : 400, fill: solid(lit ? C.orange : C.grey) }}>
        {mode}
      </Text>
    </Group>
  );
}

export function BlendDemo({ frame }: DemoProps) {
  const lit = frame >= 10 * BEAT;
  return (
    <Group x={(STAGE.width - (COLS * CELL.width + (COLS - 1) * CELL.gap)) / 2} y={18}>
      {MODES.map((mode, i) => {
        const col = i % COLS;
        const row = Math.floor(i / COLS);
        const p = enter(frame, i * 2.5, 12);
        return (
          <Group key={mode} x={col * (CELL.width + CELL.gap)} y={row * (CELL.height + 50)} opacity={p}>
            <Cell mode={mode} frame={frame} lit={lit && NON_SEPARABLE.has(mode)} />
          </Group>
        );
      })}
      <Group y={3 * (CELL.height + 50) + 26} opacity={enter(frame, 10 * BEAT)}>
        <Text style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.paper) }}>hue · saturation · color · luminosity</Text>
        <Text y={34} style={{ fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) }}>non-separable: they mix whole colors,</Text>
        <Text y={60} style={{ fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) }}>and PSD layers in these modes now draw as such</Text>
      </Group>
    </Group>
  );
}
