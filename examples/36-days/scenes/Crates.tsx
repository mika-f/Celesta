import { Easings, Group, Rect, Stagger, progress, useCountUp, useCurrentFrame } from '@celesta/react';
import { Exit } from '../components/Exit';
import { Header } from '../components/Header';
import { Label } from '../components/Label';
import { C, H, W } from '../constants';
import { RUST_LINES, SOURCES, TS_LINES, fmt } from '../data';

const MAX_LINES = SOURCES[0].lines;

export function CrateRow({ index }: { index: number }) {
  const f = useCurrentFrame();
  const s = SOURCES[index];
  const lines = useCountUp(s.lines);
  const width = 1000 * (s.lines / MAX_LINES) * progress(f, 0, 30, Easings.easeOutExpo);
  return (
    <Group y={240 + index * 50} opacity={progress(f, 0, 8)}>
      <Label x={160} size={24} font="mono" color={s.ts ? C.coral : C.paper} ay={0.5}>{s.name}</Label>
      <Rect x={520} y={-12} width={Math.max(0.01, width)} height={24} fill={s.ts ? C.coral : C.mint} />
      <Label x={540 + width} size={22} font="mono" color={C.soft} ay={0.5}>{fmt(lines)}</Label>
    </Group>
  );
}

export function Crates() {
  const f = useCurrentFrame();
  const total = useCountUp(RUST_LINES + TS_LINES, { delay: 100, durationInFrames: 40 });
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="WHERE THE CODE LIVES" ja="クレートごとのソース行数" />
        <Stagger from={10} each={3}>
          {SOURCES.map((s, i) => <CrateRow key={s.name} index={i} />)}
        </Stagger>
        <Group opacity={progress(f, 100, 12)}>
          <Label x={W - 160} y={930} size={22} font="mono" color={C.grey} ax={1} ay={0.5}>
            {`RUST ${fmt(RUST_LINES)}  +  TYPESCRIPT ${fmt(TS_LINES)}`}
          </Label>
          <Label x={W - 160} y={880} size={64} ax={1} ay="baseline">{fmt(total)}</Label>
        </Group>
      </Exit>
    </>
  );
}
