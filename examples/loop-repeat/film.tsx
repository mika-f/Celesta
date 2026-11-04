import { Audio, Composition, Group, Loop, Rect, Text, interpolate, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
// One beat at 120 BPM, and the phrase make-phrase.py writes: four beats.
const BEAT = 15;
const PHRASE = 4 * BEAT;
const INK = '#14161C';
const PAPER = '#F2EEE6';
const ACCENT = '#E4572E';

/** One ring growing and fading over the cycle `<Loop>` gives it. */
function Ring() {
  const frame = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const radius = interpolate(frame, [0, durationInFrames], [40, 420]);
  return <Rect x={WIDTH / 2} y={HEIGHT / 2} width={radius * 2} height={radius * 2}
    anchorX={0.5} anchorY={0.5} cornerRadius={radius}
    stroke={PAPER} strokeWidth={6}
    opacity={interpolate(frame, [0, durationInFrames], [0.9, 0], { extrapolateRight: 'clamp' })} />;
}

/** A dot that drops on every beat. */
function Beat() {
  const frame = useCurrentFrame();
  const scale = interpolate(frame, [0, 6, BEAT], [1.6, 1, 1], { extrapolateRight: 'clamp' });
  return <Rect x={WIDTH / 2} y={HEIGHT / 2} width={48} height={48} anchorX={0.5} anchorY={0.5}
    cornerRadius={24} scale={scale} fill={ACCENT} />;
}

export default function LoopRepeat() {
  return <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={12 * FPS}>
    <Rect width={WIDTH} height={HEIGHT} fill={INK} />
    {/* The background: a new ring every phrase, offset by half a phrase. */}
    <Loop durationInFrames={PHRASE}><Ring /></Loop>
    <Loop from={PHRASE / 2} durationInFrames={PHRASE}><Ring /></Loop>
    <Loop durationInFrames={BEAT}><Beat /></Loop>
    {/* The phrase plays five and a half times; the last half is cut. */}
    <Loop from={FPS} durationInFrames={PHRASE} times={5.5}>
      <Audio src="./assets/phrase.wav" />
    </Loop>
    <Group x={48} y={40}>
      <Text style={{ fontSize: 28, fill: { type: 'solid', color: PAPER } }}>Loop</Text>
    </Group>
  </Composition>;
}
