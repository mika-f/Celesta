import { Composition, Rect, useVideoConfig } from '@celesta/react';
import { TransitionSeries, computeTransitionSeries, defineTransition, iris } from '@celesta/transitions';

// Slats that open over the next scene while the previous one darkens.
const blinds = defineTransition({
  name: 'blinds',
  present: ({ progress, width, height }) => ({
    enter: {
      mask: (
        <>
          {Array.from({ length: 8 }, (_, i) => (
            <Rect key={i} y={(i * height) / 8} width={width} height={(height / 8) * progress} fill="#FFFFFF" />
          ))}
        </>
      ),
    },
    exit: { brightness: 1 - 0.5 * progress },
  }),
});

const BLINDS = { type: blinds, durationInFrames: 24 };
const IRIS = { type: iris({ center: { x: 0.3, y: 0.5 }, feather: 40 }), durationInFrames: 30 };

const { durationInFrames } = computeTransitionSeries([
  { durationInFrames: 75 }, BLINDS, { durationInFrames: 90 }, IRIS, { durationInFrames: 75 },
]);

function Card({ color }: { color: string }) {
  const { width, height } = useVideoConfig();
  return <Rect width={width} height={height} fill={color} />;
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={75}><Card color="#22314F" /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...BLINDS} />
        <TransitionSeries.Sequence durationInFrames={90}><Card color="#B5651D" /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...IRIS} />
        <TransitionSeries.Sequence durationInFrames={75}><Card color="#2F6F62" /></TransitionSeries.Sequence>
      </TransitionSeries>
    </Composition>
  );
}
