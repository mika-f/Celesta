import { Group, Rect } from '@celesta/react';
import { Label } from '../components/Label';
import { ACID, BLUE, BONE, GREY, H, INK, W } from '../constants';
import { clamp, mix } from '../math';

export function Scrub({ f }: { f: number }) {
  const local = f - 300;
  const position = local < 45 ? local / 45 : local < 65 ? 1 - (local - 45) / 35
    : 0.43 + (local - 65) / 25 * 0.57;
  const playX = mix(87, 1823, clamp(position));
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Label x={69} y={91} size={160} color={BONE} weight={700}>EVERY FRAME.</Label>
    <Label x={78} y={303} size={21} mono color={GREY}>THE TIMELINE / SCRUB FORWARD, THEN BACK.</Label>
    {[0, 1].map(row => <Rect key={row} x={75} y={395 + row * 239}
      width={1771} height={209} fill={row ? '#25292A' : '#1B2020'} />)}
    {Array.from({ length: 16 }, (_, i) => {
      const x = 84 + i * 109;
      return <Group key={i}>
        <Rect x={x} y={407} width={103} height={185}
          fill={i % 4 === 0 ? ACID : i % 4 === 1 ? '#ADB6AE' : '#465B59'} />
        <Rect x={x + 51} y={505} anchorX={0.5} anchorY={0.5}
          width={20 + i * 2.4} height={20 + i * 2.4} fill={INK}
          rotation={i * 17 + f * 0.2} />
        <Rect x={x} y={646 + (i % 3) * 8}
          width={103} height={110 - (i % 3) * 15} fill={i % 2 ? BLUE : ACID} />
      </Group>;
    })}
    <Rect x={playX} y={350} width={5} height={524} fill={BONE} />
    <Rect x={playX - 13} y={344} width={31} height={25} fill={BONE} />
    <Label x={77} y={887} size={40} mono color={ACID}>
      {`FRAME ${String(Math.floor(clamp(position) * 479)).padStart(3, '0')}`}
    </Label>
    <Label x={1546} y={908} size={22} mono color={GREY}>← 30 FPS →</Label>
    <Rect x={76} y={1000} width={1770} height={2} fill={GREY} />
  </>;
}
