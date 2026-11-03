import { Group, Rect, Text, useTextMetrics } from '@celesta.studio/react';
import type { TextStyle } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { JA_GLYPHS } from '../fonts';
import { FALLBACK_CODE } from '../strings';
import { BAR, BEAT } from '../timeline';

// Above: code set in JetBrains Mono alone, with fontFallback="loaded". The
// mono face has no Japanese, so those characters come from the next font
// the film loaded with <Font>, Noto Sans JP, not from whatever the machine
// has installed. The bar under each glyph says which face drew it.
// Below: every Japanese character in this film, which is all that Noto
// Sans JP is loaded with: <Font google text> asks Google Fonts for these alone.
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const CODE: TextStyle = { fontFamily: 'JetBrains Mono', fontSize: 30, fontFallback: 'loaded' };
const LATIN = /^[\0- ]+$/;

function CodeLine({ text, y, frame, at }: { text: string; y: number; frame: number; at: number }) {
  const m = useTextMetrics(text, CODE);
  const p = enter(frame, at, 12);
  const typed = Math.max(0, Math.floor((frame - at) * 1.6));
  return (
    <Group y={y} opacity={p}>
      <Text y={m.ascent} anchorY="baseline" style={{ ...CODE, fill: solid(C.paper), visibleCharacters: typed }}>{text}</Text>
      {m.glyphs.map((g, i) => (i < typed && g.text.trim() !== '' ? (
        <Rect key={i} x={g.x + 1} y={m.lineHeight + 4} width={g.width - 2} height={6}
          fill={LATIN.test(g.text) ? C.dim : C.orange} />
      ) : null))}
    </Group>
  );
}

export function FallbackDemo({ frame }: DemoProps) {
  const subset = enter(frame, 2 * BAR, 14);
  const perRow = 34;
  const shown = Math.max(0, Math.floor((frame - 2 * BAR) * 6));
  return (
    <Group>
      <Rect width={STAGE.width} height={210} cornerRadius={10} fill={C.raised} opacity={enter(frame, 0)} />
      <Group x={24} y={30}>
        {FALLBACK_CODE.map((line, i) => <CodeLine key={line} text={line} y={i * 84} frame={frame} at={i * BEAT * 2} />)}
      </Group>
      <Group x={24} y={240} opacity={enter(frame, BAR)}>
        <Rect y={-8} width={18} height={18} fill={C.dim} />
        <Text x={30} anchorY={0.5} y={1} style={mono(18, C.grey)}>{"fontFamily: 'JetBrains Mono'"}</Text>
        <Rect x={420} y={-8} width={18} height={18} fill={C.orange} />
        <Text x={450} anchorY={0.5} y={1} style={mono(18, C.grey)}>{'<Font google="Noto Sans JP">'}</Text>
      </Group>
      <Group x={24} y={310} opacity={subset}>
        <Text style={mono(19, C.paper, 700)}>{`<Font google="Noto Sans JP" text={…} />`}</Text>
        <Text y={32} style={mono(18, C.grey)}>{`${[...JA_GLYPHS].length} characters: every Japanese one in this film, and no others`}</Text>
        <Group y={90}>
          {[...JA_GLYPHS].slice(0, shown).map((ch, i) => (
            <Text key={i} x={(i % perRow) * 24} y={Math.floor(i / perRow) * 30}
              style={{ fontFamily: FONT.sans, fontSize: 20, fontWeight: 500, fill: solid(i % 7 === 0 ? C.orange : C.grey) }}>{ch}</Text>
          ))}
        </Group>
      </Group>
    </Group>
  );
}
