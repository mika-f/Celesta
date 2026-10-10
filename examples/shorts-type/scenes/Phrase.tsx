import { Easings, Group, Rect, Span, Text, progress, useCue, useCurrentFrame } from '@celesta/react';
import { TextReveal } from '@celesta/text';
import { Copy, Tag, textStyle } from '../components/Copy';
import { Stage } from '../components/Stage';
import { AREA, BEAT, C, COL, TOP } from '../constants';

const COPY = '長い一文を狭い画面に流し込んでも、言葉の途中では折れません。';
// Every other beat the column narrows; both panels re-wrap at the same width.
const WIDTHS = [760, 600, 480, 400].map((width, i) => ({ at: i * BEAT * 2, width }));
const SIZE = 56;
const LINE = 72;
const PANEL = LINE * 6;

function Panel({ y, mode, width, color, note }:
  { y: number; mode: 'normal' | 'phrase'; width: number; color: string; note: string }) {
  const accent = mode === 'phrase' ? C.hot : C.grey;
  return (
    <Group y={y}>
      <Tag x={COL.x} color={accent}>{`lineBreak: '${mode}'`}</Tag>
      <Copy x={COL.x + 400} size={26} weight={700} color={accent}>{note}</Copy>
      <Group y={48}>
        <Rect x={COL.x + width} y={-8} width={3} height={PANEL} fill={accent} opacity={0.8} />
        <Text x={COL.x} maxWidth={width}
          style={{ ...textStyle(SIZE, 700, color), lineHeight: LINE, lineBreak: mode }}>
          {COPY}
        </Text>
      </Group>
    </Group>
  );
}

// The same sentence, the same width, two line-breaking rules. 'normal' is the
// counter-example: it breaks wherever Unicode allows, so words split.
export function Phrase() {
  const f = useCurrentFrame();
  const stop = useCue(WIDTHS);
  const from = stop?.previous?.width ?? WIDTHS[0].width;
  const to = stop?.cue.width ?? WIDTHS[0].width;
  const width = from + (to - from) * progress(stop?.frame ?? 0, 0, 10, Easings.easeInOutCubic);
  return (
    <Stage bg={C.ink} y={AREA.height / 2 + 16 * progress(f, 0, BEAT * 8)}>
      <TextReveal x={COL.x} y={TOP} lineHeight={124} baseline={0.8} from={-6} stagger={4} style={textStyle(108, 900)}>
        <Span style={{ fill: C.hot }}>文節</Span>{'で、\n折り返す。'}
      </TextReveal>
      <Group opacity={progress(f, 0, 8)}>
        <Copy x={COL.x + COL.width} y={TOP + 208} ax={1} size={26} weight={500} font="mono" color={C.soft}>
          {`maxWidth={${Math.round(width)}}`}
        </Copy>
        <Panel y={TOP + 280} mode="normal" width={width} color={C.grey} note="言葉の途中で折れる" />
        <Panel y={TOP + 280 + PANEL + 72} mode="phrase" width={width} color={C.paper} note="文節で折れる" />
      </Group>
    </Stage>
  );
}
