import { Easings, Group, Rect, Video, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Label } from '../components/Label';
import { C, H, TOOL, W } from '../constants';
import { EXPORT } from '../data';
import { progress } from '../helpers';

export function Outro() {
  const f = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const word = spring({ frame: f - 30, fps, config: { damping: 14, stiffness: 110 } });
  const out = 1 - progress(f, durationInFrames - 24, 24, Easings.easeInCubic);
  const lines = ['Write it in React.', 'Render it on the GPU.'];
  return (
    <Group opacity={out}>
      <Group opacity={0.3 * progress(f, 0, 30)} blur={14}>
        <Video src="./bench/out/celesta.mp4" startFrom={5} />
      </Group>
      <Rect width={W} height={H} fill={{
        type: 'radial', center: { x: W / 2, y: H / 2 }, radius: 1100,
        stops: [{ offset: 0, color: '#05060C80' }, { offset: 1, color: '#05060CF8' }],
      }} />
      {lines.map((line, i) => {
        const p = progress(f, 2 + i * 8, 22, Easings.easeOutExpo);
        return (
          <Group key={line} x={W / 2} y={240 + i * 70} clip={{ x: -900, y: -40, width: 1800, height: 80 }}>
            <Label y={70 * (1 - p)} ax={0.5} ay={0.5} size={52} weight={500} color={i ? TOOL.celesta.color : C.ink}>{line}</Label>
          </Group>
        );
      })}
      <Group x={W / 2} y={540} scale={0.9 + 0.1 * word} opacity={Math.min(1, word)}
        glow={{ color: TOOL.celesta.color, blur: 32 }}>
        <Label ax={0.5} ay={0.5} size={230} weight={700} spacing={-8} color={C.ink}>Celesta</Label>
      </Group>
      <Label x={W / 2} y={720} ax={0.5} size={24} font="mono" weight={400} color={C.soft} opacity={progress(f, 56, 20)}>
        {`NEBULA → MP4 in ${EXPORT.celesta.toFixed(1)}s · one .tsx file · live reload`}
      </Label>
      <Label x={W / 2} y={800} ax={0.5} size={22} font="mono" weight={700} color={TOOL.celesta.color} spacing={2} opacity={progress(f, 70, 20)}>
        github.com/mika-f/celesta
      </Label>
    </Group>
  );
}
