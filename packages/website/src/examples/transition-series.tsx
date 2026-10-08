import {
  Audio, Composition, Easings, Rect, TransitionSeries,
  computeTransitionSeries, useTransitionVolume, useVideoConfig,
} from '@celesta/react';

const FADE = { type: 'crossfade', durationInFrames: 15 } as const;
const PUSH = { type: 'slide', durationInFrames: 20, from: 'right', easing: Easings.easeInOutCubic } as const;

// 90 + 120 + 90 frames, less the 15 and 20 the transitions share: 265.
const { durationInFrames } = computeTransitionSeries([
  { durationInFrames: 90 }, FADE, { durationInFrames: 120 }, PUSH, { durationInFrames: 90 },
]);

function Card({ color }: { color: string }) {
  const { width, height } = useVideoConfig();
  return (
    <>
      <Rect width={width} height={height} fill={color} />
      {/* Fades in and out with the picture. */}
      <Audio src="./room-tone.wav" volume={useTransitionVolume(0.6)} />
    </>
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={90}><Card color="#22314F" /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...FADE} />
        <TransitionSeries.Sequence durationInFrames={120}><Card color="#2F6F62" /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...PUSH} />
        <TransitionSeries.Sequence durationInFrames={90}><Card color="#B5651D" /></TransitionSeries.Sequence>
      </TransitionSeries>
    </Composition>
  );
}
