import { Easings, Group, Rect, Text, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Header, ToolTag } from '../components/Chrome';
import { CodeBlock } from '../components/CodeBlock';
import { C, FONT, ORDER, TOOL } from '../constants';
import { presence, progress } from '../helpers';
import { EFFECTS, PARTICLE, type Snippet } from '../snippets';
import type { ToolId } from '../constants';

const SIZE = 20;
const LH = 32;
const LEFT = 96;
const CODE_X = 470;
const WIDTH = 1728;

function Rows({ snippets, f }: { snippets: Record<ToolId, Snippet>; f: number }) {
  let y = 290;
  return ORDER.map((id, i) => {
    const s = snippets[id];
    const lines = s.source.split('\n').length;
    const h = Math.max(lines * LH + 48, 168);
    const top = y;
    y += h + 24;
    const start = 16 + i * 30;
    const p = progress(f, start, 20, Easings.easeOutExpo);
    const celesta = id === 'celesta';
    const focus = celesta ? progress(f, 150, 24) : 0;
    return (
      <Group key={id} x={LEFT} y={top + 24 * (1 - p)} opacity={p * (celesta ? 1 : 1 - 0.35 * progress(f, 150, 24))}>
        <Rect width={WIDTH} height={h} cornerRadius={16} fill={C.panel}
          stroke={celesta ? `${TOOL.celesta.color}${Math.round(focus * 255).toString(16).padStart(2, '0')}` : C.line}
          strokeWidth={celesta ? 3 : 1} />
        <Rect width={6} height={h} cornerRadius={3} fill={TOOL[id].color} />
        <ToolTag id={id} x={36} y={28} size={30} />
        <Text x={36} y={110} maxWidth={300} style={{
          fontFamily: FONT.sans, fontSize: 19, lineHeight: 28, fontWeight: 400,
          fill: { type: 'solid', color: C.soft },
        }}>{s.note}</Text>
        <Group x={CODE_X - LEFT} y={24}>
          <CodeBlock source={s.source} lang={s.lang} size={SIZE} lineHeight={LH}
            reveal={progress(f, start + 8, 36, Easings.easeInOutCubic)}
            marks={f > start + 50 ? s.marks : []} markColor={TOOL[id].color} />
        </Group>
      </Group>
    );
  });
}

function SyntaxScene({ n, title, sub, snippets }: { n: string; title: string; sub: string; snippets: Record<ToolId, Snippet> }) {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  return (
    <Group opacity={presence(f, durationInFrames, 12, 12)}>
      <Header f={f} n={n} title={title} sub={sub} />
      <Rows snippets={snippets} f={f} />
    </Group>
  );
}

export const SyntaxParticle = () => (
  <SyntaxScene n="03 — SYNTAX" title="Draw one particle." snippets={PARTICLE}
    sub="One of the 1,500. The same math, written the way each tool wants it." />
);

export const SyntaxEffects = () => (
  <SyntaxScene n="03 — SYNTAX" title="Blur it. Blend it. Make it glow." snippets={EFFECTS}
    sub="The heavier the effect, the wider the gap in how you write it." />
);
