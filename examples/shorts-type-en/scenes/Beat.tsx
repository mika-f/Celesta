import { Easings, Group, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Burst } from '../components/Burst';
import { Copy } from '../components/Copy';
import { Stage } from '../components/Stage';
import { AREA, BEAT, BPM, C, COL, TOP, tint } from '../constants';
import { beatWords } from '../measure';

// One word per beat, each at the size fitText() found for it (measure.ts).
const LOOKS: { bg: string; fg: string; ax: number; y: number }[] = [
  { bg: C.hot, fg: C.ink, ax: 0, y: 440 },
  { bg: C.ink, fg: C.paper, ax: 1, y: 640 },
  { bg: C.paper, fg: C.ink, ax: 0, y: 520 },
  { bg: C.sun, fg: C.ink, ax: 1, y: 760 },
  { bg: C.ink, fg: C.paper, ax: 0, y: 440 },
  { bg: C.paper, fg: C.ink, ax: 1, y: 620 },
  { bg: C.hot, fg: C.ink, ax: 0, y: 560 },
  { bg: C.ink, fg: C.sun, ax: 0.5, y: 600 },
];

export function Beat() {
  const f = useCurrentFrame();
  const { beat, pulse } = useBeat({ bpm: BPM });
  const i = Math.min(beat, LOOKS.length - 1);
  const look = LOOKS[i];
  const word = beatWords[i];
  const drop = progress(f - i * BEAT, 0, 6, Easings.easeOutExpo);
  const x = COL.x + COL.width * look.ax;
  const accent = look.bg === C.hot ? C.ink : C.hot;
  const middle = x + (0.5 - look.ax) * word.width;
  return (
    <Stage bg={look.bg} tone={tint(look.fg, 0.2)} fg={look.fg} accent={accent}
      tapes={['Cut on the beat.', 'useBeat()']}
      rotation={i % 2 === 0 ? -1.5 : 1.5} zoom={1 + 0.025 * pulse} shake={3}>
      <Burst x={middle} y={TOP + look.y} t={(f - i * BEAT) / BEAT} color={accent} seed={i} />
      <Group blur={18 * (1 - drop)}>
        <Copy x={x} y={TOP + look.y - 60 * (1 - drop)} ax={look.ax} ay={0.5} size={word.size} font="display"
          color={look.fg} scale={1 + 0.05 * pulse}>
          {word.text}
        </Copy>
      </Group>
      <Copy x={COL.x} y={AREA.height - 60} size={24} weight={700} font="mono" color={look.fg} opacity={0.7}>
        {`useBeat() → beat ${beat}`}
      </Copy>
      <Rect x={COL.x} y={AREA.height - 24} width={((i + 1) / LOOKS.length) * COL.width} height={4} fill={look.fg}
        opacity={0.7} />
    </Stage>
  );
}
