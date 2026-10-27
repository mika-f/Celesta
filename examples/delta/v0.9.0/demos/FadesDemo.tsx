import { Easings, Group, Rect, Text, interpolate } from '@celesta/react';
import { Polyline } from '@celesta/shapes';
import { C } from '../../components/theme';
import { BED, bedGain, fadeAt } from '../mix';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { scoreAt } from '../score';
import { BAR, DURATION, chapterAt } from '../timeline';
import { Lane, mono } from './common';

// FADES: this film's own bed, as the mix plays it. Across the top, the
// whole 86 seconds: the file's level (grey) and what is heard (orange),
// faded in and out by <Audio fadeIn fadeOut> and dipped under the three
// voices. Below, the two fades up close, and a clip whose sequence end
// moves: the fade-out follows it, with nothing to recompute.

const CHAPTER = 5;
const W = STAGE.width - 80;
const level = (frame: number) => Math.min(1, scoreAt(frame).rms * 3);

/** Bars of the bed between two composition frames, file level behind what is heard. */
function Bed({ x, y, width, height, from, to, count, now }:
  { x: number; y: number; width: number; height: number; from: number; to: number; count: number; now: number }) {
  const w = width / count;
  const points: [number, number][] = [];
  return (
    <Group x={x} y={y}>
      {Array.from({ length: count }, (_, i) => {
        const f = from + ((i + 0.5) / count) * (to - from);
        const raw = level(f) * height;
        const heard = raw * bedGain(f);
        points.push([i * w + w / 2, height - bedGain(f) * height]);
        const past = f <= now;
        return (
          <Group key={i} x={i * w}>
            <Rect y={height - raw} width={Math.max(1, w - 1.5)} height={raw} fill="line" />
            <Rect y={height - heard} width={Math.max(1, w - 1.5)} height={heard} fill={past ? 'orange' : '#F26A2E80'} />
          </Group>
        );
      })}
      <Polyline points={points} stroke={C.paper} strokeWidth={2} join="round" />
      {now >= from && now <= to ? <Rect x={((now - from) / (to - from)) * width - 1} y={-6} width={2} height={height + 12} fill="paper" /> : null}
    </Group>
  );
}

export function FadesDemo({ frame }: DemoProps) {
  const now = chapterAt(CHAPTER) + frame;
  const fadeInFrames = BED.fadeIn * 30;
  const fadeOutFrames = BED.fadeOut * 30;
  // The moving clip: its sequence ends at one of four lengths, a new one every two beats.
  const ends = [520, 760, 620, 880];
  const step = Math.max(0, Math.floor(frame / BAR));
  const end = interpolate(frame - step * BAR, [0, 10], [ends[Math.max(0, step - 1) % 4], ends[step % 4]],
    { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const clipX = 40;
  // The clip lane: 920 px for 240 frames, a clip with fadeIn={0.5} fadeOut={1}.
  const px = W / 240;
  const length = Math.round((end - clipX) / px);
  return (
    <Group>
      <Lane x={40} y={46} width={W} label={`score-0.9.0.wav, the whole film · gain now ${bedGain(now).toFixed(2)}`} />
      <Bed x={40} y={56} width={W} height={120} from={0} to={DURATION} count={180} now={now} />

      <Lane x={40} y={238} width={W / 2 - 20} label={`fadeIn={${BED.fadeIn}}`} />
      <Bed x={40} y={248} width={W / 2 - 20} height={110} from={0} to={fadeInFrames * 1.6} count={60} now={now} />
      <Lane x={40 + W / 2 + 20} y={238} width={W / 2 - 20} label={`fadeOut={${BED.fadeOut}}`} />
      <Bed x={40 + W / 2 + 20} y={248} width={W / 2 - 20} height={110} from={DURATION - fadeOutFrames * 1.6} to={DURATION} count={60} now={now} />
      <Text x={40} y={372} style={mono(15, C.dim)}>{`gain at the start ${fadeAt(0).toFixed(2)} · ${(fadeInFrames / 30).toFixed(1)} s to 1`}</Text>
      <Text x={40 + W} y={372} anchorX={1} style={mono(15, C.dim)}>{`${(fadeOutFrames / 30).toFixed(1)} s to 0 at the end`}</Text>

      <Group y={430}>
        <Text x={clipX} y={-12} anchorY={1} style={mono(16, C.grey)}>
          {`<Sequence durationInFrames={${length}}>  <Audio fadeIn={0.5} fadeOut={1} />`}
        </Text>
        <Rect x={clipX} width={end - clipX} height={90} cornerRadius={4} fill="raised" stroke="line" strokeWidth={2} />
        <Polyline points={[[clipX, 86], [clipX + 15 * px, 8], [end - 30 * px, 8], [end, 86]]} stroke={C.orange} strokeWidth={4} join="round" />
        <Rect x={end - 2} y={-8} width={4} height={106} fill="orange" />
      </Group>
    </Group>
  );
}
