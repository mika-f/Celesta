import type { Run } from '../components/Mono';
import { Easings, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { Mono, runLength } from '../components/Mono';
import { BAR, BEAT, C, H, W } from '../constants';
import { clamp, progress } from '../helpers';
import { monoAdvance } from '../metrics';

// Keep in sync with TYPED_CHUNKS in assets/celesta-reel/make-music.py, which
// puts a key click on every typed character.
const TYPED: Run[][] = [
  [{ text: 'video', color: C.paper }],
  [{ text: ' = ', color: C.grey }, { text: 'f', color: C.accent }, { text: '(', color: C.grey }],
  [{ text: 'frame', color: C.paper }, { text: ')', color: C.grey }],
];

export function Intro() {
  const f = useCurrentFrame();
  const size = 72;
  const runs = TYPED.flat();
  const length = runLength(runs);
  const x0 = W / 2 - (length * size * monoAdvance) / 2;
  // Chunk k starts on beat k and types one character per frame.
  const typed = TYPED.reduce((n, chunk, k) => n + clamp(f - k * BEAT + 1, 0, runLength(chunk)), 0);
  const caretOn = typed < length || Math.floor(f / 4) % 2 === 0;

  const push = interpolate(f, [0, BAR], [1, 1.08], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const rule = progress(f, 40, 10, Easings.easeInOutExpo);
  const open = progress(f, 50, 10, Easings.easeInExpo);

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group x={W / 2} y={H / 2} scale={push} opacity={1 - progress(f, 50, 6)}>
        <Group x={-W / 2} y={-H / 2}>
          <Mono runs={runs} x={x0} y={H / 2} size={size} visible={typed} />
          {caretOn && (
            <Rect x={x0 + typed * size * monoAdvance + 4} y={H / 2 - 52} width={6} height={82} fill={C.accent} />
          )}
        </Group>
      </Group>
      <Rect x={W / 2} y={H / 2 + 90} anchorX={0.5} anchorY={0.5}
        width={Math.max(1, W * rule)} height={Math.max(1, 3 + (H + 200) * open)}
        fill={open > 0 ? C.paper : C.accent} />
    </>
  );
}
