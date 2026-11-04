import { Group, Rect, Text } from '@celesta.studio/react';
import { loopNoise } from '@celesta.studio/math';
import { Path } from '@celesta.studio/shapes';
import type { PathCommand } from '@celesta.studio/react';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// A ring of bars driven by loopNoise over this chapter's own 120 frames: the
// motion at frame 120 is the motion at frame 0, which inspect.mjs --loop
// checks frame by frame.
const LOOP = 120;
const BARS = 72;
const CX = STAGE.width / 2;
const CY = 250;

const TERMINAL: { at: number; text: string; color: string }[] = [
  { at: 44, text: 'frame 120 vs frame 0: same Scene', color: C.grey },
  { at: 50, text: 'frame 121 vs frame 1: same Scene', color: C.grey },
  { at: 62, text: 'OK: the last frame runs into the first', color: C.green },
];
const COMMAND = '$ node inspect.mjs film.tsx --loop';

export function LoopDemo({ frame }: DemoProps) {
  const commands: PathCommand[] = [];
  for (let i = 0; i < BARS; i++) {
    const a = (i / BARS) * Math.PI * 2 - Math.PI / 2;
    const n = loopNoise(`loop-bar-${i}`, frame / 20, LOOP / 20);
    const inner = 150;
    const outer = inner + 22 + 70 * (0.5 + 0.5 * n);
    commands.push({ type: 'moveTo', x: CX + Math.cos(a) * inner, y: CY + Math.sin(a) * inner });
    commands.push({ type: 'lineTo', x: CX + Math.cos(a) * outer, y: CY + Math.sin(a) * outer });
  }
  const loopFrame = ((frame % LOOP) + LOOP) % LOOP;
  const head = (loopFrame / LOOP) * Math.PI * 2 - Math.PI / 2;
  const mono = { fontFamily: FONT.mono, fontSize: 20 };
  const typed = Math.max(0, Math.floor((frame - 6) * 1.4));
  const appear = enter(frame, -6, 16);
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group opacity={appear}>
        <Path commands={commands} stroke={C.orange} strokeWidth={5} cap="round" />
        <Rect x={CX + Math.cos(head) * 118} y={CY + Math.sin(head) * 118} anchorX={0.5} anchorY={0.5}
          width={16} height={16} cornerRadius={8} fill={C.paper} />
        <Text x={CX} y={CY - 6} anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.display, fontSize: 96, fill: solid(C.paper) }}>
          {String(loopFrame).padStart(3, '0')}
        </Text>
        <Text x={CX} y={CY + 52} anchorX={0.5} anchorY={0.5} style={{ ...mono, fontSize: 18, fill: solid(C.dim) }}>
          {`of ${LOOP}  ·  loopNoise`}
        </Text>
      </Group>
      <Group x={30} y={480} opacity={enter(frame, 0)}>
        <Rect width={STAGE.width - 60} height={232} cornerRadius={10} fill={C.raised} stroke={C.line} strokeWidth={2} />
        <Text x={24} y={40} anchorY={0.5} style={{ ...mono, fill: solid(C.paper), visibleCharacters: typed }}>{COMMAND}</Text>
        {TERMINAL.map((line, i) => frame >= line.at ? (
          <Text key={i} x={24} y={92 + i * 40 + (i === 2 ? 14 : 0)} anchorY={0.5}
            style={{ ...mono, fontWeight: i === 2 ? 700 : 400, fill: solid(line.color) }}>{line.text}</Text>
        ) : null)}
      </Group>
    </Group>
  );
}
