import { Easings, Group, Rect } from '@celesta/react';
import { C } from '../constants';
import { progress } from '../helpers';
import { Label } from './Label';

export type Bar = { key: string; label: string; sub?: string; value: number; color: string; strong?: boolean };

// Horizontal bars that grow in turn while their values count up.
export function Bars({ bars, f, start, width, max, unit, rowHeight = 120, labelWidth = 330, decimals = 1 }: {
  bars: Bar[];
  f: number;
  start: number;
  width: number;
  max: number;
  unit: string;
  rowHeight?: number;
  labelWidth?: number;
  decimals?: number;
}) {
  const track = width - labelWidth - 170;
  return bars.map((bar, i) => {
    const appear = progress(f, start + i * 10, 16, Easings.easeOutExpo);
    const grow = progress(f, start + i * 10 + 4, 40, Easings.easeOutCubic);
    const value = bar.value * grow;
    return (
      <Group key={bar.key} y={i * rowHeight} opacity={appear}>
        <Label y={4} size={28} weight={700} color={bar.strong ? bar.color : C.ink}>{bar.label}</Label>
        {bar.sub ? <Label y={44} size={18} weight={400} color={C.grey}>{bar.sub}</Label> : null}
        <Rect x={labelWidth} y={4} width={track} height={56} cornerRadius={10} fill="#FFFFFF08" />
        <Rect x={labelWidth} y={4} width={Math.max(2, (track * value) / max)} height={56} cornerRadius={10}
          fill={bar.color} opacity={bar.strong ? 1 : 0.75} />
        <Label x={labelWidth + Math.max(2, (track * value) / max) + 20} y={10} size={38} font="mono" weight={700}
          color={bar.strong ? bar.color : C.ink}>{`${value.toFixed(decimals)} ${unit}`}</Label>
      </Group>
    );
  });
}
