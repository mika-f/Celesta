import { Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { BLUE, H, INK, PAPER, RED, W } from '../constants';
import { ease } from '../math';

export function Punctuation() {
  const f = useCurrentFrame();
  const index = Math.floor(f / 30);
  const local = f % 30;
  const bg = [PAPER, RED, BLUE][index];
  return <>
    <Rect width={W} height={H} fill={bg} />
    <Label x={960} y={515 + (1 - ease(local / 9)) * 140} size={410} center
      color={index === 2 ? PAPER : INK}>{['WRITE.', 'PLAY.', 'REPEAT.'][index]}</Label>
    <Label x={960} y={872} size={24} mono center color={index === 2 ? PAPER : INK}>FROM THE FIRST IDEA TO THE FINAL FRAME.</Label>
  </>;
}
