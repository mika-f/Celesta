import { Group, Rect } from '@celesta/react';
import { C } from '../constants';
import type { CardDatum } from '../shared/cards';
import { Headline } from './Headline';
import { Label } from './Label';

export const CARD = { width: 260, height: 150 } as const;

// One card, laid out like CARD_SOURCE: title, value, and a bar of value × 2.2.
// `bar` overrides the bar's value while it animates toward datum.value.
export function Card({ x, y, datum, bar = datum.value, lit = false, opacity = 1 }: {
  x: number; y: number; datum: CardDatum; bar?: number; lit?: boolean; opacity?: number;
}) {
  return (
    <Group x={x} y={y} opacity={opacity}>
      <Rect width={CARD.width} height={CARD.height} fill={C.panel} stroke={lit ? C.accent : C.line} strokeWidth={1} />
      <Group x={20} y={16}>
        <Label x={0} y={0} size={20} color={C.muted}>{datum.title}</Label>
        <Headline x={0} y={40} size={56}>{`${datum.value}`}</Headline>
        <Rect y={120} width={bar * 2.2} height={6} fill={C.accent} />
      </Group>
    </Group>
  );
}
