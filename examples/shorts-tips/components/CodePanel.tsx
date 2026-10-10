import { Group, Rect, Span, Text, progress } from '@celesta/react';
import { Circle } from '@celesta/shapes';
import { BAR, BEAT, C, CODE, FONT, SAFE, TYPE_FRAMES, W } from '../constants';
import { THEME, getPlan } from '../plan';

// The code band across the upper half: an editor holding the variant's
// snippet. With `frame` (the absolute film frame) each line types in at its
// reveal time, with a band on the newest line; without it the whole snippet
// is shown.
//
// Lines are drawn without measuring: JetBrains Mono's fixed advance gives the
// caret position, so scripts/inspect.mjs can evaluate every frame (the
// <Code> component of @celesta/code measures text, which inspect.mjs cannot).
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
          <Rect width={W} height={lineHeight} fill={`${accent}26`} />
          <Rect width={CODE.textX - 24} height={lineHeight} fill={accent} />
        </Group>
      )}

      {lines.map((line, i) => {
        const typed = frame === undefined ? line.length : Math.floor(line.length * progress(frame, revealAt[i], TYPE_FRAMES));
        if (typed <= 0) return null;
        return (
          <Text key={i} x={CODE.textX} y={top + i * lineHeight + lineHeight * 0.72} anchorY="baseline"
            style={{
              fontFamily: FONT.mono, fontSize, fill: { type: 'solid', color: THEME.foreground },
              visibleCharacters: typed < line.length ? typed : undefined,
            }}>
            {line.runs.map((run, j) => (
              <Span key={j} style={{ fill: run.color, fontFamily: run.wide ? FONT.ja : undefined }}>{run.text}</Span>
            ))}
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
  const { length, advances } = lines[line];
  const typed = Math.floor(length * progress(frame, revealAt[line], TYPE_FRAMES));
  const done = typed >= length;
  if (done && (frame - revealAt[line]) % BEAT >= BEAT / 2) return null;
  const x = CODE.textX + advances.slice(0, typed).reduce((a, b) => a + b, 0) * fontSize;
  const y = CODE.bar + CODE.padY + line * lineHeight + (lineHeight - fontSize * 1.15) / 2;
  return <Rect x={x + 2} y={y} width={Math.round(fontSize * 0.5)} height={Math.round(fontSize * 1.15)} fill="#FFFFFFCC" />;
}
