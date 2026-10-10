import { Easings, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Copy, Tag } from '../components/Copy';
import { FitStack, stackHeight } from '../components/FitStack';
import { Stage } from '../components/Stage';
import { BAR, BEAT, BPM, C, COL, TOP, tint } from '../constants';
import { hookLines } from '../measure';

const Y = TOP + 180;

// Back to the opening line on the opening background, so a looping player
// cuts from here into the hook without a jump.
export function Outro() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: BPM });
  const mark = progress(f, BEAT * 2, 12, Easings.easeOutExpo);
  const rule = Y + stackHeight(hookLines) + 60;
  return (
    <Stage bg={C.ink} tone={tint(C.hot, 0.35)} fg={C.paper} accent={C.hot}
      tapes={['Write your video in code.', 'Celesta']} zoom={1.03 - 0.03 * progress(f, 0, BAR * 2, Easings.easeOutCubic)}>
      <FitStack lines={hookLines} x={COL.x} y={Y} from={-6} stagger={BEAT}
        glow={{ color: tint(C.hot, 0.35 + 0.4 * pulse), blur: 24 }} />
      <Rect x={COL.x} y={rule} width={COL.width * mark} height={4} fill={C.dim} />
      <Copy x={COL.x} y={rule + 60 + 40 * (1 - mark)} size={176} weight={700} font="mono" opacity={mark}
        scale={1 + 0.015 * pulse}>
        Celesta
      </Copy>
      <Tag x={COL.x + 6} y={rule + 300} color={C.soft} opacity={progress(f, BEAT * 4, 10)}>
        {'examples/shorts-type-en/film.tsx'}
      </Tag>
    </Stage>
  );
}
