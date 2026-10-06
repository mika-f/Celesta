// SPECTRA: a performance reel that puts most of Celesta's renderer to work
// in one 11.5-second, 60 fps film. See README.md.
import { Assets, Composition, Font, Group, Rect, Series, Text, Transition, computeSeries, frameToTimecode, useCurrentFrame } from '@celesta/react';

import { Geometry } from './scenes/Geometry';
import { Ignition } from './scenes/Ignition';
import { Signal } from './scenes/Signal';
import { Source } from './scenes/Source';
import { Typography } from './scenes/Typography';
import { CHAPTER, CHAPTERS, DIM, DURATION, FPS, H, MONO, OVERLAP, W, style } from './shared';

const SCENES = [Ignition, Geometry, Typography, Source, Signal];
const TIMING = computeSeries(SCENES.map((_, i) => ({ durationInFrames: CHAPTER, offset: i === 0 ? 0 : -OVERLAP })));

function Background() {
  const frame = useCurrentFrame();
  const k = 0.5 + 0.5 * Math.sin(frame / FPS * 0.6);
  return (
    <Rect width={W} height={H} fill={{
      type: 'radial', center: { x: W / 2, y: H * (0.4 + 0.1 * k) }, radius: W * 0.75,
      stops: [{ offset: 0, color: '#1A1F4A' }, { offset: 0.55, color: '#0B0E24' }, { offset: 1, color: '#04050C' }],
    }} />
  );
}

/** Chapter label, timecode and progress, drawn over every chapter. */
function Hud() {
  const frame = useCurrentFrame();
  const chapter = TIMING.sequences.reduce((last, sequence, i) => (frame >= sequence.from ? i : last), 0);
  return (
    <Group>
      <Rect width={W} height={H} blendMode="multiply" fill={{
        type: 'radial', center: { x: W / 2, y: H / 2 }, radius: W * 0.62,
        stops: [{ offset: 0.6, color: '#FFFFFF' }, { offset: 1, color: '#5A6080' }],
      }} />
      <Text x={40} y={44} anchorY="baseline" style={style(MONO, 18, DIM, { letterSpacing: 4 })}>
        {`${String(chapter + 1).padStart(2, '0')} / ${CHAPTERS[chapter]}`}
      </Text>
      <Text x={W - 40} y={44} anchorX={1} anchorY="baseline" style={style(MONO, 18, DIM, { letterSpacing: 2 })}>
        {`${frameToTimecode(frame, FPS)}  F${String(frame).padStart(4, '0')}`}
      </Text>
      <Rect x={40} y={H - 12} width={W - 80} height={3} fill="#FFFFFF18" />
      <Rect x={40} y={H - 12} width={(W - 80) * (frame / (DURATION - 1))} height={3} fill="#7B5CFF" />
    </Group>
  );
}

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION}>
      <Assets>
        <Font src="../versus/bench/assets/fonts/BebasNeue-Regular.ttf" />
        <Font src="../versus/bench/assets/fonts/IBMPlexMono-Regular.ttf" />
      </Assets>
      <Background />
      <Series>
        {SCENES.map((Scene, i) => (
          <Series.Sequence key={CHAPTERS[i]} durationInFrames={CHAPTER} offset={i === 0 ? 0 : -OVERLAP}>
            <Transition type={['fade', 'scale']} scaleFrom={0.96} durationInFrames={OVERLAP} direction="in">
              <Transition type="fade" durationInFrames={OVERLAP} direction="out">
                <Scene />
              </Transition>
            </Transition>
          </Series.Sequence>
        ))}
      </Series>
      <Hud />
    </Composition>
  );
}
