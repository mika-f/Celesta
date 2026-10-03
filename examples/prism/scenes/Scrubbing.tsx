import { Easings, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Line } from '../components/Line';
import { Prism } from '../components/Prism';
import { Rails } from '../components/Rails';
import { H, INK, PAPER, RED, W } from '../constants';

export function Scrubbing() {
  const f = useCurrentFrame();
  const position = interpolate(f, [0, 55, 100, 149], [0, 0.9, 0.3, 0.72], {
    easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  const selectedFrame = Math.round(position * 149);
  return <>
    <Rect width={W} height={H} fill={PAPER} />
    <Rails chapter="06 / PREVIEW & SCRUB" light />
    <Label y={169} size={180} color={INK}>EVERY FRAME IS YOURS.</Label>
    <Label y={388} size={23} mono color={INK}>SCRUB FORWARD. GO BACK. FIND THE EXACT MOMENT.</Label>
    <Rect x={80} y={462} width={790} height={406} fill={INK} />
    <Prism frame={selectedFrame + 180} x={475} y={648} size={132} count={19} />
    <Label x={922} y={476} size={19} mono color={INK}>CURRENT FRAME</Label>
    <Label x={910} y={526} size={231} color={INK}>{String(selectedFrame).padStart(3, '0')}</Label>
    <Label x={925} y={802} size={24} mono color={INK}>SAME FRAME. SAME RESULT.</Label>
    <Line a={[80, 927]} b={[1840, 927]} color={INK} opacity={0.3} />
    {Array.from({ length: 61 }, (_, i) => <Rect key={i} x={80 + i * 1760 / 60}
      y={912} width={1} height={i % 5 === 0 ? 32 : 14} fill={INK} opacity={0.55} />)}
    <Rect x={80 + position * 1760} y={895} width={5} height={68} fill={RED} />
    <Rect x={72 + position * 1760} y={889} width={21} height={12} fill={RED} />
  </>;
}
