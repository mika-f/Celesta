import { Group, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Glitch } from '../components/Glitch';
import { Label } from '../components/Label';
import { ACID, BONE, CYAN, H, INK, MAG, W } from '../constants';

export function Tunnel() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const N = 16;
  const colors = [ACID, MAG, CYAN];
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Group x={W / 2} y={H / 2} rotation={Math.sin(f / 45) * 5} scale={1 + pulse * 0.04}>
      {Array.from({ length: N }, (_, i) => {
        const t = ((i / N + f * 0.0045) % 1);
        const s = t ** 2.6 * 2.1;
        return <Rect key={i} anchorX={0.5} anchorY={0.5} width={W * s} height={H * s}
          stroke={colors[i % 3]} strokeWidth={2 + t * 5} rotation={t * 38 * Math.sin(f / 70)}
          opacity={Math.min(1, t * 6) * (1 - t * 0.25)} />;
      })}
    </Group>
    <Rect width={W} height={H} fill={BONE} opacity={1 - progress(f, 0, 14)} />
    <Group opacity={progress(f, 48, 6)}>
      <Glitch x={W / 2} y={H / 2 - 20} size={560} frame={f} amount={pulse * 14}>{'APEX'}</Glitch>
    </Group>
    <Label x={W / 2} y={H - 120} size={30} mono color={BONE} anchorX={0.5} letterSpacing={8}
      opacity={progress(f, 120, 20)}>{'A FILM WRITTEN IN REACT'}</Label>
  </>;
}
