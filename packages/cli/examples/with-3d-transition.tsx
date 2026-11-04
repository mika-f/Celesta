import { Composition, Rect, Text, useVideoConfig } from '@celesta.studio/react';
import { TransitionSeries, computeTransitionSeries, cube, defineTransition, flip } from '@celesta.studio/transitions';

// Transitions that turn the scenes in 3D about the middle of the screen. A
// scene's group turns about `originX`/`originY`, which is also the vanishing
// point of its `perspective`, so `present()` alone can put the axis at the
// series' centre.

/**
 * The leaving scene tips back about its horizontal centre line and sinks
 * away as the next one tips up into place from below.
 */
const tumble = defineTransition({
  name: 'tumble',
  present: ({ progress, width, height }) => {
    const turned = Math.min(1, Math.max(0, progress));
    const about = { originX: width / 2, originY: height / 2, perspective: 1600 };
    return {
      // Past 90° the faces turn their backs, so each is hidden then.
      enter: { ...about, rotationX: -90 * (1 - turned), backfaceVisibility: 'hidden', opacity: turned < 0.5 ? 0 : 1 },
      exit: { ...about, rotationX: 90 * turned, brightness: 1 - 0.5 * turned, opacity: turned < 0.5 ? 1 : 0 },
    };
  },
});

const SCENES = [
  { label: 'Tumble', color: '#22314F' },
  { label: 'Flip', color: '#B5651D' },
  { label: 'Cube', color: '#2F6F62' },
  { label: 'Done', color: '#6B2D5C' },
];
const TUMBLE = { type: tumble, durationInFrames: 30 };
const FLIP = { type: flip({ perspective: 1600 }), durationInFrames: 30 };
const CUBE = { type: cube({ perspective: 1600 }), durationInFrames: 30, from: 'right' as const };
const { durationInFrames } = computeTransitionSeries([
  { durationInFrames: 45 }, TUMBLE, { durationInFrames: 60 }, FLIP, { durationInFrames: 60 }, CUBE, { durationInFrames: 45 },
]);

function Card({ label, color }: { label: string; color: string }) {
  const { width, height } = useVideoConfig();
  return (
    <>
      <Rect width={width} height={height} fill={color} />
      <Rect x={40} y={40} width={width - 80} height={height - 80} stroke="#FFFFFF66" strokeWidth={4} />
      <Text x={width / 2} y={height / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontSize: 140, fontWeight: 800, fill: { type: 'solid', color: '#FFFFFF' } }}>
        {label}
      </Text>
    </>
  );
}

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={durationInFrames}>
      <Rect width={1280} height={720} fill="#0B0B0F" />
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={45}><Card {...SCENES[0]} /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...TUMBLE} />
        <TransitionSeries.Sequence durationInFrames={60}><Card {...SCENES[1]} /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...FLIP} />
        <TransitionSeries.Sequence durationInFrames={60}><Card {...SCENES[2]} /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...CUBE} />
        <TransitionSeries.Sequence durationInFrames={45}><Card {...SCENES[3]} /></TransitionSeries.Sequence>
      </TransitionSeries>
    </Composition>
  );
}
