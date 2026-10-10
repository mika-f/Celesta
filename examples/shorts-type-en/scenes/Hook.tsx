import { Easings, Rect, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { Copy, Tag } from '../components/Copy';
import { FitStack, stackHeight } from '../components/FitStack';
import { Stage } from '../components/Stage';
import { BAR, BEAT, BPM, C, COL, TOP, tint } from '../constants';
import { hookLines } from '../measure';

const Y = TOP + 140;

// The hook has to land inside the first second: the reveal starts before
// frame 0, so the first frame already shows the opening line, and the whole
// headline is up by frame 12.
export function Hook() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: BPM });
  const height = stackHeight(hookLines);
  return (
    <Stage bg={C.ink} tone={tint(C.hot, 0.35)} fg={C.paper} accent={C.hot}
      tapes={['Write video in code.', 'React + TypeScript → MP4']}
      zoom={1 + 0.035 * progress(f, 0, BAR * 2) + 0.012 * pulse}>
      <Tag x={COL.x} y={Y - 64} opacity={progress(f, 4, 8)}>{'// film.tsx'}</Tag>
      <FitStack lines={hookLines} x={COL.x} y={Y} from={-8} stagger={3}
        glow={{ color: tint(C.hot, 0.45 + 0.4 * pulse), blur: 28 }} />
      {/* Every line already runs the full column, so the accent is a rule under it. */}
      <Rect x={COL.x} y={Y + height + 28} width={COL.width * progress(f, 10, 10, Easings.easeOutExpo)} height={10}
        fill={C.hot} />
      <Rect x={COL.x + COL.width - 34} y={Y + height + 28} width={34} height={10} fill={C.paper}
        opacity={Math.floor(f / (BEAT / 2)) % 2 === 0 ? progress(f, 20, 2) : 0} />
      <Copy x={COL.x} y={Y + height + 80} size={44} color={C.soft} lineHeight={60}
        maxWidth={COL.width} opacity={progress(f, BAR, 10)}>
        React and TypeScript, rendered to vertical video.
      </Copy>
      <Tag x={COL.x} y={Y + height + 240} color={C.hot} opacity={progress(f, BAR + BEAT, 10)}>
        {'1080 × 1920 · 30 fps · 120 BPM'}
      </Tag>
    </Stage>
  );
}
