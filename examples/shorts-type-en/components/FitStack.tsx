import { Group, Span } from '@celesta/react';
import type { CommonProps } from '@celesta/react';
import { TextReveal } from '@celesta/text';
import { textStyle } from './Copy';
import { C } from '../constants';
import type { Line } from '../measure';

// Capitals fill about 0.74 em above the baseline, so lines can sit this
// tight; the reveal mask is one line box tall.
const LEAD = 0.92;
const GAP = 10;

export const stackHeight = (lines: Line[]) => lines.reduce((h, l) => h + l.size * LEAD + GAP, -GAP);

// Headline lines stacked flush left, each at the size and tracking fitText()
// found for it, so every line runs the full column width. Lines reveal one after
// another; `accent` in a line is drawn in the accent colour.
export function FitStack({ lines, x = 0, y = 0, from = 0, stagger = 4, color = C.paper, accent = C.hot, glow }: {
  lines: Line[]; x?: number; y?: number; from?: number; stagger?: number; color?: string; accent?: string; glow?: CommonProps['glow'];
}) {
  let top = 0;
  return (
    <Group x={x} y={y} glow={glow}>
      {lines.map((line, i) => {
        const height = line.size * LEAD;
        const at = top;
        top += height + GAP;
        const cut = line.accent ? line.text.indexOf(line.accent) : -1;
        return (
          <TextReveal key={line.text} y={at} lineHeight={height} baseline={0.86} from={from + i * stagger}
            durationInFrames={14} style={{ ...textStyle(line.size, 'display', color), letterSpacing: line.tracking }}>
            {cut < 0 ? line.text : <>
              {line.text.slice(0, cut)}
              <Span style={{ fill: accent }}>{line.accent}</Span>
              {line.text.slice(cut + line.accent!.length)}
            </>}
          </TextReveal>
        );
      })}
    </Group>
  );
}
