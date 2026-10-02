import { Easings, Group, Rect, Stagger, progress, useCountUp, useCurrentFrame } from '@celesta/react';
import { Exit } from '../components/Exit';
import { Header } from '../components/Header';
import { Label } from '../components/Label';
import { BEAT, C, H, W } from '../constants';
import { RUST_LINES, SOURCES, TOTAL_COMMITS, TS_LINES, fmt } from '../data';

const STATS = [
  { value: TOTAL_COMMITS, en: 'COMMITS', ja: 'マージを含む、すべてのコミット' },
  { value: SOURCES.length - 1, en: 'RUST CRATES', ja: 'ワークスペースのクレート' },
  { value: RUST_LINES, en: 'LINES OF RUST', ja: 'レンダラー、書き出し、エディタ' },
  { value: TS_LINES, en: 'LINES OF TYPESCRIPT', ja: '@celesta/react のランタイム' },
];

// Written for its own frame 0; <Stagger> starts each one a beat apart.
export function Stat({ index, x, y }: { index: number; x: number; y: number }) {
  const f = useCurrentFrame();
  const stat = STATS[index];
  const value = useCountUp(stat.value, { durationInFrames: 40 });
  return (
    <Group x={x} y={y} opacity={progress(f, 0, 12, Easings.easeOutExpo)}>
      <Rect width={760 * progress(f, 0, 24, Easings.easeInOutCubic)} height={2} fill={C.mint} />
      <Label y={40} size={24} font="mono" weight={700} color={C.mint}>{`0${index + 1}`}</Label>
      <Label x={60} y={40} size={24} font="mono" weight={700}>{stat.en}</Label>
      <Label y={200} size={140} ay="baseline">{fmt(value)}</Label>
      <Label y={250} size={24} font="ja" weight={500} color={C.soft} ay={0.5}>{stat.ja}</Label>
    </Group>
  );
}

export function Numbers() {
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="BY THE NUMBERS" ja="数字で見る" />
        <Stagger from={BEAT - 6} each={BEAT}>
          {STATS.map((stat, i) => (
            <Stat key={stat.en} index={i} x={160 + (i % 2) * 820} y={260 + Math.floor(i / 2) * 340} />
          ))}
        </Stagger>
      </Exit>
    </>
  );
}
