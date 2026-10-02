import { Easings, Group, Rect, interpolate, spring, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Line } from '../components/Line';
import { Rails } from '../components/Rails';
import { FPS, H, INK, PAPER, RED, W } from '../constants';
import { ease, lerp } from '../math';

export function Motion() {
  const f = useCurrentFrame();
  const phase = interpolate(f, [0, 150], [0, 1], { easing: Easings.easeInOutCubic,
    extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return <>
    <Rect width={W} height={H} fill={RED} />
    <Rails chapter="02 / FRAME BY FRAME" light />
    <Label y={157} size={190} color={INK}>NOTHING BY CHANCE.</Label>
    <Label y={383} size={22} mono color={INK}>INTERPOLATE. SPRING. SEQUENCE.</Label>
    {Array.from({ length: 8 }, (_, i) => {
      const rise = spring({ frame: f, fps: FPS, delay: i * 4,
        config: { damping: 13, stiffness: 95 } });
      const cx = 185 + i * 220;
      return <Group key={i}>
        <Line a={[cx, 500]} b={[cx, 890]} color={INK} opacity={0.2} />
        <Rect x={cx} y={lerp(840, 659 + Math.sin(phase * 6.28 + i * 0.7) * 120, rise)}
          anchorX={0.5} anchorY={0.5} width={132} height={132}
          opacity={ease((f - i * 4) / 12)}
          rotation={phase * 180 + i * 12} fill={i % 3 === 0 ? PAPER : INK}
          cornerRadius={i % 2 ? 66 : 0} />
        <Label x={cx} y={935} size={18} mono center color={INK}>{`0${i + 1}`}</Label>
      </Group>;
    })}
  </>;
}
