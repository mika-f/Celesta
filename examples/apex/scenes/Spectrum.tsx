import { Group, Rect, noise, progress, useBeat, useCountUp, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { ACID, BONE, CYAN, GREY, H, INK, MAG, W } from '../constants';
import { mix } from '../math';

export function Spectrum() {
  const f = useCurrentFrame();
  const { pulse, beat } = useBeat({ bpm: 120, decay: 5 });
  const count = 80, step = (W - 160) / count;
  const bpm = useCountUp(120, { durationInFrames: 36 });
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Label x={W / 2} y={H / 2} size={720} color={BONE} anchorX={0.5} anchorY={0.5} outline opacity={0.1}>
      {`${bpm}`}
    </Label>
    {Array.from({ length: count }, (_, i) => {
      const u = i / (count - 1);
      const env = 0.35 + 0.65 * Math.sin(u * Math.PI) ** 0.8;
      const v = (0.5 + 0.5 * noise(`s${i}`, f / 6 + i * 0.4)) * env + pulse * 0.28 * (1 - u * 0.5);
      const h = 14 + 440 * Math.min(1, v) * progress(f, i * 0.5, 16);
      const col = u < 0.5 ? mix(0, 1, u * 2) : 1;
      return <Group key={i}>
        <Rect x={80 + i * step} y={H / 2 - h / 2} width={step - 5} height={h}
          fill={u < 0.34 ? ACID : u < 0.67 ? CYAN : MAG} opacity={0.55 + 0.45 * col} />
      </Group>;
    })}
    <Rect x={80} y={H / 2 - 1} width={W - 160} height={2} fill={BONE} />
    <Label x={72} y={64} size={28} mono color={GREY}>{'04 — SPECTRUM'}</Label>
    <Label x={W - 72} y={64} size={28} mono color={BONE} anchorX={1}>{`BEAT ${String(beat + 1).padStart(3, '0')}`}</Label>
    <Label x={72} y={H - 96} size={26} mono color={GREY}>{'80 BANDS · DRIVEN BY noise() + useBeat()'}</Label>
  </>;
}
