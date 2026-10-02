import type { ReactNode } from 'react';
import { Easings, Group, Rect, spring, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { BAR, C, DX, FPS } from '../constants';
import { clamp, pad, progress } from '../helpers';

// ── 02 · Motion: three curves, one frame number ───────────────────────────

const CURVES: { name: string; fn: (t: number) => number }[] = [
  { name: 'easeOutExpo', fn: Easings.easeOutExpo },
  { name: 'easeInOutBack', fn: Easings.easeInOutBack },
  { name: 'spring()', fn: (t) => spring({ frame: t * 40, fps: FPS, config: { damping: 8, stiffness: 140 } }) },
];

export function MotionDemo() {
  const f = useCurrentFrame();
  const size = 250;
  const gapX = 25;
  const top = 280;
  const lo = -0.3;
  const hi = 1.3;
  const py = (v: number) => top + size - 20 - ((v - lo) / (hi - lo)) * (size - 40);
  return (
    <>
      {CURVES.map(({ name, fn }, i) => {
        const x0 = DX + i * (size + gapX);
        const enter = progress(f, 4 + i * 4, 16, Easings.easeOutExpo);
        const cycle = (f + 60 - i * 3) % BAR;
        const t = clamp((cycle - 8) / 36);
        const v = fn(t);
        const dots: ReactNode[] = [];
        for (let k = 0; k <= 48; k++) {
          const u = k / 48;
          dots.push(
            <Rect key={k} x={x0 + 20 + u * (size - 40)} y={py(fn(u))} anchorX={0.5} anchorY={0.5}
              width={4} height={4} cornerRadius={2} fill={u <= t ? C.paper : C.dim} />,
          );
        }
        const trackY = top + size + 120;
        return (
          <Group key={name} y={40 * (1 - enter)} opacity={enter}>
            <Rect x={x0} y={top} width={size} height={size} cornerRadius={14} fill={C.panel} stroke={C.line} strokeWidth={1} />
            <Rect x={x0 + 20} y={py(0)} width={size - 40} height={1} fill={C.dim} />
            <Rect x={x0 + 20} y={py(1)} width={size - 40} height={1} fill={C.dim} />
            {dots}
            <Rect x={x0 + 20 + t * (size - 40)} y={py(v)} anchorX={0.5} anchorY={0.5}
              width={16} height={16} cornerRadius={8} fill={C.blue} />
            <Label x={x0 + size - 16} y={top + 22} size={15} font="mono" weight={400} color={C.grey} ax={1} ay={0.5}>
              {v.toFixed(3)}
            </Label>
            <Label x={x0} y={top + size + 34} size={19} font="mono" weight={700} ay={0.5}>{name}</Label>
            <Rect x={x0} y={trackY} width={size} height={1} fill={C.dim} />
            <Rect x={x0 + 22 + v * (size - 44)} y={trackY} anchorX={0.5} anchorY={0.5}
              width={40} height={40} rotation={v * 90} fill={i === 1 ? C.pink : C.blue} />
            <Rect x={x0 + size / 2} y={trackY + 110} anchorX={0.5} anchorY={0.5}
              width={16 + 60 * clamp(v, 0, 1.4)} height={16 + 60 * clamp(v, 0, 1.4)}
              cornerRadius={8 + 30 * clamp(v, 0, 1.4)} fill="#FFFFFF00" stroke={C.soft} strokeWidth={2} />
          </Group>
        );
      })}
      <Label x={DX} y={880} size={18} font="mono" weight={400} color={C.grey} ay={0.5}
        opacity={progress(f, 20, 12)}>{`frame ${pad(f, 3)} → t = ${clamp(((f % BAR) - 8) / 36).toFixed(2)}`}</Label>
    </>
  );
}
