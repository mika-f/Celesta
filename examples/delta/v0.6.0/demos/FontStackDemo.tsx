import { Group, Rect, Span, Text } from '@celesta.studio/react';
import { enter, STAGE } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';

// Top: one Text with a font stack, Latin from Space Grotesk and Japanese from
// Noto Sans JP (the orange only marks which is which). Bottom: one em of
// letterSpacing, the same at three sizes, so the gaps grow with the type.
const LATIN = 'Celesta 0.6.0 ';
const JAPANESE = 'で和欧混植。';
const LEGEND_JA = 'かな・漢字';
// Every Japanese string the film draws besides the PR titles; the Japanese
// web font is subset to these.
export const JA_STRINGS = [JAPANESE, LEGEND_JA, '日本語'];

const SIZES = [64, 100, 150];

export function FontStackDemo({ frame }: DemoProps) {
  const mono = { fontFamily: FONT.mono, fontSize: 19 };
  const typed = Math.max(0, Math.floor(frame * 0.7));
  const legend = enter(frame, 20);
  const lower = enter(frame, 34);
  const em = 0.02 + 0.22 * (0.5 - 0.5 * Math.cos(Math.max(0, frame - 40) / 80 * Math.PI * 2));
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Text x={40} y={44} anchorY={0.5} opacity={enter(frame, -4)} style={{ ...mono, fill: solid(C.dim) }}>
        {"fontFamily: ['Space Grotesk', 'Noto Sans JP']"}
      </Text>
      <Text x={40} y={110} style={{ fontFamily: FONT.sans, fontSize: 78, fontWeight: 700, fill: solid(C.paper), visibleCharacters: typed }}>
        {LATIN}<Span style={{ fill: C.orange }}>{JAPANESE}</Span>
      </Text>
      <Group x={40} y={250} opacity={legend}>
        <Rect y={-7} width={14} height={14} cornerRadius={7} fill={C.paper} />
        <Text x={28} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>Space Grotesk  ·  Latin, digits</Text>
        <Rect y={33} width={14} height={14} cornerRadius={7} fill={C.orange} />
        <Text x={28} y={40} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>{`Noto Sans JP   ·  ${LEGEND_JA}`}</Text>
      </Group>
      <Rect x={40} y={340} width={STAGE.width - 80} height={2} fill={C.line} opacity={lower} />
      <Group opacity={lower}>
        <Text x={40} y={384} anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>letterSpacing:</Text>
        <Text x={210} y={384} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.orange) }}>
          {`{ em: ${em.toFixed(2)} }`}
        </Text>
        {SIZES.map((size, i) => (
          <Text key={size} x={40} y={430 + [0, 76, 190][i]}
            style={{ fontFamily: FONT.display, fontSize: size, letterSpacing: { em }, fill: solid(i === 2 ? C.paper : C.grey) }}>
            TRACKING
          </Text>
        ))}
      </Group>
    </Group>
  );
}
