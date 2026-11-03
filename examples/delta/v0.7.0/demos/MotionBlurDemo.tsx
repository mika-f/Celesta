import { Easings, Group, MotionBlur, Rect, Text, Trail, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { Circle, Star } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { CUT, STAGE, enter } from '../../components/ChapterShell';
import { Delta } from '../../components/Delta';
import { C, FONT, solid } from '../../components/theme';
import { BAR } from '../timeline';

// Three lanes on one clock. The wheel and the Δ are drawn the same way in
// the first two; only the second is inside a <MotionBlur>. The star in the
// third bounces with a <Trail>. Inside, useCurrentFrame() is each moment's
// own frame, so the components animate between the samples by themselves.
const LANE = 236;

/** Seconds of the lane's own clock: the chapter frame, from inside a moment. */
function useLaneFrame() {
  return useCurrentFrame() - CUT;
}

function Wheel() {
  const f = useLaneFrame();
  const angle = interpolate(f, [0, 2 * BAR, 4 * BAR], [0, 1600, 2600], { easing: Easings.easeInOutSine, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Group x={120} y={LANE / 2} rotation={angle}>
      <Circle anchorX={0.5} anchorY={0.5} radius={82} stroke={C.paper} strokeWidth={8} />
      {[0, 45, 90, 135].map((a) => (
        <Rect key={a} anchorX={0.5} anchorY={0.5} width={160} height={10} rotation={a} fill={a === 0 ? C.orange : C.paper} />
      ))}
    </Group>
  );
}

function Dash() {
  const f = useLaneFrame();
  // A whip across and back, once a bar.
  const phase = ((f % BAR) + BAR) % BAR;
  const x = interpolate(phase, [0, 14, 20, 34, 40], [300, 760, 760, 300, 300], { easing: Easings.easeInOutCubic });
  return <Delta x={x} y={LANE / 2} size={110} fill={C.orange} />;
}

function Lane({ y, label, sub, children, on }: { y: number; label: string; sub: string; children: React.ReactNode; on: number }) {
  return (
    <Group y={y} opacity={on}>
      <Rect width={STAGE.width} height={LANE - 12} cornerRadius={10} fill={C.raised} />
      <Group clip={{ width: STAGE.width, height: LANE - 12, cornerRadius: 10 }}>{children}</Group>
      <Text x={STAGE.width - 20} y={22} anchorX={1} style={{ fontFamily: FONT.mono, fontSize: 20, fontWeight: 700, fill: solid(C.paper) }}>{label}</Text>
      <Text x={STAGE.width - 20} y={52} anchorX={1} style={{ fontFamily: FONT.mono, fontSize: 17, fill: solid(C.grey) }}>{sub}</Text>
    </Group>
  );
}

function Bouncer() {
  const f = useLaneFrame();
  const t = ((f % 20) + 20) % 20 / 20;
  const x = 80 + ((f + 400) * 9) % 700;
  const y = LANE - 70 - 120 * 4 * t * (1 - t);
  return <Star x={x} y={y} anchorX={0.5} anchorY={0.5} radius={36} points={5} innerRatio={0.45} rotation={f * 9} fill={C.amber} />;
}

export function MotionBlurDemo({ frame }: DemoProps) {
  return (
    <Group>
      <Lane y={0} label="no blur" sub="the same wheel and Δ" on={enter(frame, 0)}>
        <Wheel /><Dash />
      </Lane>
      <Lane y={LANE} label="<MotionBlur>" sub="shutterAngle={270} samples={12}" on={enter(frame, 4)}>
        <MotionBlur shutterAngle={270} samples={12}>
          <Wheel /><Dash />
        </MotionBlur>
      </Lane>
      <Lane y={2 * LANE} label="<Trail>" sub="copies={6} lagInFrames={1.5}" on={enter(frame, 8)}>
        <Trail copies={6} lagInFrames={1.5} trailOpacity={0.6}>
          <Bouncer />
        </Trail>
      </Lane>
    </Group>
  );
}
