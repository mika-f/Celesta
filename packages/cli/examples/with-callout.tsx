import { Callout, Motion, PatternBackground } from '@celesta.studio/components';
import { Composition, Easings, Font, Group, Rect, Text, Theme, progress, useCurrentFrame } from '@celesta.studio/react';

// <Callout> points at something: a box with a title and text, as tall as
// the text measures, and a line from the middle of the edge facing the
// target (or a named edge) to it, ending in an arrowhead. The target and
// the box are in the parent's coordinates, so a callout in the same group
// as its target stays on it whatever the group's transform; animate the
// target and the line follows.

const colors = { bg: '#0E1319', surface: '#182030', line: '#2C3A50', ink: '#F4F1EA', dim: '#8A97A8', accent: '#FF4D1F', sky: '#5BC0FF' };
const note = {
  titleStyle: { fontFamily: 'sans', fontSize: 16, fontWeight: 700 },
  textStyle: { fontFamily: 'sans', fontSize: 13, lineHeight: 19, fill: { type: 'solid', color: colors.dim } },
  fill: 'surface', stroke: 'line', strokeWidth: 1, padding: 12, cornerRadius: 10, color: 'ink',
} as const;

/** A dot to point at. */
const Dot = ({ x, y, fill = colors.accent }: { x: number; y: number; fill?: string }) =>
  <Rect x={x} y={y} anchorX={0.5} anchorY={0.5} width={12} height={12} cornerRadius={6} fill={fill} />;

export default function WithCallout() {
  const frame = useCurrentFrame();
  const t = frame / 30;
  // A target circling; its callout's line follows it.
  const moving = { x: 790 + Math.cos(t * 1.6) * 80, y: 300 + Math.sin(t * 1.6) * 40 };
  const grow = progress(frame, 0, 24, Easings.easeOutCubic);
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Theme fonts={{ sans: 'Inter' }} colors={colors}>
        <PatternBackground background="bg" pattern="dots" spacing={24} size={2} color="ink" opacity={0.08} />

        {/* side="auto": four callouts around one object, each line from the edge facing it. */}
        <Rect x={170} y={210} width={120} height={80} cornerRadius={10} fill="#FFFFFF14" stroke={colors.accent} strokeWidth={2} />
        <Callout {...note} target={{ x: 230, y: 210 }} x={160} y={60} width={140} title="above" lineColor="accent" />
        <Callout {...note} target={{ x: 230, y: 290 }} x={160} y={400} width={140} title="below" lineColor="accent" />
        <Callout {...note} target={{ x: 170, y: 250 }} x={20} y={225} width={100} title="left" lineColor="accent" />
        <Callout {...note} target={{ x: 290, y: 250 }} x={340} y={225} width={100} title="right" lineColor="accent" />

        {/* An elbow from a named edge; long text grows the box and the line keeps the edge's middle. */}
        <Dot x={520} y={200} fill={colors.sky} />
        <Callout {...note} target={{ x: 520, y: 200 }} x={600} y={30} width={320} side="bottom" route="elbow" lineColor="sky"
          title='route="elbow" side="bottom"'
          text="Long text wraps inside the padding and the box grows taller; the line still leaves the middle of the bottom edge and turns once." />

        {/* The target moves and the line follows it; lineProgress grows the line from the target. */}
        <Dot x={moving.x} y={moving.y} fill={colors.ink} />
        <Callout {...note} target={moving} x={560} y={400} width={170} title="animated target" text="lineProgress={grow}"
          lineProgress={grow} lineColor="ink" dash={[6, 5]} />

        {/* Callout and target in one scaled, rotated group: the line still meets the target. */}
        <Group x={770} y={360} scale={0.9} rotation={-8}>
          <Dot x={175} y={150} />
          <Motion enter="fade" from={30}>
            <Callout {...note} target={{ x: 175, y: 150 }} x={0} y={0} width={130} title="shared transform"
              text="Scaled and rotated with its target." lineColor="accent" />
          </Motion>
        </Group>
        <Text x={20} y={524} anchorY="baseline" fontFamily="sans" fontSize={12} color="dim">
          {'<Callout target={{ x, y }} x y width title text />'}
        </Text>
      </Theme>
    </Composition>
  );
}
