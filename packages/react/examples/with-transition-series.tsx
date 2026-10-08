import {
  Audio,
  Composition,
  Easings,
  Rect,
  Text,
  TransitionSeries,
  computeTransitionSeries,
  useCurrentFrame,
  useTransitionSeriesScene,
  useTransitionVolume,
  useVideoConfig,
} from '@celesta/react';
import type { TransitionSeriesTransitionProps } from '@celesta/react';

// Five cards, each entered a different way: a cut, a cross-fade, a slide,
// and a wipe. Every transition overlaps the cards on either side, so the
// film is 5 × 60 − 3 × 20 = 240 frames, not 300.
const SCENE = 60;
const CARDS: { label: string; color: string; enter?: TransitionSeriesTransitionProps }[] = [
  { label: 'Start', color: '#22314F' },
  { label: 'Cut', color: '#7A3B69', enter: { type: 'cut' } },
  { label: 'Crossfade', color: '#2F6F62', enter: { type: 'crossfade', durationInFrames: 20 } },
  {
    label: 'Slide',
    color: '#B5651D',
    enter: { type: 'slide', durationInFrames: 20, from: 'right', easing: Easings.easeInOutCubic },
  },
  { label: 'Wipe', color: '#3C5A99', enter: { type: 'wipe', durationInFrames: 20, easing: Easings.easeInOutSine } },
];

const { durationInFrames } = computeTransitionSeries(
  CARDS.flatMap(({ enter }) => [...(enter ? [enter] : []), { durationInFrames: SCENE }]),
);

function Card({ label, color }: { label: string; color: string }) {
  // Counts from this card's own first frame, the first frame of its
  // transition in.
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const { enter } = useTransitionSeriesScene();
  // Each card's sound fades in and out with its picture.
  const volume = useTransitionVolume(0.8);
  const caption = enter ? `in: ${enter.type}, ${enter.durationInFrames} frames` : 'first scene';
  const style = { fontFamily: 'sans-serif', align: 'center', fill: { type: 'solid', color: '#FFFFFF' } } as const;
  return (
    <>
      <Rect width={width} height={height} fill={color} />
      <Text x={width / 2} y={height / 2 - 40} anchorX={0.5} anchorY={0.5} style={{ ...style, fontSize: 96 }}>
        {label}
      </Text>
      <Text x={width / 2} y={height / 2 + 60} anchorX={0.5} anchorY={0.5} style={{ ...style, fontSize: 32 }}>
        {`${caption} · frame ${frame} of ${SCENE}`}
      </Text>
      <Audio src="./voice.wav" volume={volume} />
    </>
  );
}

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={durationInFrames}>
      <TransitionSeries>
        {CARDS.flatMap(({ label, color, enter }) => [
          ...(enter ? [<TransitionSeries.Transition key={`${label}-in`} {...enter} />] : []),
          <TransitionSeries.Sequence key={label} durationInFrames={SCENE}>
            <Card label={label} color={color} />
          </TransitionSeries.Sequence>,
        ])}
      </TransitionSeries>
    </Composition>
  );
}
