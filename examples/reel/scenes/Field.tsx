import type { ReactNode } from 'react';
import { Easings, Group, Rect, Text, progress, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import { Tag } from '../components/Tag';
import { BAR, C, FPS, H, W } from '../constants';

const COLS = 32;
const ROWS = 18;
const CELL = W / COLS;

export function Field() {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const t = f / FPS;
  const second = f >= BAR * 1.5; // two interfering sources from here on
  const cells: ReactNode[] = [];
  for (let r = 0; r < ROWS; r++) {
    for (let c = 0; c < COLS; c++) {
      const cx = c - (COLS - 1) / 2;
      const cy = r - (ROWS - 1) / 2;
      const d = Math.hypot(cx, cy * 1.1);
      let v: number;
      if (!second) {
        v = 0.5 + 0.5 * Math.sin(d * 0.55 - t * 5);
      } else {
        const a = Math.hypot(cx + 7 * Math.cos(t * 0.9), cy + 3 * Math.sin(t * 1.3));
        const b = Math.hypot(cx - 7 * Math.cos(t * 0.7), cy - 3 * Math.sin(t * 1.1));
        v = 0.5 + 0.25 * (Math.sin(a * 0.8 - t * 6) + Math.sin(b * 0.8 - t * 6));
      }
      const enter = progress(f, d * 1.1, 12, Easings.easeOutBack);
      const exit = 1 - progress(f, durationInFrames - 22 + (12 - d) * 0.9, 8, Easings.easeInCubic);
      const size = (4 + 24 * v * v) * enter * exit;
      if (size < 0.5) continue;
      const hot = v > 0.9;
      cells.push(
        <Rect key={`${r}-${c}`} x={(c + 0.5) * CELL} y={(r + 0.5) * CELL} anchorX={0.5} anchorY={0.5}
          width={size} height={size} rotation={second ? v * 90 : 0}
          fill={hot ? C.accent : C.paper} opacity={hot ? 1 : 0.18 + 0.6 * v} />,
      );
    }
  }

  const textIn = progress(f, 12, 16, Easings.easeOutExpo);
  const textOut = 1 - progress(f, durationInFrames - 20, 10, Easings.easeOutCubic);
  const formula = second ? 'v = ½·(sin(0.8·a − 6t) + sin(0.8·b − 6t))' : 'v = sin(0.55·d − 5t)';
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      {cells}
      <Group x={120} y={H - 380} opacity={textIn * textOut}>
        <Rect x={-40} y={-50} width={900} height={330} fill={C.ink} opacity={0.88} />
        <Tag x={0} y={0} n="02" name="DETERMINISTIC" />
        <Text x={0} y={48 + 30 * (1 - textIn)} fontFamily="display" fontSize={104} fontWeight={700} color="paper" style={{ lineHeight: 108 }}>{'Every frame,\na pure function.'}</Text>
      </Group>
      <Group x={W - 120} y={120} opacity={textIn * textOut}>
        <Rect x={-620} y={-40} width={660} height={190} fill={C.ink} opacity={0.88} />
        <Text x={0} y={0} fontFamily="mono" fontSize={24} fontWeight={400} color="grey" anchorX={1} anchorY={0.5}>{formula}</Text>
        <Text x={0} y={50} fontFamily="mono" fontSize={24} fontWeight={400} color="paper" anchorX={1} anchorY={0.5}>
          {`t = ${t.toFixed(3)} s`}
        </Text>
        <Text x={0} y={100} fontFamily="mono" fontSize={24} fontWeight={700} color="accent" anchorY={0.5} anchorX={1}>
          {`frame ${String(f).padStart(3, '0')} → same pixels, every time`}
        </Text>
      </Group>
    </>
  );
}
