import { Easings, Group, Rect, Text, interpolate, useBeat, useCurrentFrame } from '@celesta/react';
import { BPM, C, CODE, FONT, SAFE, W } from '../constants';
import { CodePanel } from '../components/CodePanel';
import { SETTLED } from '../components/DemoClock';
import { DemoPanel } from '../components/Frame';
import type { TipId } from '../demos';

// Bar 0: the finished result, from the very first frame. The whole snippet,
// the demo already running, and the hook line across the code.
export function Hook({ tip, hook, accent, stages }: { tip: TipId; hook: string; accent: string; stages: number }) {
  const frame = useCurrentFrame();
  return (
    <>
      <CodePanel accent={accent} />
      <DemoPanel tip={tip} frame={frame} stageAt={new Array(stages).fill(SETTLED)} accent={accent} />
      <HookRibbon text={hook} accent={accent} />
    </>
  );
}

function HookRibbon({ text, accent }: { text: string; accent: string }) {
  const frame = useCurrentFrame();
  const { pulse } = useBeat({ bpm: BPM });
  // Lands on frame 0 already readable, then settles from a slight zoom.
  const settle = interpolate(frame, [0, 8], [1.12, 1], { easing: Easings.easeOutBack, extrapolateRight: 'clamp' });
  // Over the lower part of the code, so the running demo stays in view.
  const y = CODE.y + CODE.height * 0.62;
  const centre = (SAFE.left + SAFE.right) / 2;
  return (
    <Group x={centre} y={y} rotation={-4} scale={settle + 0.025 * pulse}>
      <Rect x={-W} y={-150} width={W * 2} height={300} fill={accent} shadow={{ color: '#00000080', blur: 32, offsetX: 0, offsetY: 12 }} />
      <Text anchorX={0.5} anchorY={0.5} maxWidth={SAFE.right - SAFE.left - 40}
        style={{
          fontFamily: FONT.ja, fontSize: 88, fontWeight: 900, lineHeight: 112, align: 'center',
          lineBreak: 'phrase', fill: { type: 'solid', color: C.ink },
        }}>
        {text}
      </Text>
    </Group>
  );
}
