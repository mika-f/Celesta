import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { C, H, W } from '../constants';
import { progress } from '../helpers';

// ── 01 · React: a frame is a stack of layers ──────────────────────────────

export function ReactDemo() {
  const f = useCurrentFrame();
  // The blue that the index opened up drains away first.
  const drain = progress(f, 0, 12, Easings.easeInOutExpo);
  const cx = 1330;
  const base = 700;
  const pw = 480;
  const ph = 270;
  const rise = progress(f, 6, 18, Easings.easeOutExpo);
  const gap = 150 * progress(f, 28, 26, Easings.easeInOutCubic);
  const bob = progress(f, 54, 20);
  const plates = [
    { name: '<Rect>', note: 'fill="#3D5BFF"' },
    { name: '<Group>', note: '3 × <Rect cornerRadius>' },
    { name: '<Text>', note: '"Hello."' },
  ];
  return (
    <>
      {plates.map((plate, i) => {
        const y = base - i * gap - 80 * (1 - rise) + Math.sin((f + i * 9) / 12) * 5 * bob;
        const label = progress(f, 40 + i * 5, 12, Easings.easeOutExpo);
        return (
          <Group key={i}>
            <Group x={cx} y={y} opacity={rise}>
              <Group scaleY={0.56}>
                <Group rotation={45}>
                  <Plate layer={i} w={pw} h={ph} f={f} />
                </Group>
              </Group>
            </Group>
            <Group opacity={label}>
              <Rect x={cx + 170} y={y} width={Math.max(1, 110 * label)} height={1} fill={C.soft} />
              <Rect x={cx + 166} y={y - 4} width={8} height={8} cornerRadius={4} fill={C.paper} />
              <Label x={cx + 296} y={y - 12} size={22} font="mono" weight={700} ay={0.5}>{plate.name}</Label>
              <Label x={cx + 296} y={y + 18} size={15} font="mono" weight={400} color={C.grey} ay={0.5}>{plate.note}</Label>
            </Group>
          </Group>
        );
      })}
      <Rect width={W} height={H * (1 - drain)} fill={C.blue} />
    </>
  );
}

function Plate({ layer, w, h, f }: { layer: number; w: number; h: number; f: number }) {
  if (layer === 0) {
    return (
      <>
        <Rect anchorX={0.5} anchorY={0.5} width={w} height={h} fill={C.blue} />
        {Array.from({ length: 11 }, (_, i) => (
          <Rect key={i} x={-w / 2 + (i + 1) * (w / 12)} y={-h / 2} width={1} height={h} fill="#FFFFFF22" />
        ))}
      </>
    );
  }
  if (layer === 1) {
    return (
      <>
        <Rect anchorX={0.5} anchorY={0.5} width={w} height={h} fill="#FFFFFF08" stroke="#FFFFFF55" strokeWidth={2} />
        {[C.pink, C.paper, C.sky].map((color, k) => {
          const a = f * 0.07 + (k * Math.PI * 2) / 3;
          return (
            <Rect key={k} x={Math.cos(a) * 110} y={Math.sin(a) * 60} anchorX={0.5} anchorY={0.5}
              width={64} height={64} cornerRadius={32} fill={color} />
          );
        })}
      </>
    );
  }
  return (
    <>
      <Rect anchorX={0.5} anchorY={0.5} width={w} height={h} fill="#FFFFFF05" stroke="#FFFFFF55" strokeWidth={2} />
      <Label x={0} y={0} size={64} ax={0.5} ay={0.5}>Hello.</Label>
    </>
  );
}
