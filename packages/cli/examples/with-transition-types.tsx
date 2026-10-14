import { Composition, Easings, Rect, Text, useCurrentFrame, useVideoConfig } from '@celesta/react';
import {
  TransitionSeries,
  clockWipe,
  computeTransitionSeries,
  cube,
  dissolve,
  iris,
  useTransitionSeriesScene,
  zoom,
} from '@celesta/transitions';
import type { TransitionSeriesTransitionProps } from '@celesta/transitions';

// Every built-in transition, in turn, each into a card named after it. All
// but the cut overlap their cards by 20 frames. The names alone use each
// transition's defaults; the calls show some of the options.
const SCENE = 50;
const EASE = Easings.easeInOutCubic;
const CARDS: { label: string; color: string; enter?: TransitionSeriesTransitionProps }[] = [
  { label: 'Start', color: '#22314F' },
  { label: 'cut', color: '#7A3B69', enter: { type: 'cut' } },
  { label: 'crossfade', color: '#2F6F62', enter: { type: 'crossfade', durationInFrames: 20 } },
  { label: 'slide', color: '#B5651D', enter: { type: 'slide', durationInFrames: 20, from: 'right', easing: EASE } },
  { label: 'wipe', color: '#3C5A99', enter: { type: 'wipe', durationInFrames: 20, easing: EASE } },
  { label: 'iris', color: '#8E3B46', enter: { type: iris({ feather: 40 }), durationInFrames: 20, easing: EASE } },
  { label: 'clock-wipe', color: '#4B7F52', enter: { type: clockWipe({ startAngle: 90 }), durationInFrames: 20 } },
  { label: 'zoom', color: '#5B4B8A', enter: { type: zoom({ scale: 2 }), durationInFrames: 20, easing: EASE } },
  { label: 'dissolve', color: '#A0522D', enter: { type: dissolve({ cellSize: 24 }), durationInFrames: 20 } },
  { label: 'blur', color: '#2E6E8E', enter: { type: 'blur', durationInFrames: 20 } },
  { label: 'flip', color: '#6B4226', enter: { type: 'flip', durationInFrames: 20, easing: EASE } },
  { label: 'cube', color: '#3D3B6E', enter: { type: cube({ shade: 0.6 }), durationInFrames: 20, from: 'right', easing: EASE } },
];

const { durationInFrames } = computeTransitionSeries(
  CARDS.flatMap(({ enter }) => [...(enter ? [enter] : []), { durationInFrames: SCENE }]),
);

function Card({ label, color }: { label: string; color: string }) {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const { index, enter } = useTransitionSeriesScene();
  const style = { fontFamily: 'sans-serif', align: 'center', fill: { type: 'solid', color: '#FFFFFF' } } as const;
  return (
    <>
      <Rect width={width} height={height} fill={color} />
      {/* A grid of dots, so a turn, a zoom, or a blur shows on the card itself. */}
      {Array.from({ length: 8 * 4 }, (_, dot) => (
        <Rect
          key={dot}
          x={120 + (dot % 8) * 148}
          y={110 + Math.floor(dot / 8) * 166}
          width={24}
          height={24}
          cornerRadius={12}
          fill="#FFFFFF33"
        />
      ))}
      <Text x={width / 2} y={height / 2 - 30} anchorX={0.5} anchorY={0.5} style={{ ...style, fontSize: 120, fontWeight: 800 }}>
        {label}
      </Text>
      <Text x={width / 2} y={height / 2 + 70} anchorX={0.5} anchorY={0.5} style={{ ...style, fontSize: 30 }}>
        {`card ${index + 1} of ${CARDS.length} · ${enter ? `in by ${enter.type}` : 'first card'} · frame ${frame}`}
      </Text>
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
