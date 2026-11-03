import { Group, Rect, Text, useTextMetrics } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { BAR, BEAT } from '../timeline';

// A word measured with useTextMetrics(), which shapes it the way <Text>
// draws it: every line here comes from the result. The baseline, then the
// font's capHeight and xHeight, then each glyph's ink (inkAscent,
// inkDescent) inside its advance, kerning already taken out of it.
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const WORD = 'Type AV';
const STYLE = { fontFamily: FONT.sans, fontSize: 170, fontWeight: 700 } as const;
const BASELINE = 330;

function Guide({ y, label, value, color, at, frame, width }: {
  y: number; label: string; value: number; color: string; at: number; frame: number; width: number;
}) {
  const p = enter(frame, at, 14);
  return (
    <Group y={y} opacity={p}>
      <Rect x={0} y={-1} width={width * p} height={2} fill={color} />
      <Text x={width} y={-16} anchorX={1} anchorY={0.5} style={mono(17, color, 700)}>{`${label} ${value.toFixed(1)}`}</Text>
    </Group>
  );
}

export function MetricsDemo({ frame }: DemoProps) {
  const m = useTextMetrics(WORD, STYLE);
  // Measured alone, a glyph has no neighbour to kern with.
  const alone = useTextMetrics('A', STYLE).width;
  const kerned = m.glyphs.find((g) => g.text === 'A')?.width ?? alone;
  const left = 30;
  const width = STAGE.width - 2 * left;
  const ink = enter(frame, 2 * BAR, 14);
  const advances = enter(frame, 3 * BAR, 14);
  return (
    <Group>
      <Group x={left} opacity={enter(frame, 0)}>
        <Text y={BASELINE} anchorY="baseline" style={{ ...STYLE, fill: solid(C.paper) }}>{WORD}</Text>
      </Group>
      <Group x={left}>
        <Guide y={BASELINE} label="baseline" value={0} color={C.orange} at={BEAT} frame={frame} width={width} />
        <Guide y={BASELINE - m.xHeight} label="xHeight" value={m.xHeight} color={C.blue} at={BAR} frame={frame} width={width} />
        <Guide y={BASELINE - m.capHeight} label="capHeight" value={m.capHeight} color={C.green} at={BAR + BEAT} frame={frame} width={width} />
        {m.glyphs.map((g, i) => {
          if (g.text.trim() === '') return null;
          const p = enter(frame, 2 * BAR + i * 3, 12);
          return (
            <Group key={i} x={g.x} opacity={ink * p}>
              <Rect y={BASELINE - g.inkAscent} width={g.width} height={g.inkAscent + g.inkDescent}
                stroke={C.amber} strokeWidth={2} dash={[6, 5]} />
              <Group opacity={advances}>
                <Rect y={BASELINE + 70} width={g.width - 4} height={6} fill={C.dim} />
                <Text x={g.width / 2} y={BASELINE + 100} anchorX={0.5} anchorY={0.5} style={mono(17, C.grey)}>{g.width.toFixed(0)}</Text>
              </Group>
            </Group>
          );
        })}
      </Group>
      <Group x={left} y={500} opacity={enter(frame, 2 * BAR + BEAT)}>
        <Text style={mono(19, C.amber, 700)}>inkAscent / inkDescent</Text>
        <Text y={32} style={mono(18, C.grey)}>the outline's own top and bottom, per glyph</Text>
      </Group>
      <Group x={left} y={580} opacity={advances}>
        <Text style={mono(19, C.paper, 700)}>width: the advance, kerning included</Text>
        <Text y={32} maxWidth={STAGE.width - 2 * left} style={{ ...mono(18, C.grey), lineHeight: 28 }}>
          {`A alone ${alone.toFixed(1)}  ·  A before V ${kerned.toFixed(1)}  ·  kerning ${(kerned - alone).toFixed(1)}`}
        </Text>
      </Group>
    </Group>
  );
}

