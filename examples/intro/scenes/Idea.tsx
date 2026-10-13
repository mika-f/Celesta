import { Easings, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { Arrow } from '@celesta/shapes';
import { Headline } from '../components/Headline';
import { Label } from '../components/Label';
import { BAR, C, W } from '../constants';
import { BALL, ballY } from '../shared/ball';
import { scrubFrame } from '../shared/scrub';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

// The scrub bar spans frames 0–120 of the ball. Above it, the ball is
// plotted against its frame number: x is the frame, y is ballY(frame) scaled.
const BAR_X = 360;
const BAR_W = 1200;
const BAR_Y = 860;
const SPAN = 120;
const PLOT = { y: 300, scale: 0.4 } as const;
const SIZE = BALL.size * PLOT.scale;
const plotX = (f: number) => BAR_X + (f / SPAN) * BAR_W;
const plotY = (f: number) => PLOT.y + ballY(f) * PLOT.scale;

// Frame numbers already shown at an earlier local frame, every fifth one, so
// their outlines mark where the ball must land again.
function visitedBefore(local: number): number[] {
  const seen = new Set<number>();
  for (let l = 0; l < local; l += 1) {
    const f = scrubFrame(l);
    if (f % 5 === 0) seen.add(f);
  }
  return [...seen];
}

// 18–26 s. A frame number in, a picture out: sweeping the frame back and
// forth puts the ball exactly on its earlier outline each time the number
// repeats.
export function Idea() {
  const local = useCurrentFrame();
  const frame = scrubFrame(local);
  const enter = interpolate(local, [0, 12], [0, 1], { ...clamp, easing: Easings.easeOutCubic });
  const arrow = interpolate(local, [6, 20], [0, 1], { ...clamp, easing: Easings.easeOutCubic });
  const copy = interpolate(local, [BAR * 3, BAR * 3 + 10], [0, 1], clamp);
  const playhead = BAR_X + (frame / SPAN) * BAR_W;
  return (
    <>
      <Group opacity={enter}>
        <Label x={160} y={110} size={96} weight={700}>frame</Label>
        <Label x={160} y={230} size={32} color={C.accent}>{`= ${frame}`}</Label>
        <Headline x={1000} y={110} size={96}>picture</Headline>
      </Group>
      {arrow > 0 && <Arrow x1={540} y1={168} x2={540 + 380 * arrow} y2={168} stroke={C.accent} strokeWidth={6} />}

      <Rect x={BAR_X} y={PLOT.y + BALL.floor * PLOT.scale} width={BAR_W} height={1} fill={C.line} />
      {visitedBefore(local).map((f) => (
        <Rect key={f} x={plotX(f) - SIZE / 2} y={plotY(f)} width={SIZE} height={SIZE} stroke={`${C.accent}99`} strokeWidth={1} />
      ))}
      <Rect x={plotX(frame) - SIZE / 2} y={plotY(frame)} width={SIZE} height={SIZE} fill={C.accent} />
      <Rect x={plotX(frame)} y={plotY(frame) + SIZE} width={1} height={BAR_Y - 88 - plotY(frame) - SIZE} fill={`${C.accent}66`} />

      <Rect x={BAR_X} y={BAR_Y} width={BAR_W} height={1} fill={C.line} />
      {Array.from({ length: SPAN / 10 + 1 }, (_, i) => (
        <Rect key={i} x={BAR_X + (i * 10 / SPAN) * BAR_W} y={BAR_Y} width={1} height={i % 3 === 0 ? 16 : 8} fill={C.muted} />
      ))}
      <Rect x={playhead - 1} y={BAR_Y - 40} width={2} height={56} fill={C.accent} />
      <Label x={playhead} y={BAR_Y - 52} size={22} color={C.accent} ax={0.5} ay={1}>{`${frame}`}</Label>

      <Headline x={W / 2} y={930} size={64} anchorX={0.5} opacity={copy}>A frame is a function of time.</Headline>
    </>
  );
}
