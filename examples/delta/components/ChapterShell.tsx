import type { ComponentType, ReactNode } from 'react';
import { Easings, Group, Rect, Text, interpolate, progress, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
import type { CodeLanguage } from '@celesta.studio/code';
import { useFitText } from '@celesta.studio/text';
import { Backdrop } from './Backdrop';
import { Delta } from './Delta';
import { DiffPanel } from './DiffPanel';
import { C, FONT, ease, solid } from './theme';

// One feature, laid out like a page of release notes: the name, what it
// does, the lines that use it, the pull requests that merged it, and a live
// preview of it on the right.

/** Frames a cut takes. A chapter's frame 0 is the downbeat the cut lands on. */
export const CUT = 8;

/** The preview panel's drawing area, which every demo draws into. */
export const PREVIEW = { x: 900, y: 130, width: 900, height: 820 } as const;
export const STAGE = { x: PREVIEW.x + 20, y: PREVIEW.y + 58, width: 860, height: 742 } as const;

export interface DemoProps {
  /** Chapter frame: 0 on the downbeat, negative while the cut comes in. */
  frame: number;
}

export interface Chapter {
  key: string;
  title: string;
  /** One line; may hold <Span>s. */
  headline: ReactNode;
  body: string;
  file: string;
  language?: CodeLanguage;
  code: string;
  merged: readonly (readonly [number, string])[];
  Demo: ComponentType<DemoProps>;
  /** Right-hand label of the preview header. */
  meta?: string;
}

const HEAD_STYLE = { fontFamily: FONT.sans, fontSize: 46, fontWeight: 700, lineHeight: 56, fill: solid(C.paper) } as const;
const BODY_STYLE = { fontFamily: FONT.sans, fontSize: 29, lineHeight: 44, fill: solid(C.grey) } as const;
const TITLE_STYLE ={ fontFamily: FONT.display, letterSpacing: { em: 0.01 }, fill: solid(C.paper) } as const;

/** Bebas Neue's capital height, as a share of the font size. */
const CAP = 0.7;
const TITLE_Y = 170;

function useTitleFit(text: string) {
  return useFitText(text, { width: 720, height: 320, minFontSize: 120, maxFontSize: 300, maxLines: 1, style: TITLE_STYLE });
}

function Title({ text, frame }: { text: string; frame: number }) {
  const fit = useTitleFit(text);
  const cap = fit.fontSize * CAP;
  const rise = ease(frame, -CUT + 2, 22);
  // The letters track in: spacing in em, so it scales with the fitted size.
  const spacing = interpolate(frame, [-CUT, 26], [0.09, 0.01], { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Group x={116} y={TITLE_Y} clip={{ x: -20, y: -20, width: 900, height: cap + 40 }}>
      <Text y={(cap + 40) * (1 - rise)} style={{ ...fit.style, letterSpacing: { em: spacing } }}>{text}</Text>
    </Group>
  );
}

function Label({ index, total, chapterKey, frame }: { index: number; total: number; chapterKey: string; frame: number }) {
  const mono = { fontFamily: FONT.mono, fontSize: 22 };
  const p = ease(frame, -CUT, 16);
  return (
    <Group x={120} y={118} opacity={p}>
      <Delta x={10} y={0} size={22} />
      <Text x={34} y={0} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.orange) }}>
        {`Δ${String(index + 1).padStart(2, '0')}`}
      </Text>
      <Text x={110} y={0} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>{`/ ${total}`}</Text>
      <Text x={190} y={0} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>{chapterKey}</Text>
    </Group>
  );
}

function Preview({ frame, meta, Demo }: { frame: number; meta: string; Demo: ComponentType<DemoProps> }) {
  const mono = { fontFamily: FONT.mono, fontSize: 19 };
  const shown = Math.max(0, frame);
  const p = ease(frame, -CUT, 20);
  return (
    <Group opacity={p} y={30 * (1 - p)}>
      <Rect x={PREVIEW.x} y={PREVIEW.y} width={PREVIEW.width} height={PREVIEW.height}
        fill={C.panel} stroke={C.line} strokeWidth={2} cornerRadius={18} />
      <Rect x={PREVIEW.x + 28} y={PREVIEW.y + 26} width={12} height={12} cornerRadius={6} fill={C.red} />
      <Text x={PREVIEW.x + 50} y={PREVIEW.y + 32} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.paper) }}>
        {`PREVIEW  ·  frame ${String(shown).padStart(3, '0')}`}
      </Text>
      <Text x={PREVIEW.x + PREVIEW.width - 28} y={PREVIEW.y + 32} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>
        {meta}
      </Text>
      <Group x={STAGE.x} y={STAGE.y} clip={{ width: STAGE.width, height: STAGE.height, cornerRadius: 10 }}>
        <Demo frame={frame} />
      </Group>
    </Group>
  );
}

export function ChapterShell({ chapter, index, total }: { chapter: Chapter; index: number; total: number }) {
  const frame = useCurrentFrame() - CUT;
  const head = ease(frame, 4, 20);
  const body = ease(frame, 9, 20);
  const panel = ease(frame, 14, 20);
  const merged = ease(frame, 30, 20);
  const typed = Math.max(0, Math.floor((frame - 18) * 2.2));
  // The copy stacks by measured heights: under the fitted title, then under a
  // headline that may take two lines.
  const headY = TITLE_Y + useTitleFit(chapter.title).fontSize * CAP + 58;
  const headM = useTextMetrics(chapter.headline, HEAD_STYLE, { maxWidth: 740 });
  const bodyY = headY + headM.height + 22;
  const bodyM = useTextMetrics(chapter.body, BODY_STYLE, { maxWidth: 720 });
  return (
    <Group>
      <Backdrop />
      <Label index={index} total={total} chapterKey={chapter.key} frame={frame} />
      <Title text={chapter.title} frame={frame} />
      <Text x={120} y={headY + 24 * (1 - head)} opacity={head} maxWidth={740} style={HEAD_STYLE}>
        {chapter.headline}
      </Text>
      <Text x={120} y={bodyY + 24 * (1 - body)} opacity={body} maxWidth={720} style={BODY_STYLE}>
        {chapter.body}
      </Text>
      <Group opacity={panel} y={20 * (1 - panel)}>
        <DiffPanel x={120} y={Math.max(650, bodyY + bodyM.height + 28)} width={720} file={chapter.file} code={chapter.code}
          language={chapter.language} typed={typed} />
      </Group>
      <Group x={120} y={1000} opacity={merged}>
        <Text anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 18, fontWeight: 700, fill: solid(C.dim) }}>MERGED</Text>
        <Text x={110} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) }}>
          {chapter.merged.map(([n, label]) => `#${n} ${label}`).join('   ·   ')}
        </Text>
      </Group>
      <Preview frame={frame} meta={chapter.meta ?? '1920×1080  ·  30 fps'} Demo={chapter.Demo} />
    </Group>
  );
}

/** Fades a demo element in on a beat: 0 → 1 over `duration` frames from `start`. */
export const enter = (frame: number, start: number, duration = 18) =>
  progress(frame, start, duration, Easings.easeOutExpo);
