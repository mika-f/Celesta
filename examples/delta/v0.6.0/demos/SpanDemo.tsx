import { Easings, Group, Rect, Span, Text, interpolate } from '@celesta.studio/react';
import type { SpanStyle } from '@celesta.studio/react';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { BEAT, C, FONT, solid } from '../../components/theme';

// One <Text>. On each beat another word takes a span: a fill, a weight, a
// family. Then the wrapping width moves and the paragraph rewraps as one.
const STYLES: { word: string; prop: string; style: SpanStyle }[] = [
  { word: 'color', prop: 'fill', style: { fill: C.orange } },
  { word: 'weight', prop: 'fontWeight', style: { fill: C.paper, fontWeight: 700 } },
  { word: 'family', prop: 'fontFamily', style: { fill: C.blue, fontFamily: [...FONT.mono] } },
  { word: 'one paragraph', prop: 'fill', style: { fill: C.green, fontWeight: 700 } },
];

export function SpanDemo({ frame }: DemoProps) {
  const on = (i: number) => frame >= BEAT * (i + 1) - 4;
  const span = (i: number) => <Span style={on(i) ? STYLES[i].style : {}}>{STYLES[i].word}</Span>;
  const width = interpolate(frame, [76, 92, 104, 118], [780, 470, 470, 780],
    { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const mono = { fontFamily: FONT.mono, fontSize: 19 };
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Text x={40} y={44} anchorY={0.5} opacity={enter(frame, -4)} style={{ ...mono, fill: solid(C.dim) }}>
        {`<Text maxWidth={${Math.round(width)}}>`}
      </Text>
      <Rect x={40 + width} y={80} width={2} height={500} fill={C.red} opacity={enter(frame, 70)} />
      <Text x={40} y={96} maxWidth={width} opacity={enter(frame, -6, 20)}
        style={{ fontFamily: FONT.sans, fontSize: 52, lineHeight: 70, fill: solid(C.grey) }}>
        One text, many styles: {span(0)}, {span(1)} and {span(2)} change mid-line, and it still wraps as {span(3)}.
      </Text>
      {STYLES.slice(0, 3).map((s, i) => {
        const lit = on(i);
        const p = enter(frame, BEAT * (i + 1) - 4, 12);
        return (
          <Group key={s.prop} x={40 + i * 262} y={640}>
            <Rect width={244} height={64} cornerRadius={10} fill={lit ? '#FFFFFF0D' : C.ink}
              stroke={lit ? s.style.fill as string : C.line} strokeWidth={2} />
            <Text x={20} y={32} anchorY={0.5} opacity={0.4 + 0.6 * p}
              style={{ ...mono, fill: solid(lit ? s.style.fill as string : C.dim) }}>{`<Span ${s.prop}>`}</Text>
          </Group>
        );
      })}
    </Group>
  );
}
