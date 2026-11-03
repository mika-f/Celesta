import type { ComponentType } from 'react';
import { Easings, Group, Rect, Text, interpolate, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
import { useFitText } from '@celesta.studio/text';
import { Backdrop } from '../components/Backdrop';
import { CUT, STAGE } from '../components/ChapterShell';
import type { Chapter, DemoProps } from '../components/ChapterShell';
import { Delta } from '../components/Delta';
import { DiffPanel } from '../components/DiffPanel';
import { C, FONT, ease, solid } from '../components/theme';
import { PORTRAIT } from './layout';

// A chapter page for the 9:16 short. The same parts as ChapterShell, stacked:
// the label, the title, the headline, the diff, and the live demo (its
// 860×742 stage at 0.95) last and largest. The copy and the code are inside
// the 'vertical-social' safe area; the demo is centred on the frame and may
// reach a little past it, as pictures may. The PRs move into the preview's
// header, and the body copy is left out.
const { width: WIDTH, height: HEIGHT, safe } = PORTRAIT;
const LEFT = safe.left + 24;
const INNER = WIDTH - safe.left - safe.right - 48;
const TOP = safe.top + 100;
const SCALE = 0.95;
const PANEL = { width: STAGE.width * SCALE + 40, height: 58 + STAGE.height * SCALE + 18 };
const PANEL_X = (WIDTH - PANEL.width) / 2;

const HEAD_STYLE = { fontFamily: FONT.sans, fontSize: 46, fontWeight: 700, lineHeight: 56, fill: solid(C.paper) } as const;
const TITLE_STYLE = { fontFamily: FONT.display, letterSpacing: { em: 0.01 }, fill: solid(C.paper) } as const;
const CAP = 0.7;

/** Spacing the title tracks in from; while it does, a long title may run past the safe area. */
const START_SPACING = 0.09;

function useTitleFit(text: string) {
  return useFitText(text, { width: INNER, height: 260, minFontSize: 110, maxFontSize: 300, maxLines: 1, style: TITLE_STYLE });
}

function Preview({ frame, y, meta, Demo }: { frame: number; y: number; meta: string; Demo: ComponentType<DemoProps> }) {
  const mono = { fontFamily: FONT.mono, fontSize: 19 };
  const p = ease(frame, -CUT, 20);
  return (
    <Group opacity={p} y={y + 30 * (1 - p)}>
      <Rect x={PANEL_X} width={PANEL.width} height={PANEL.height} fill={C.panel} stroke={C.line} strokeWidth={2} cornerRadius={18} />
      <Rect x={PANEL_X + 26} y={24} width={12} height={12} cornerRadius={6} fill={C.red} />
      <Text x={PANEL_X + 48} y={30} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.paper) }}>
        {`PREVIEW  ·  frame ${String(Math.max(0, frame)).padStart(3, '0')}`}
      </Text>
      <Text x={PANEL_X + PANEL.width - 26} y={30} anchorX={1} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>{meta}</Text>
      <Group x={PANEL_X + 20} y={58} scale={SCALE}>
        <Group clip={{ width: STAGE.width, height: STAGE.height, cornerRadius: 10 }}>
          <Demo frame={frame} />
        </Group>
      </Group>
    </Group>
  );
}

export function ShortChapter({ chapter, index, total }: { chapter: Chapter; index: number; total: number }) {
  const frame = useCurrentFrame() - CUT;
  const fit = useTitleFit(chapter.title);
  const cap = fit.fontSize * CAP;
  const rise = ease(frame, -CUT + 2, 22);
  const spacing = interpolate(frame, [-CUT, 26], [START_SPACING, 0.01], { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const head = ease(frame, 4, 20);
  const panel = ease(frame, 14, 20);
  const typed = Math.max(0, Math.floor((frame - 18) * 2.2));
  const label = ease(frame, -CUT, 16);
  const titleY = TOP + 40;
  const headY = titleY + cap + 44;
  const headM = useTextMetrics(chapter.headline, HEAD_STYLE, { maxWidth: INNER });
  const diffY = headY + headM.height + 28;
  const lines = chapter.code.split('\n').length;
  const previewY = diffY + 46 + lines * 46 + 10 + 28;
  const mono = { fontFamily: FONT.mono, fontSize: 22 };
  return (
    <Group>
      <Backdrop width={WIDTH} height={HEIGHT} />
      <Group x={LEFT} y={TOP} opacity={label}>
        <Delta x={10} y={0} size={22} />
        <Text x={34} y={0} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.orange) }}>
          {`Δ${String(index + 1).padStart(2, '0')}`}
        </Text>
        <Text x={110} y={0} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>{`/ ${total}`}</Text>
        <Text x={190} y={0} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>{chapter.key}</Text>
      </Group>
      <Group x={LEFT - 4} y={titleY} clip={{ x: -20, y: -20, width: INNER + 40, height: cap + 40 }}>
        <Text y={(cap + 40) * (1 - rise)} style={{ ...fit.style, letterSpacing: { em: spacing } }}>{chapter.title}</Text>
      </Group>
      <Text x={LEFT} y={headY + 24 * (1 - head)} opacity={head} maxWidth={INNER} style={HEAD_STYLE}>
        {chapter.headline}
      </Text>
      <Preview frame={frame} y={previewY} meta={chapter.merged.map(([n]) => `#${n}`).join(' ')} Demo={chapter.Demo} />
      <Group opacity={panel} y={20 * (1 - panel)}>
        <DiffPanel x={LEFT} y={diffY} width={INNER} file={chapter.file} code={chapter.code} language={chapter.language} typed={typed} />
      </Group>
    </Group>
  );
}
