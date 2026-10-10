import { Rect, Span, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { TextReveal } from '@celesta/text';
import { Copy, Tag, textStyle } from '../components/Copy';
import { Stage } from '../components/Stage';
import { BAR, BEAT, BPM, C, COL, TOP } from '../constants';
import { HOOK_SIZE, hookCaretX } from '../measure';

const LINE = 196;
const Y = TOP + 140;

// The hook has to land inside the first second: the reveal starts before
// frame 0, so the first frame already shows the opening line, and the whole
// headline is up by frame 12.
export function Hook() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: BPM });
  const caretOn = f < BEAT * 2 || Math.floor(f / (BEAT / 2)) % 2 === 0;
  return (
    <Stage bg={C.ink} zoom={1 + 0.035 * progress(f, 0, BAR * 2) + 0.012 * pulse}>
      <Tag x={COL.x} y={Y - 64} opacity={progress(f, 4, 8)}>{'// film.tsx'}</Tag>
      <TextReveal x={COL.x} y={Y} lineHeight={LINE} baseline={0.8} from={-8} stagger={3} durationInFrames={14}
        style={textStyle(HOOK_SIZE, 900)}>
        {'動画を、\n'}<Span style={{ fill: C.hot }}>コード</Span>{'で\n書く。'}
      </TextReveal>
      <Rect x={COL.x + hookCaretX + 12} y={Y + LINE * 2 + 30} width={18} height={LINE * 0.72}
        fill={C.hot} opacity={caretOn ? progress(f, 10, 4) : 0} />
      <Copy x={COL.x} y={Y + LINE * 3 + 80} size={46} weight={400} color={C.soft} lineHeight={66}
        maxWidth={COL.width} opacity={progress(f, BAR, 10)}>
        React と TypeScript で、縦長のショート動画を。
      </Copy>
      <Tag x={COL.x} y={Y + LINE * 3 + 240} color={C.hot} opacity={progress(f, BAR + BEAT, 10)}>
        {'1080 × 1920 · 30 fps · 120 BPM'}
      </Tag>
    </Stage>
  );
}
