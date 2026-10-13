import { Easings, Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { useTypewriter } from '@celesta/text';
import { BallStage } from '../components/BallStage';
import { Headline } from '../components/Headline';
import { Label } from '../components/Label';
import { BAR, BEAT, C, W } from '../constants';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

// A lower third sliding in on a rule.
function TitleCard({ f }: { f: number }) {
  const t = interpolate(f, [0, 8], [0, 1], { ...clamp, easing: Easings.easeOutCubic });
  return (
    <Group x={960 - 40 * (1 - t)} opacity={t}>
      <Rect x={0} y={520} width={8} height={132} fill={C.accent} />
      <Label x={40} y={520} size={64} font="display" weight={700}>Ada Lovelace</Label>
      <Label x={40} y={612} size={28} color={C.muted}>Analyst, 1843</Label>
      <Rect x={0} y={700} width={800 * t} height={1} fill={C.line} />
    </Group>
  );
}

// Five bars growing from the baseline.
function BarChart({ f }: { f: number }) {
  const { fps } = useVideoConfig();
  const heights = [180, 320, 240, 420, 360];
  return (
    <>
      <Rect x={960} y={900} width={680} height={1} fill={C.line} />
      {heights.map((h, i) => {
        const s = spring({ frame: f, fps, delay: i, config: { stiffness: 260, damping: 18 } });
        return <Rect key={i} x={960 + i * 140} y={900 - h * s} width={100} height={h * s} fill={C.accent} />;
      })}
    </>
  );
}

// A caption band, typed to the frame.
function Caption({ f }: { f: number }) {
  const line = 'Captions, timed to the frame.';
  const frame = useCurrentFrame();
  const { text } = useTypewriter(line, { from: frame - f, framesPerChar: 0.4 });
  return (
    <>
      <Rect x={160} y={820} width={W - 320} height={120} fill={C.panel} />
      <Rect x={160} y={820} width={W - 320} height={1} fill={C.line} />
      <Label x={W / 2} y={880} size={40} ax={0.5} ay={0.5}>{text}</Label>
    </>
  );
}

const VIGNETTES = [
  { word: 'Titles.', Picture: TitleCard },
  { word: 'Charts.', Picture: BarChart },
  { word: 'Captions.', Picture: Caption },
];

// 0–6 s. The ball bounces for a bar, then three motion vignettes, one beat
// each, play twice; the ball returns for the last two beats so the Reveal
// starts from it.
export function ColdOpen() {
  const frame = useCurrentFrame();
  const montage = frame >= BAR && frame < BAR + BEAT * 6;
  if (!montage) return <BallStage frame={frame} />;
  const beat = Math.floor((frame - BAR) / BEAT);
  const { word, Picture } = VIGNETTES[beat % VIGNETTES.length];
  return (
    <>
      <Headline x={160} y={160}>{word}</Headline>
      <Picture f={(frame - BAR) % BEAT} />
    </>
  );
}
