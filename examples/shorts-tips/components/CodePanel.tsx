import { Group, Rect, Text, progress } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import { withAlpha } from '../color';
import { BAR, BEAT, C, CODE, CODE_FONTS, FONT, SAFE, TYPE_FRAMES, W } from '../constants';
import { THEME, getPlan } from '../plan';

// The code band across the upper half: an editor holding the variant's
// snippet. With `frame` (the absolute film frame) each line types in at its
// reveal time, with a band on the newest line; without it the whole snippet
// is shown.
//
// It is drawn here rather than with @celesta.studio/code's <Code>, to control the
// look: a band on the newest line, a caret that blinks on the beat, and a
// band that runs to the frame's edges. The lines and colour runs come from
// highlightCode() (plan.ts), so they split and colour exactly like <Code>. Positions come from JetBrains Mono's fixed advance instead of
// measuring, so scripts/inspect.mjs can evaluate every frame even without
// an exporter to shape text (--no-native).
export function CodePanel({ frame, accent }: { frame?: number; accent: string }) {
  const { lines, fontSize, lineHeight, revealAt } = getPlan();
  const top = CODE.bar + CODE.padY;
  const last = revealAt[revealAt.length - 1];
  // The newest revealed line, and how strongly it is marked: the band stays
  // through the build and fades out a bar after the last line is typed.
  const current = frame === undefined ? -1 : revealAt.findLastIndex((at, i) => at <= frame && lines[i].length > 0);
  const band = frame === undefined ? 0 : 1 - progress(frame, last + TYPE_FRAMES + BAR, 12);
  return (
    <Group y={CODE.y}>
      <Rect width={W} height={CODE.height} fill={C.panel} />
      <Rect width={W} height={CODE.bar} fill={C.bar} />
      {['#FF5F57', '#FEBC2E', '#28C840'].map((fill, i) => (
        <Circle key={fill} x={CODE.textX + 8 + i * 30} y={CODE.bar / 2} anchorX={0.5} anchorY={0.5} radius={8} fill={fill} />
      ))}
      <Text x={(SAFE.left + SAFE.right) / 2} y={CODE.bar / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 24, fontWeight: 700, fill: { type: 'solid', color: C.grey } }}>
        film.tsx
      </Text>

      {current >= 0 && band > 0 && (
        <Group y={top + current * lineHeight} opacity={band}>
          <Rect width={W} height={lineHeight} fill={withAlpha(accent, 0x26 / 255)} />
          <Rect width={CODE.textX - 24} height={lineHeight} fill={accent} />
        </Group>
      )}

      {lines.map((line, i) => {
        const typed = frame === undefined ? line.length : Math.floor(line.length * progress(frame, revealAt[i], TYPE_FRAMES));
        if (typed <= 0) return null;
        return (
          <Text key={i} x={CODE.textX} y={top + i * lineHeight + lineHeight * 0.72} anchorY="baseline"
            style={{
              fontFamily: CODE_FONTS, fontSize, fill: { type: 'solid', color: THEME.foreground },
              colorRuns: line.runs, visibleCharacters: typed < line.length ? line.columns[typed] : undefined,
            }}>
            {line.text}
          </Text>
        );
      })}

      {frame !== undefined && current >= 0 && band > 0 && <Caret frame={frame} line={current} />}
    </Group>
  );
}

// A block caret after the typed text of the newest line; it blinks on the beat once the line is done.
function Caret({ frame, line }: { frame: number; line: number }) {
  const { lines, fontSize, lineHeight, revealAt } = getPlan();
  const { length, columns, advances } = lines[line];
  const typed = Math.floor(length * progress(frame, revealAt[line], TYPE_FRAMES));
  const done = typed >= length;
  if (done && (frame - revealAt[line]) % BEAT >= BEAT / 2) return null;
  const x = CODE.textX + advances.slice(0, columns[typed]).reduce((a, b) => a + b, 0) * fontSize;
  const y = CODE.bar + CODE.padY + line * lineHeight + (lineHeight - fontSize * 1.15) / 2;
  return <Rect x={x + 2} y={y} width={Math.round(fontSize * 0.5)} height={Math.round(fontSize * 1.15)} fill="#FFFFFFCC" />;
}
