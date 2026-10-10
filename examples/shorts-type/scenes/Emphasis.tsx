import type { ReactNode } from 'react';
import { Easings, Group, Rect, Span, Text, interpolateColor, progress, useCue, useCurrentFrame } from '@celesta/react';
import { Copy, Tag } from '../components/Copy';
import { Stage } from '../components/Stage';
import { AREA, BEAT, C, COL, TOP } from '../constants';
import { SPAN_COPY, SPAN_STYLE, segmentsOf, spanLayout } from '../measure';

// One beat per lit phrase; on the last cue every phrase lights at once.
const LIT = ['同じ', '一文', '光らせる', '言葉', '意味', '変わる'];
const CUES = [...LIT.map((phrase, i) => ({ at: i * BEAT, phrases: [phrase] })), { at: BEAT * 6, phrases: LIT }];
const Y = TOP + 300;

// The paragraph with the given phrases wrapped in <Span>s. It stays one
// paragraph, so the lines wrap exactly as in the measured layout.
function lit(phrases: string[], color: string): ReactNode[] {
  const ranges = phrases.map((p) => [SPAN_COPY.indexOf(p), SPAN_COPY.indexOf(p) + p.length]).sort((a, b) => a[0] - b[0]);
  const out: ReactNode[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    out.push(SPAN_COPY.slice(at, start));
    out.push(<Span key={start} style={{ fill: color, fontWeight: 900 }}>{SPAN_COPY.slice(start, end)}</Span>);
    at = end;
  }
  out.push(SPAN_COPY.slice(at));
  return out;
}

export function Emphasis() {
  const f = useCurrentFrame();
  const cue = useCue(CUES);
  const phrases = cue?.cue.phrases ?? [];
  const sweep = progress(cue?.frame ?? 0, 0, 5, Easings.easeOutCubic);
  const segments = phrases.flatMap(segmentsOf);
  // The camera drifts towards the lit line.
  const focus = segments.length === 1 ? segments[0].y : spanLayout.height / 2;
  // Starts a few frames before the cut, so the scene's first frame has copy.
  const enter = progress(f, -4, 10, Easings.easeOutExpo);
  return (
    <Stage bg={C.ink} y={AREA.height / 2 + (focus - spanLayout.height / 2) * 0.15} zoom={1.02}>
      <Tag x={COL.x} y={TOP + 140} color={C.hot} opacity={enter}>{'<Span style={{ fontWeight: 900 }}>'}</Tag>
      <Group x={COL.x} y={Y + 30 * (1 - enter)} opacity={enter}>
        {segments.map((s, i) => (
          <Rect key={i} x={s.x - 8} y={s.y + spanLayout.lineHeight * 0.1} width={(s.width + 16) * sweep}
            height={spanLayout.lineHeight * 0.82} fill={C.hot} />
        ))}
        <Text maxWidth={COL.width} style={{ ...SPAN_STYLE, fill: { type: 'solid', color: C.grey }, lineBreak: 'phrase' }}>
          {lit(phrases, interpolateColor(sweep, [0, 1], [C.paper, C.ink]))}
        </Text>
      </Group>
      <Copy x={COL.x} y={Y + spanLayout.height + 80} size={40} weight={400} color={C.soft} lineHeight={60}
        maxWidth={COL.width} opacity={progress(f, BEAT, 8)}>
        強調だけを変えても、改行も基準線も一つの段落のまま。
      </Copy>
    </Stage>
  );
}
