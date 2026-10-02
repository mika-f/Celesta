import { Rect } from '@celesta/react';
import { Ribbon } from '../components/Ribbon';
import { Type } from '../components/Type';
import { FPS, INK, PAPER } from '../constants';

export function Impact({ f }: { f: number }) {
  const t = f / FPS;
  return <>
    <Ribbon time={t * 1.3} size={790 - (f - 300) * 2.4} color={INK} turn={1.2} strands={17} />
    <Type x={960} y={540} center size={f < 330 ? 530 : f < 360 ? 470 : 400} color={PAPER}>
      {f < 330 ? 'HOLD' : f < 360 ? 'THAT' : 'FEELING.'}
    </Type>
    <Type x={74} y={56} mono size={19} color={INK}>DO NOT LOOK AWAY.</Type>
    <Type x={74} y={1017} mono size={19} color={INK}>A MOMENT CAN OUTLIVE ITSELF.</Type>
    <Rect x={1825} y={62} width={20} height={20} fill={INK} />
  </>;
}
