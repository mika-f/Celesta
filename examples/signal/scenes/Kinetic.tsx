import { Rect } from '@celesta/react';
import { Label } from '../components/Label';
import { Rotor } from '../components/Rotor';
import { ACID, BLUE, BONE, H, INK, W } from '../constants';
import { ease } from '../math';

export function Kinetic({ f }: { f: number }) {
  const local = f < 210 ? f - 150 : f < 255 ? f - 210 : f - 255;
  const stage = f < 210 ? 0 : f < 255 ? 1 : 2;
  const bg = [INK, ACID, BLUE][stage], fg = [BONE, INK, BONE][stage];
  const accent = [ACID, BLUE, ACID][stage];
  const word = ['COMPOSE.', 'PREVIEW.', 'RENDER.'][stage];
  const slide = (1 - ease(local / 12)) * 530;
  return <>
    <Rect width={W} height={H} fill={bg} />
    <Label x={75} y={63} size={23} mono color={fg}>
      {`0${stage + 2} / ${word.slice(0, -1)}`}
    </Label>
    <Rect x={76} y={138} width={1770} height={3} fill={fg} />
    <Rotor frame={f} x={1441} y={562} color={accent} size={1.22} />
    <Label x={72 - slide} y={335} size={183} color={fg} weight={700}>{word}</Label>
    <Label x={80} y={807} size={25} mono color={fg}>
      {stage === 0 ? '1920 × 1080 / 30 FPS' :
        stage === 1 ? 'NO WAITING FOR A RENDER TO SEE IT.' : 'H.264 + AAC / MP4'}
    </Label>
    <Rect x={76} y={980} width={1770} height={2} fill={fg} />
    {Array.from({ length: 9 }, (_, i) => <Rect key={i}
      x={W - Math.max(0, 9 - i) * 244 + local * 80} y={0}
      width={24} height={H} fill={fg} opacity={0.8 * (1 - ease(local / 12))} />)}
  </>;
}
