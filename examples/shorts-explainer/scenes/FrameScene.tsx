import { Easings, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { Circle } from '@celesta/shapes';
import { CodeCard, SYNTAX } from '../components/CodeCard';
import type { CodeLine } from '../components/CodeCard';
import { Label } from '../components/Label';
import { BODY, Panel } from '../components/Panel';
import { C } from '../constants';
import { lineFrom, plan } from '../voice';

export const TEXT = ['フレームは関数', '→ 絵'];

const CODE: CodeLine[] = [
  [['const ', SYNTAX.tag], ['x', SYNTAX.plain], [' = ', SYNTAX.plain], ['interpolate', SYNTAX.fn], ['(', SYNTAX.plain], ['frame', SYNTAX.attr], [', [0, 30], [0, 600])', SYNTAX.value]],
];

// "There is no timeline": the frame number goes in, a picture comes out.
// The ball's place is computed from the frame, so the earlier frames can be
// drawn again exactly, as the ghosts behind it.
export function FrameScene() {
  const frame = useCurrentFrame();
  const absolute = frame + plan.scene('frame').from;
  const answer = lineFrom('frame', 'frame-2');
  const lanes = BODY.y + 250;
  const ballAt = (f: number) => {
    const t = ((f % 60) + 60) % 60;
    return interpolate(t < 30 ? t : 60 - t, [0, 30], [0, BODY.w - 120], { easing: Easings.easeInOutSine });
  };
  const fn = interpolate(frame, [answer, answer + 10], [0, 1], { easing: Easings.easeOutBack, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Panel index="02" title="フレームは関数" accent={C.mint}>
      {/* The frame counter: the composition's real frame number. */}
      <Label x={BODY.x} y={BODY.y + 40} ay={0.5} size={40} font="mono" weight={700} color={C.soft}>frame</Label>
      <Label x={BODY.x + 150} y={BODY.y + 46} ay={0.5} size={110} font="mono" weight={800} color={C.ink}>
        {String(absolute).padStart(4, '0')}
      </Label>
      <Group opacity={fn} x={40 * (1 - fn)}>
        <Rect x={BODY.x + BODY.w - 290} y={BODY.y - 4} width={290} height={100} cornerRadius={50} fill={C.mint} stroke={C.ink} strokeWidth={5} />
        <Label x={BODY.x + BODY.w - 145} y={BODY.y + 46} ax={0.5} ay={0.5} size={50} weight={900} color={C.paper}>→ 絵</Label>
      </Group>
      {/* The lane: ghosts of the last frames, then the ball now. */}
      <Rect x={BODY.x} y={lanes - 4} width={BODY.w} height={8} cornerRadius={4} fill={C.line} />
      {[12, 8, 4].map((back) => (
        <Circle key={back} x={BODY.x + 60 + ballAt(absolute - back)} y={lanes} anchorX={0.5} anchorY={0.5} radius={52}
          fill={`${C.mint}${back === 12 ? '30' : back === 8 ? '55' : '88'}`} />
      ))}
      <Circle x={BODY.x + 60 + ballAt(absolute)} y={lanes} anchorX={0.5} anchorY={0.5} radius={56} fill={C.mint} stroke={C.ink} strokeWidth={6} />
      {/* Tick marks under the lane: one per frame of the last second. */}
      {Array.from({ length: 30 }, (_, i) => (
        <Rect key={i} x={BODY.x + (i / 29) * BODY.w} y={lanes + 84} anchorX={0.5} width={6} height={i === 29 ? 44 : 24}
          cornerRadius={3} fill={i === 29 ? C.ink : C.line} />
      ))}
      <Label x={BODY.x} y={lanes + 160} ay={0.5} size={32} weight={800} color={C.soft}>{`${absolute - 29}`}</Label>
      <Label x={BODY.x + BODY.w} y={lanes + 160} ax={1} ay={0.5} size={32} weight={800}>{`${absolute}`}</Label>
      <CodeCard x={BODY.x} y={BODY.y + BODY.h - 140} width={BODY.w} file="film.tsx" lines={CODE} size={26} />
    </Panel>
  );
}
