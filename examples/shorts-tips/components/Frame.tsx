import { Group, Rect } from '@celesta/react';
import { C, DEMO, H, SAFE, W } from '../constants';
import { DEMOS, type TipId } from '../demos';
import { DemoClock } from './DemoClock';

export function Background({ accent }: { accent: string }) {
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Rect width={W} height={H}
        fill={{ type: 'radial', center: { x: 120, y: 160 }, radius: 1100,
          stops: [{ offset: 0, color: `${accent}30` }, { offset: 1, color: `${accent}00` }] }} />
    </>
  );
}

// The lower half, with the variant's demo running in it.
export function DemoPanel({ tip, frame, stageAt, accent }: {
  tip: TipId;
  frame: number;
  stageAt: readonly number[];
  accent: string;
}) {
  const { Demo } = DEMOS[tip];
  return (
    <Group y={DEMO.y} clip={{ width: W, height: DEMO.height }}>
      <Rect width={W} height={DEMO.height}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: DEMO.height },
          stops: [{ offset: 0, color: C.stage }, { offset: 1, color: C.ink }] }} />
      <Rect width={W} height={2} fill={C.edge} />
      <DemoClock frame={frame} stageAt={stageAt} accent={accent}>
        <Demo />
      </DemoClock>
    </Group>
  );
}

// `guides: true` tints where the platforms' own UI covers the picture. A
// checking aid only: it is off by default and never part of a delivered video.
export function SafeGuides() {
  const tint = '#FF2D5540';
  return (
    <>
      <Rect y={SAFE.bottom} width={W} height={H - SAFE.bottom} fill={tint} />
      <Rect x={SAFE.right} width={W - SAFE.right} height={SAFE.bottom} fill={tint} />
      <Rect width={SAFE.right} height={SAFE.top} fill={tint} />
      <Rect y={SAFE.top} width={SAFE.left} height={SAFE.bottom - SAFE.top} fill={tint} />
    </>
  );
}
