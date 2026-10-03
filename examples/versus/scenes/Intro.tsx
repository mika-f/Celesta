import { Easings, Group, Rect, Video, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { ToolTag } from '../components/Chrome';
import { Label } from '../components/Label';
import { C, H, ORDER, TOOL, W } from '../constants';
import { progress } from '../helpers';

function Slam({ f, at, y, children, color = C.ink }: { f: number; at: number; y: number; children: string; color?: string }) {
  const { fps } = useVideoConfig();
  const s = spring({ frame: f - at, fps, config: { damping: 15, stiffness: 220 } });
  return (
    <Group x={96} y={y} scale={1.25 - 0.25 * s} opacity={Math.min(1, s * 1.6)}>
      <Label size={168} weight={700} spacing={-6} color={color}>{children}</Label>
    </Group>
  );
}

export function Intro() {
  const f = useCurrentFrame();
  const back = progress(f, 0, 40);
  return (
    <>
      <Group opacity={0.38 * back} blur={10}>
        <Video src="./bench/out/celesta.mp4" startFrom={4} />
      </Group>
      <Rect width={W} height={H} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: W, y: 0 },
        stops: [{ offset: 0, color: '#05060CF2' }, { offset: 0.6, color: '#05060CB0' }, { offset: 1, color: '#05060C40' }],
      }} />
      <Group opacity={progress(f, 2, 12)}>
        <Rect x={96} y={206} width={Math.max(1, 36 * progress(f, 2, 16, Easings.easeOutExpo))} height={4} fill={TOOL.celesta.color} />
        <Label x={148} y={200} size={20} font="mono" weight={700} color={TOOL.celesta.color} spacing={4}>RENDER BENCHMARK · 2026</Label>
      </Group>
      <Slam f={f} at={8} y={270}>ONE SCENE.</Slam>
      <Slam f={f} at={24} y={450} color={TOOL.celesta.color}>THREE ENGINES.</Slam>
      <Label x={100} y={670} size={30} weight={400} color={C.soft} opacity={progress(f, 50, 18)}>
        Same heavy motion graphic, built three times. Which one is fast, and which one is easy?
      </Label>
      {ORDER.map((id, i) => {
        const p = progress(f, 70 + i * 8, 22, Easings.easeOutExpo);
        return (
          <Group key={id} x={100 + i * 440} y={800 + 24 * (1 - p)} opacity={p}>
            <Rect y={-24} width={400} height={2} fill={TOOL[id].color} />
            <ToolTag id={id} size={40} />
          </Group>
        );
      })}
    </>
  );
}
