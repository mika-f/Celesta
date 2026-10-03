import { Camera, Easings, Polyline, Rect, interpolate, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Disc } from '../components/Disc';
import { Label } from '../components/Label';
import { ACID, BONE, CYAN, GREY, H, INK, MAG, TAU, W } from '../constants';
import { proj, rot } from '../math';

export function Gyro() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const rings = [
    { r: 250, ax: 0.0, ay: 0.4, sx: 0.011, sy: 0.017, color: ACID },
    { r: 330, ax: 1.0, ay: 0.0, sx: 0.013, sy: -0.009, color: MAG },
    { r: 410, ax: 0.4, ay: 1.2, sx: -0.008, sy: 0.014, color: CYAN },
    { r: 490, ax: 2.0, ay: 0.7, sx: 0.006, sy: 0.01, color: BONE },
    { r: 570, ax: 0.7, ay: 2.4, sx: -0.01, sy: -0.007, color: ACID },
  ];
  const zoom = interpolate(f, [0, 240], [0.82, 1.18], { easing: Easings.easeInOutSine });
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Camera zoom={zoom} rotation={Math.sin(f / 80) * 3}>
      {rings.map((g, i) => {
        const pts = Array.from({ length: 129 }, (_, k): [number, number] => {
          const a = k / 128 * TAU;
          return proj(rot([Math.cos(a) * g.r, Math.sin(a) * g.r, 0], g.ax + f * g.sx, g.ay + f * g.sy), W / 2, H / 2);
        });
        return <Polyline key={i} points={pts} progress={progress(f, i * 8, 40, Easings.easeOutCubic)}
          stroke={g.color} strokeWidth={i % 2 ? 2 : 4} opacity={0.9} />;
      })}
      {rings.map((g, i) => {
        const a = f * 0.05 * (i % 2 ? -1 : 1) + i * 1.3;
        const [px, py] = proj(rot([Math.cos(a) * g.r, Math.sin(a) * g.r, 0], g.ax + f * g.sx, g.ay + f * g.sy), W / 2, H / 2);
        return <Disc key={i} x={px} y={py} r={10 + pulse * 5} color={g.color} />;
      })}
      <Rect x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} width={110 + pulse * 22} height={110 + pulse * 22}
        rotation={f * 1.5} fill={ACID} glow={{ color: ACID, blur: 40 }} />
    </Camera>
    <Label x={72} y={64} size={28} mono color={GREY}>{'03 — GYRO'}</Label>
    <Label x={72} y={H - 140} size={110} color={BONE}>{`${String(Math.round((f * 1.5) % 360)).padStart(3, '0')}°`}</Label>
    <Label x={W - 72} y={H - 100} size={26} mono color={GREY} anchorX={1}>{'5 AXES · 640 SAMPLES · NO MESH'}</Label>
  </>;
}
