import { Easings, Group, Rect, Text, interpolate, progress, useCurrentFrame } from '@celesta/react';
import { BAR, C, CODE, FONT, OUTRO_AT, SAFE, SAFE_W, W } from '../constants';
import { DemoPanel } from '../components/Frame';
import type { TipId } from '../demos';
import { getPlan } from '../plan';

// Bars 8–11: the closing line takes the code band's place while the demo
// keeps running underneath.
export function Outro({ tip, closing, number, accent }: { tip: TipId; closing: string; number: number; accent: string }) {
  const frame = useCurrentFrame() + OUTRO_AT;
  const downbeat = BAR * 8;
  const pop = progress(frame, downbeat, 14, Easings.easeOutBack);
  const sign = progress(frame, downbeat + BAR, 12, Easings.easeOutCubic);
  const centre = (SAFE.left + SAFE.right) / 2;
  return (
    <>
      <Group y={CODE.y}>
        <Rect width={W} height={CODE.height} fill={accent} />
        <Text x={centre} y={CODE.height / 2 - 30} anchorX={0.5} anchorY={0.5}
          maxWidth={SAFE_W - 40} scale={interpolate(pop, [0, 1], [0.9, 1])} opacity={Math.min(1, pop * 2)}
          style={{
            fontFamily: FONT.ja, fontSize: 76, fontWeight: 900, lineHeight: 104, align: 'center',
            lineBreak: 'phrase', fill: { type: 'solid', color: C.ink },
          }}>
          {closing}
        </Text>
        <Group y={CODE.height - 64 + 16 * (1 - sign)} opacity={sign}>
          <Text x={centre} anchorX={0.5} anchorY={0.5}
            style={{ fontFamily: FONT.display, fontSize: 28, fontWeight: 800, letterSpacing: 4, fill: { type: 'solid', color: `${C.ink}B0` } }}>
            {`CELESTA TIPS #${String(number).padStart(2, '0')}`}
          </Text>
        </Group>
      </Group>
      <DemoPanel tip={tip} frame={frame} stageAt={getPlan().stageAt} accent={accent} />
    </>
  );
}
