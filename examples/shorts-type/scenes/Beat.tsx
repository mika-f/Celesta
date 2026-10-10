import { Easings, Group, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Burst } from '../components/Burst';
import { Copy } from '../components/Copy';
import { Stage } from '../components/Stage';
import { AREA, BEAT, BPM, C, COL, TOP, tint } from '../constants';

// One word per beat. Sizes are picked so every word fits the column on one
// line with room for the punch (characters × size ≤ 720).
const WORDS: { text: string; size: number; weight: number; bg: string; fg: string; ax: number; y: number }[] = [
  { text: '拍に', size: 320, weight: 900, bg: C.hot, fg: C.ink, ax: 0, y: 420 },
  { text: '合わせて', size: 176, weight: 400, bg: C.ink, fg: C.paper, ax: 1, y: 640 },
  { text: '言葉を', size: 224, weight: 900, bg: C.paper, fg: C.ink, ax: 0, y: 520 },
  { text: '落とす。', size: 176, weight: 900, bg: C.sun, fg: C.ink, ax: 1, y: 780 },
  { text: '書いて、', size: 170, weight: 900, bg: C.ink, fg: C.paper, ax: 0, y: 440 },
  { text: '保存して、', size: 140, weight: 400, bg: C.paper, fg: C.ink, ax: 1, y: 620 },
  { text: 'すぐ', size: 340, weight: 900, bg: C.hot, fg: C.ink, ax: 0, y: 560 },
  { text: '確かめる。', size: 140, weight: 900, bg: C.ink, fg: C.sun, ax: 0.5, y: 600 },
];

export function Beat() {
  const f = useCurrentFrame();
  const { beat, pulse } = useBeat({ bpm: BPM });
  const i = Math.min(beat, WORDS.length - 1);
  const word = WORDS[i];
  const drop = progress(f - i * BEAT, 0, 6, Easings.easeOutExpo);
  const x = COL.x + COL.width * word.ax;
  const accent = word.bg === C.hot ? C.ink : C.hot;
  // The word's middle, for the speed lines (one em per Japanese glyph).
  const middle = x + (0.5 - word.ax) * word.text.length * word.size;
  return (
    <Stage bg={word.bg} tone={tint(word.fg, 0.2)} fg={word.fg} accent={accent}
      tapes={['拍に合わせて、言葉を落とす。', 'useBeat()']}
      rotation={i % 2 === 0 ? -1.5 : 1.5} zoom={1 + 0.025 * pulse} shake={3}>
      <Burst x={middle} y={TOP + word.y} t={(f - i * BEAT) / BEAT} color={accent} seed={i} />
      <Group blur={18 * (1 - drop)}>
        <Copy x={x} y={TOP + word.y - 60 * (1 - drop)} ax={word.ax} ay={0.5} size={word.size} weight={word.weight}
          color={word.fg} scale={1 + 0.05 * pulse}>
          {word.text}
        </Copy>
      </Group>
      <Copy x={COL.x} y={AREA.height - 60} size={24} weight={700} font="mono" color={word.fg} opacity={0.7}>
        {`useBeat() → beat ${beat}`}
      </Copy>
      <Rect x={COL.x} y={AREA.height - 24} width={((i + 1) / WORDS.length) * COL.width} height={4} fill={word.fg}
        opacity={0.7} />
    </Stage>
  );
}
