import { clamp } from '@celesta.studio/math';
import { Easings, Rect, Text, progress, useCurrentFrame } from '@celesta.studio/react';
import { BEAT, C, H, W } from '../constants';

const WORDS: { text: string; bg: string; ax: number; size: number }[] = [
  { text: 'CODE', bg: C.paper, ax: 0.5, size: 360 },
  { text: 'IS', bg: C.ink, ax: 0, size: 360 },
  { text: 'THE', bg: C.paper, ax: 1, size: 360 },
  { text: 'CUT.', bg: C.accent, ax: 0.5, size: 400 },
  { text: 'EVERY', bg: C.ink, ax: 0, size: 330 },
  { text: 'FRAME', bg: C.paper, ax: 1, size: 330 },
  { text: 'A', bg: C.ink, ax: 0.5, size: 400 },
  { text: 'FUNCTION.', bg: C.accent, ax: 0.5, size: 270 },
];
const inkOn = (bg: string) => (bg === C.ink ? C.paper : C.ink);

export function Words() {
  const f = useCurrentFrame();
  const i = clamp(Math.floor(f / BEAT), 0, WORDS.length - 1);
  const local = f - i * BEAT;
  const word = WORDS[i];
  const fg = inkOn(word.bg);
  const punch = 1 + 0.14 * (1 - progress(local, 0, 9, Easings.easeOutExpo));
  const x = word.ax === 0 ? 120 : word.ax === 1 ? W - 120 : W / 2;
  const drift = (word.ax === 0 ? 1 : word.ax === 1 ? -1 : 0) * local * 1.2;
  return (
    <>
      <Rect width={W} height={H} fill={word.bg} />
      <Text x={x + drift} y={H / 2 + 10} fontFamily="display" fontSize={word.size} fontWeight={700} color={fg} anchorX={word.ax} anchorY={0.5} scale={punch}>
        {word.text}
      </Text>
      <Text x={120} y={H - 150} fontFamily="mono" fontSize={22} fontWeight={400} color={fg} opacity={0.6}>
        {`${String(i + 1).padStart(2, '0')}/${String(WORDS.length).padStart(2, '0')}`}
      </Text>
      <Rect x={120} y={H - 110} width={((i + local / BEAT) / WORDS.length) * 360} height={3} fill={fg} opacity={0.6} />
    </>
  );
}
