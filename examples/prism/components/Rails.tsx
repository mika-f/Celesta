import { Rect } from '@celesta/react';
import { Label } from './Label';
import { INK, PAPER } from '../constants';

export function Rails({ chapter, light = false }: { chapter: string; light?: boolean }) {
  const c = light ? INK : PAPER;
  return <>
    <Label x={80} y={52} size={20} mono color={c}>CELESTA / PRISM</Label>
    <Label x={1390} y={52} size={20} mono color={c}>{chapter}</Label>
    <Rect x={80} y={115} width={1760} height={1} fill={c} opacity={0.35} />
    <Rect x={80} y={987} width={1760} height={1} fill={c} opacity={0.35} />
    <Label x={80} y={1010} size={17} mono color={c}>IDEA → CODE → MOTION</Label>
    <Label x={1555} y={1010} size={17} mono color={c}>A FILM IN REACT</Label>
  </>;
}
