import { Easings, Group, Rect, Text, progress } from '@celesta.studio/react';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// @celesta.studio/react sinks to become the core, and twelve focused packages rise
// out of it, each with one thing it provides. Underneath, the codemod moves
// an import to its new home.
const PACKAGES = [
  ['shapes', 'Circle, Arrow'], ['shader', 'defineShader'], ['layout', 'Center, Grid'], ['transitions', 'TransitionSeries'],
  ['text', 'TextBox'], ['code', 'highlightCode'], ['math', 'loopNoise'], ['media-utils', 'preloadMedia'],
  ['character', 'Dialogue'], ['voicevox', 'timingFromVoicevox'], ['project', 'ProjectTimeline'], ['debug', 'DebugOverlay'],
] as const;
const GRID = { x: 20, y: 24, columns: 4, width: 196, height: 84, gap: 12 };
const CORE_Y = GRID.y + 3 * (GRID.height + GRID.gap);
const CORE_WIDTH = GRID.columns * GRID.width + (GRID.columns - 1) * GRID.gap;

export function PackagesDemo({ frame }: DemoProps) {
  const sink = progress(frame, 6, 18, Easings.easeInOutCubic);
  const coreY = 180 + (CORE_Y - 180) * sink;
  const coreH = 150 - (150 - GRID.height) * sink;
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      {PACKAGES.map(([name, provides], i) => {
        const p = progress(frame, 20 + i * 2.5, 18, Easings.easeOutBack);
        if (p <= 0) return null;
        const col = i % GRID.columns;
        const row = Math.floor(i / GRID.columns);
        const x = GRID.x + col * (GRID.width + GRID.gap);
        const y = GRID.y + row * (GRID.height + GRID.gap);
        return (
          <Group key={name} x={x} y={CORE_Y + (y - CORE_Y) * p} opacity={Math.min(1, p * 1.5)}>
            <Rect width={GRID.width} height={GRID.height} cornerRadius={10} fill={C.raised} stroke={C.line} strokeWidth={2} />
            <Text x={16} y={26} anchorY={0.5} style={{ ...mono, fontSize: 14, fill: solid(C.dim) }}>@celesta.studio/</Text>
            <Text x={16} y={50} anchorY={0.5} style={{ ...mono, fontSize: 20, fontWeight: 700, fill: solid(C.paper) }}>{name}</Text>
            <Text x={16} y={70} anchorY={0.5} style={{ ...mono, fontSize: 13, fill: solid(C.orange) }}>{provides}</Text>
          </Group>
        );
      })}
      <Group x={GRID.x} y={coreY} opacity={enter(frame, -6, 12)}>
        <Rect width={CORE_WIDTH} height={coreH} cornerRadius={12} fill={C.orange} />
        <Text x={CORE_WIDTH / 2} y={coreH / 2 - 8 * sink} anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.mono, fontSize: 40 - 14 * sink, fontWeight: 700, fill: solid(C.ink) }}>@celesta.studio/react</Text>
        <Text x={CORE_WIDTH / 2} y={coreH / 2 + 22} anchorX={0.5} anchorY={0.5} opacity={sink}
          style={{ fontFamily: FONT.mono, fontSize: 14, fill: solid(C.ink) }}>Composition · Sequence · Text · Instances · Static · Span</Text>
      </Group>
      <Group x={20} y={430} opacity={enter(frame, 56)}>
        <Text anchorY={0.5} y={20} style={{ ...mono, fill: solid(C.dim) }}>$ node skills/celesta/scripts/migrate-packages.mjs .</Text>
        <Rect y={52} width={CORE_WIDTH} height={44} fill="#FF5F561A" />
        <Text x={16} y={74} anchorY={0.5} style={{ ...mono, fill: solid(C.red) }}>{"- import { Circle } from '@celesta.studio/react';"}</Text>
        <Rect y={100} width={CORE_WIDTH} height={44} fill="#3DDC841A" opacity={enter(frame, 64)} />
        <Text x={16} y={122} anchorY={0.5} opacity={enter(frame, 64)} style={{ ...mono, fill: solid(C.green) }}>{"+ import { Circle } from '@celesta.studio/shapes';"}</Text>
      </Group>
    </Group>
  );
}
