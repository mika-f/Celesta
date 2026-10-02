import { Easings, Rect, progress, useCue } from '@celesta/react';
import { Label } from '../components/Label';
import { Ring } from '../components/Ring';
import { ACID, BONE, CYAN, H, INK, MAG, W } from '../constants';
import { mix } from '../math';

const WORDS = ['WRITE', 'CODE.', 'RENDER', 'FRAMES.', 'MOTION', 'IS', 'MATH.', 'LAYER', 'BY', 'LAYER.',
  'NO', 'TIMELINE', 'JUST', 'FUNCTIONS', 'OF', 'TIME.'];
const SWATCH = [
  { bg: BONE, fg: INK }, { bg: ACID, fg: INK }, { bg: INK, fg: BONE }, { bg: MAG, fg: INK },
  { bg: INK, fg: ACID }, { bg: CYAN, fg: INK },
];
const CUES = WORDS.map((word, i) => ({ at: i * 15, word, ...SWATCH[i % SWATCH.length] }));
export function Words() {
  const cue = useCue(CUES);
  if (!cue) return <Rect width={W} height={H} fill={INK} />;
  const { cue: c, previous, frame, index } = cue;
  const wipe = progress(frame, 0, 8, Easings.easeOutExpo);
  const pop = progress(frame, 0, 10, Easings.easeOutBack);
  return <>
    <Rect width={W} height={H} fill={previous ? previous.bg : INK} />
    <Rect width={W * wipe} height={H} fill={c.bg} />
    <Ring r={mix(200, 760, progress(frame, 0, 15, Easings.easeOutCubic))} color={c.fg} opacity={0.35 * (1 - progress(frame, 0, 15))} />
    <Label x={W / 2} y={H / 2 + 10} size={mix(300, 420, pop)} color={c.fg} anchorX={0.5} anchorY={0.5}
      scale={0.88 + 0.12 * pop}>{c.word}</Label>
    <Label x={72} y={64} size={28} mono color={c.fg}>{`${String(index + 1).padStart(2, '0')} / ${WORDS.length}`}</Label>
    <Rect x={72} y={H - 80} width={(W - 144) * ((index + progress(frame, 0, 15)) / WORDS.length)} height={4} fill={c.fg} />
  </>;
}
