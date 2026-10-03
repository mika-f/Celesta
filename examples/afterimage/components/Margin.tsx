import { Rect } from '@celesta/react';
import { DURATION, FPS, INK } from '../constants';
import { Type } from './Type';

export function Margin({ frame, color = INK, label = 'AN OPTICAL STUDY' }: {
  frame: number; color?: string; label?: string;
}) {
  return <>
    <Type x={72} y={53} size={19} mono color={color}>AFTERIMAGE / 01</Type>
    <Type x={1480} y={53} size={19} mono color={color}>{label}</Type>
    <Rect x={72} y={998} width={1776} height={1} fill={color} opacity={0.4} />
    <Type x={72} y={1022} size={17} mono color={color}>LIGHT / FORM / MEMORY</Type>
    <Type x={1645} y={1022} size={17} mono color={color}>{`00:${String(Math.floor(frame / FPS)).padStart(2, '0')} / 00:${String(DURATION / FPS).padStart(2, '0')}`}</Type>
  </>;
}
