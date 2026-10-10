import type { ReactNode } from 'react';
import { Easings, Group, Rect, Span, Text, interpolateColor, progress, useCue, useCurrentFrame } from '@celesta/react';
import { Copy, Tag } from '../components/Copy';
import { Stage } from '../components/Stage';
import { AREA, BEAT, C, COL, TOP, tint } from '../constants';
import { SPAN_COPY, SPAN_STYLE, segmentsOf, spanLayout } from '../measure';

// One beat per lit word; on the last cue every word lights at once.
const LIT = ['Same', 'sentence', 'Light', 'word', 'meaning', 'moves'];
const CUES = [...LIT.map((word, i) => ({ at: i * BEAT, words: [word] })), { at: BEAT * 6, words: LIT }];
const Y = TOP + 300;
const SIZE = SPAN_STYLE.fontSize ?? 92;
const PAD = 10;
// Inter's ascenders reach ~0.80 em above the baseline, descenders ~0.24 em below.
const ABOVE = 0.8;
const BELOW = 0.24;

// The paragraph with the given words wrapped in <Span>s. Only the colour
// changes, so every glyph keeps the advance it was measured with, and the
// paragraph wraps exactly as in the measured layout.
function lit(words: string[], color: string): ReactNode[] {
  const ranges = words.map((w) => [SPAN_COPY.indexOf(w), SPAN_COPY.indexOf(w) + w.length]).sort((a, b) => a[0] - b[0]);
  const out: ReactNode[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    out.push(SPAN_COPY.slice(at, start));
    out.push(<Span key={start} style={{ fill: color }}>{SPAN_COPY.slice(start, end)}</Span>);
    at = end;
  }
  out.push(SPAN_COPY.slice(at));
  return out;
}

export function Emphasis() {
  const f = useCurrentFrame();
  const cue = useCue(CUES);
  const words = cue?.cue.words ?? [];
  const sweep = progress(cue?.frame ?? 0, 0, 5, Easings.easeOutCubic);
  const segments = words.flatMap(segmentsOf);
  // The camera drifts towards the lit line.
  const focus = segments.length === 1 ? segments[0].y : spanLayout.height / 2;
  // Starts a few frames before the cut, so the scene's first frame has copy.
  const enter = progress(f, -4, 10, Easings.easeOutExpo);
  return (
    <Stage bg={C.ink} tone={tint(C.hot, 0.3)} fg={C.paper} accent={C.hot}
      tapes={['Light another word, and the meaning moves.', '<Span>']}
      y={AREA.height / 2 + (focus - spanLayout.height / 2) * 0.15} zoom={1.02}>
      <Tag x={COL.x} y={TOP + 140} color={C.hot} opacity={enter}>{'<Span style={{ fill }}>'}</Tag>
      <Group x={COL.x} y={Y + 30 * (1 - enter)} opacity={enter}>
        {/* Text and highlights both hang from the measured baselines. */}
        {segments.map((s, i) => (
          <Rect key={i} x={s.x - PAD} y={s.y + spanLayout.ascent - SIZE * ABOVE - PAD}
            width={(s.width + PAD * 2) * sweep} height={SIZE * (ABOVE + BELOW) + PAD * 2} fill={C.hot} />
        ))}
        <Text y={spanLayout.ascent} anchorY="baseline" maxWidth={COL.width}
          style={{ ...SPAN_STYLE, fill: { type: 'solid', color: C.grey } }}>
          {lit(words, interpolateColor(sweep, [0, 1], [C.paper, C.ink]))}
        </Text>
      </Group>
      <Copy x={COL.x} y={Y + spanLayout.height + 80} size={40} color={C.soft} lineHeight={56}
        maxWidth={COL.width} opacity={progress(f, BEAT, 8)}>
        Change one span and the paragraph still wraps, aligns and keeps its baseline as one.
      </Copy>
    </Stage>
  );
}
