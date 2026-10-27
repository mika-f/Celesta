import { Easings, Group, Rect, Text, progress } from '@celesta/react';
import { Callout, Motion, PatternBackground } from '@celesta/components';
import { Circle } from '@celesta/shapes';
import { Delta, trianglePoints } from '../../components/Delta';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { mono } from './common';

// CALLOUT: three real <Callout>s. One follows the Δ as it drifts (its
// target is the Δ's apex, recomputed every frame), one leaves the bottom
// edge and turns once (route="elbow"), one grows from its target with
// lineProgress. All in the same group as what they point at.

const note = {
  titleStyle: { fontFamily: 'mono', fontSize: 20, fontWeight: 700 },
  textStyle: { fontFamily: 'sans', fontSize: 17, lineHeight: 24 },
  fill: 'raised', stroke: 'line', strokeWidth: 2, padding: 14, cornerRadius: 6, color: 'paper', lineWidth: 3,
} as const;

export function CalloutDemo({ frame }: DemoProps) {
  const t = frame / 30;
  const center = { x: 520 + Math.cos(t * 1.4) * 70, y: 300 + Math.sin(t * 2.1) * 34 };
  const size = 190;
  const [apex] = trianglePoints(size);
  const target = { x: center.x + apex[0], y: center.y + apex[1] };
  const lens = { x: 830, y: 420 };
  const grow = progress(frame, 2 * BAR, 14, Easings.easeOutCubic);
  return (
    <Group>
      <PatternBackground width={STAGE.width} height={STAGE.height} pattern="dots" spacing={28} size={2}
        background="panel" color="paper" opacity={0.08} />
      <Delta x={center.x} y={center.y} size={size} fill={C.orange} />
      <Circle x={lens.x} y={lens.y} anchorX={0.5} anchorY={0.5} radius={34} fill="raised" stroke="blue" strokeWidth={4} />
      <Circle x={lens.x} y={lens.y} anchorX={0.5} anchorY={0.5} radius={10} fill="blue" />
      <Rect x={130} y={455} width={190} height={56} cornerRadius={6} fill="#FFFFFF10" stroke="amber" strokeWidth={2} />
      <Text x={225} y={483} anchorX={0.5} anchorY={0.5} style={mono(22, C.amber, 700)}>{`frame ${String(Math.max(0, frame)).padStart(3, '0')}`}</Text>

      <Motion enter="fade" from={0}>
        <Callout {...note} target={target} x={60} y={60} width={300} title="target={apex}"
          text="the line follows the Δ, frame by frame" lineColor="orange"
          lineProgress={progress(frame, 0, 14, Easings.easeOutCubic)} />
      </Motion>
      <Motion enter="fade" from={BAR}>
        <Callout {...note} target={lens} x={640} y={40} width={300} side="bottom" route="elbow" title='route="elbow"'
          text='out of the bottom edge, one turn' lineColor="blue"
          lineProgress={progress(frame, BAR, 14, Easings.easeOutCubic)} />
      </Motion>
      <Motion enter="fade" from={2 * BAR}>
        <Callout {...note} target={{ x: 225, y: 455 }} x={390} y={430} width={250} side="left" title="lineProgress"
          text={`${grow.toFixed(2)} of the line`} lineColor="amber" dash={[8, 6]} lineProgress={grow} />
      </Motion>
    </Group>
  );
}
