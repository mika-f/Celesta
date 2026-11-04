import { Easings, Rect, Span, progress, useBeat, useCurrentFrame } from '@celesta.studio/react';
import { TextReveal } from '@celesta.studio/text';
import { Copy, Tag, textStyle } from '../components/Copy';
import { Stage } from '../components/Stage';
import { TAPES } from '../tapes';
import { BAR, BEAT, BPM, C, COL, TOP, tint } from '../constants';

// Back to the opening line on the opening background, so when a player
// loops, the return to the hook reads as one more cut. It is not seamless:
// the tapes, the halftone and the punch-in restart there as on every cut.
export function Outro() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: BPM });
  const mark = progress(f, BEAT * 2, 12, Easings.easeOutExpo);
  return (
    <Stage bg={C.ink} tone={tint(C.hot, 0.35)} fg={C.paper} accent={C.hot}
      tapes={TAPES.outro} zoom={1.03 - 0.03 * progress(f, 0, BAR * 2, Easings.easeOutCubic)}>
      <TextReveal x={COL.x} y={TOP + 180} lineHeight={124} baseline={0.8} from={-6} stagger={BEAT}
        glow={{ color: tint(C.hot, 0.35 + 0.4 * pulse), blur: 24 }} style={textStyle(104, 900)}>
        {'動画を、\n'}<Span style={{ fill: C.hot }}>コード</Span>{'で書く。'}
      </TextReveal>
      <Rect x={COL.x} y={TOP + 500} width={COL.width * mark} height={4} fill={C.dim} />
      <Copy x={COL.x} y={TOP + 560 + 40 * (1 - mark)} size={176} weight={700} font="mono" opacity={mark}
        scale={1 + 0.015 * pulse}>
        Celesta
      </Copy>
      <Tag x={COL.x + 6} y={TOP + 800} color={C.soft} opacity={progress(f, BEAT * 4, 10)}>
        {'examples/shorts-type/film.tsx'}
      </Tag>
    </Stage>
  );
}
