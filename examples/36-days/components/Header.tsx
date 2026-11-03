import { Easings, Group, Rect, progress, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
import { Label, textStyle } from './Label';
import { C } from '../constants';

export function Header({ en, ja }: { en: string; ja: string }) {
  const f = useCurrentFrame();
  // The Japanese title follows the English one after two spaces.
  const enWidth = useTextMetrics(`${en}  `, textStyle('mono', 24, C.paper, 700)).width;
  return (
    <Group x={160} y={150} opacity={progress(f, 0, 14, Easings.easeOutExpo)}>
      <Rect y={-6} width={12} height={12} fill={C.mint} />
      <Label x={30} size={24} font="mono" weight={700} ay={0.5}>{en}</Label>
      <Label x={30 + enWidth} size={22} font="ja" weight={500} color={C.grey} ay={0.5}>{ja}</Label>
    </Group>
  );
}
