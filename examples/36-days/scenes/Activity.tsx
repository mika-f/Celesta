import type { PolylinePoint } from '@celesta/react';
import { Easings, Group, Line, Polyline, Rect, pointOnPolyline, progress, useCountUp, useCue, useCurrentFrame } from '@celesta/react';
import { Exit } from '../components/Exit';
import { Header } from '../components/Header';
import { Label } from '../components/Label';
import { C, H, W } from '../constants';
import { DAYS, NON_MERGE } from '../data';

const CHART = { x: 160, base: 860, step: 45.5, bar: 36, unit: 20, top: 300 };
const barX = (i: number) => CHART.x + i * CHART.step;
const CUMULATIVE: PolylinePoint[] = (() => {
  let total = 0;
  return DAYS.map((d, i) => {
    total += d.commits;
    return [barX(i) + CHART.bar / 2, CHART.base - (total / NON_MERGE) * (CHART.base - CHART.top)];
  });
})();

const CALLOUTS = [
  { at: 150, day: 0, text: '25 — 初日', lift: 60, days: [0, 0] },
  { at: 170, day: 13, text: '21 — エディタを作り直す', lift: 60, days: [13, 13] },
  { at: 190, day: 22, text: '15 日間の空白', lift: 120, days: [15, 29] },
];

export function Activity() {
  const f = useCurrentFrame();
  const drawn = progress(f, 70, 70, Easings.easeInOutCubic);
  const [tipX, tipY] = pointOnPolyline(CUMULATIVE, drawn);
  const total = useCountUp(NON_MERGE, { delay: 70, durationInFrames: 70 });
  const active = useCue(CALLOUTS);
  const callout = active?.cue;
  const lit = (i: number) => callout !== undefined && i >= callout.days[0] && i <= callout.days[1];

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Exit>
        <Header en="ACTIVITY" ja="日ごとのコミット数と、その累計" />
        <Rect x={CHART.x - 10} y={CHART.base} width={DAYS.length * CHART.step + 10} height={2} fill={C.line} />
        {DAYS.map((d, i) => {
          const grow = progress(f, 8 + i * 1.5, 20, Easings.easeOutBack);
          if (d.commits === 0) {
            return (
              <Rect key={d.date} x={barX(i) + CHART.bar / 2} y={CHART.base - 10} anchorX={0.5} anchorY={0.5}
                width={6} height={6} cornerRadius={3} fill={lit(i) ? C.paper : C.dim} opacity={Math.min(1, grow)} />
            );
          }
          return (
            <Rect key={d.date} x={barX(i)} y={CHART.base} anchorY={1} width={CHART.bar}
              height={Math.max(0.01, d.commits * CHART.unit * grow)} fill={lit(i) ? C.paper : C.mint}
              opacity={callout && !lit(i) ? 0.45 : 0.9} />
          );
        })}
        {[0, 7, 14, 21, 28, 35].map((i) => (
          <Label key={i} x={barX(i)} y={CHART.base + 36} size={18} font="mono" color={C.grey} ay={0.5}
            opacity={progress(f, 20, 12)}>{DAYS[i].date.slice(5)}</Label>
        ))}
        <Polyline points={CUMULATIVE} progress={drawn} strokeWidth={4} stroke={C.coral} />
        {f >= 70 && (
          <Group x={tipX} y={tipY}>
            <Rect anchorX={0.5} anchorY={0.5} width={16} height={16} cornerRadius={8} fill={C.coral} />
            <Label x={-18} y={-26} size={30} ax={1} ay="baseline" color={C.coral}>{total}</Label>
          </Group>
        )}
        {active && callout && (() => {
          const p = progress(active.frame, 0, 12, Easings.easeOutExpo);
          const x = barX(callout.day) + CHART.bar / 2;
          const top = CHART.base - DAYS[callout.day].commits * CHART.unit - 14;
          return (
            <Group opacity={p}>
              <Line x1={x} y1={top} x2={x} y2={top - callout.lift * p} stroke={C.paper} cap="butt" />
              <Label x={x + 12} y={top - callout.lift} size={28} font="ja" weight={700} ay={0.5}>{callout.text}</Label>
            </Group>
          );
        })()}
      </Exit>
    </>
  );
}
