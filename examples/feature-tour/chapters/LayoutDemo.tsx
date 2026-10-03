import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { C } from '../constants';
import { clamp, pad, progress } from '../helpers';

// ── 03 · Layout: Grid, then Stack, then Center ────────────────────────────

const TILES = 12;

function tileState(i: number, state: number) {
  const cx = 1390;
  const cy = 540;
  if (state === 0) {
    const c = i % 4;
    const r = Math.floor(i / 4);
    return { x: 1050 + c * 170 + 75, y: 300 + r * 170 + 75, size: 150, rotation: 0, fill: 1 };
  }
  if (state === 1) {
    return { x: cx - (TILES - 1) * 32 + i * 64, y: cy, size: 52, rotation: 0, fill: 1 };
  }
  return { x: cx, y: cy, size: 300, rotation: i * 15, fill: i === 10 ? 1 : 0 };
}

export function LayoutDemo() {
  const f = useCurrentFrame();
  const guides = progress(f, 0, 14) * (1 - progress(f, 45, 10));
  return (
    <>
      {/* Grid guides. */}
      <Group opacity={guides * 0.9}>
        {Array.from({ length: 5 }, (_, c) => (
          <Rect key={`c${c}`} x={1045 + c * 170} y={290} width={1} height={3 * 170} fill={C.dim} />
        ))}
        {Array.from({ length: 4 }, (_, r) => (
          <Rect key={`r${r}`} x={1045} y={290 + r * 170} width={4 * 170} height={1} fill={C.dim} />
        ))}
      </Group>
      {Array.from({ length: TILES }, (_, i) => {
        const enter = progress(f, 4 + i * 1.5, 12, Easings.easeOutBack);
        const toStack = progress(f, 45 + i * 0.8, 16, Easings.easeInOutExpo);
        const toCenter = progress(f, 90 + (TILES - i) * 0.8, 16, Easings.easeInOutExpo);
        const a = tileState(i, 0);
        const b = tileState(i, 1);
        const c = tileState(i, 2);
        const mix = (k: keyof typeof a) => a[k] + (b[k] - a[k]) * toStack + (c[k] - b[k]) * toCenter;
        const size = mix('size') * (0.5 + 0.5 * enter);
        const fill = mix('fill');
        const color = i % 5 === 0 ? C.blue : i % 3 === 0 ? C.paper : C.clip;
        return (
          <Group key={i} x={mix('x')} y={mix('y')} rotation={mix('rotation')} opacity={enter}>
            <Rect anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size * 0.06}
              fill={fill > 0.02 ? color : undefined} opacity={1}
              stroke={color === C.clip || fill < 0.98 ? '#FFFFFF40' : undefined}
              strokeWidth={color === C.clip || fill < 0.98 ? 1 : undefined} />
            {fill < 0.98 && (
              <Rect anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size * 0.06}
                fill={color} opacity={fill} />
            )}
            <Label x={-size / 2 + 12} y={-size / 2 + 20} size={15} font="mono" weight={700}
              color={color === C.paper ? C.ink : C.paper} ay={0.5} opacity={clamp((size - 100) / 40)}>
              {pad(i + 1)}
            </Label>
          </Group>
        );
      })}
    </>
  );
}
