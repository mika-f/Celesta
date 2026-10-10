import { Easings, Group, Rect, Text, progress, useCue, useCurrentFrame } from '@celesta/react';
import { Copy, Tag, textStyle } from '../components/Copy';
import { Stage } from '../components/Stage';
import { BEAT, C, COL, TOP, tint } from '../constants';
import { measured } from '../measure';

// A new word every other beat, fitted to the column and drawn over its own
// measurement: one box per glyph advance, the line's top, baseline and
// bottom, and the total width.
const STOPS = measured.map((_, i) => ({ at: i * BEAT * 2, index: i }));
const Y = TOP + 330;

export function Measure() {
  const f = useCurrentFrame();
  const stop = useCue(STOPS);
  const local = stop?.frame ?? f;
  // Read at render time: prepare() replaces the estimates before frame 0.
  const word = measured[stop?.cue.index ?? 0];
  const m = word.metrics;
  const pop = 1 + 0.05 * (1 - progress(local, 0, 8, Easings.easeOutCubic));
  const span = progress(local, 2, 10, Easings.easeOutExpo);
  const guide = (y: number, color: string, height = 2) =>
    <Rect x={-16} y={y} width={(COL.width + 32) * span} height={height} fill={color} />;
  return (
    <Stage bg={C.ink} tone={tint(C.grey, 0.3)} fg={C.paper} accent={C.hot}
      tapes={['Every glyph, measured.', 'measureText()']} fade={0.62}>
      <Copy x={COL.x} y={TOP + 10} size={72} weight={700} lineHeight={84} opacity={progress(f, -4, 8)}>
        {'Every glyph,\nmeasured.'}
      </Copy>
      <Tag x={COL.x} y={TOP + 220} color={C.hot} opacity={progress(f, 0, 8)}>{'measureText(word, style)'}</Tag>
      <Group x={COL.x} y={Y} scale={pop} opacity={progress(f, -2, 6)}>
        {guide(0, tint(C.paper, 0.25))}
        {guide(m.ascent, C.hot, 3)}
        {guide(m.lineHeight, tint(C.paper, 0.25))}
        {m.glyphs.map((g, i) => (
          <Group key={i} x={g.x} opacity={progress(local, 3 + i * 1.5, 4)}>
            <Rect width={g.width} height={m.lineHeight} stroke={tint(C.paper, 0.45)} strokeWidth={2} />
            <Copy x={g.width / 2} y={m.lineHeight + 14} ax={0.5} size={18} weight={500} font="mono" color={C.soft}>
              {String(Math.round(g.width))}
            </Copy>
          </Group>
        ))}
        <Text y={m.ascent} anchorY="baseline" style={textStyle(word.size, 'display')}>{word.text}</Text>
        {/* Capitals have no descenders, so the label fits just under the baseline. */}
        <Copy x={COL.width - 8} y={m.ascent + 8} ax={1} size={20} weight={500} font="mono" color={C.hot}
          opacity={span}>
          baseline
        </Copy>
        <Group y={m.lineHeight + 80}>
          <Rect width={m.width * span} height={3} fill={C.hot} />
          <Rect x={-1} y={-12} width={3} height={27} fill={C.hot} />
          <Rect x={m.width * span - 2} y={-12} width={3} height={27} fill={C.hot} opacity={span} />
          <Copy y={30} size={26} weight={500} font="mono" color={C.paper} opacity={span}>
            {`width ${Math.round(m.width)} px · fontSize ${word.size}`}
          </Copy>
        </Group>
      </Group>
      <Copy x={COL.x} y={Y + 560} size={38} color={C.soft} lineHeight={54} maxWidth={COL.width}
        opacity={progress(f, BEAT, 8)}>
        The renderer's own shaping, so the layout and the pixels agree.
      </Copy>
    </Stage>
  );
}
