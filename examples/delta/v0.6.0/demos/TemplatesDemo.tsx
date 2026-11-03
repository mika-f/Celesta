import { Group, Rect, Text } from '@celesta.studio/react';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { BEAT, C, FONT, solid } from '../../components/theme';

// Start from a template, then render it again with other property values:
// the Properties panel (left) and --props (under the card) set the same
// inputs, and the card follows. The card is a sketch of the ranking template.
const VARIANTS = [
  { title: 'SPRING', accent: C.green },
  { title: 'AUTUMN', accent: C.orange },
  { title: 'NIGHT', accent: C.violet },
];
const ROWS = [['SHADER', 96], ['INSTANCES', 88], ['SPAN', 74]] as const;
const COMMAND = '$ Celesta --init my-video --template ranking';

export function TemplatesDemo({ frame }: DemoProps) {
  const which = Math.max(0, Math.min(VARIANTS.length - 1, Math.floor((frame - 30) / (2 * BEAT)) + 1));
  const v = frame < 30 ? VARIANTS[0] : VARIANTS[which];
  const switched = frame < 30 ? 1 : enter(frame, 30 + (which - 1) * 2 * BEAT, 12);
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  const typed = Math.max(0, Math.floor((frame + 4) * 2));
  const panel = enter(frame, 18);
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group x={30} y={26}>
        <Rect width={STAGE.width - 60} height={60} cornerRadius={10} fill={C.raised} stroke={C.line} strokeWidth={2} />
        <Text x={22} y={30} anchorY={0.5} style={{ ...mono, fontSize: 20, fill: solid(C.paper), visibleCharacters: typed }}>{COMMAND}</Text>
      </Group>
      <Group x={30} y={120} opacity={panel}>
        <Rect width={330} height={300} cornerRadius={10} fill={C.raised} stroke={C.line} strokeWidth={2} />
        <Text x={22} y={34} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.paper) }}>Properties</Text>
        <Text x={308} y={34} anchorX={1} anchorY={0.5} style={{ ...mono, fontSize: 15, fill: solid(C.dim) }}>props.json</Text>
        <Rect y={60} width={330} height={2} fill={C.line} />
        <Text x={22} y={96} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>Title</Text>
        <Rect x={22} y={116} width={286} height={46} cornerRadius={6} fill={C.ink} stroke={C.line} strokeWidth={2} />
        <Text x={36} y={139} anchorY={0.5} style={{ ...mono, fill: solid(C.paper) }}>{v.title.charAt(0) + v.title.slice(1).toLowerCase()}</Text>
        <Text x={22} y={196} anchorY={0.5} style={{ ...mono, fill: solid(C.grey) }}>Accent</Text>
        <Rect x={22} y={216} width={46} height={46} cornerRadius={6} fill={v.accent} />
        <Text x={84} y={239} anchorY={0.5} style={{ ...mono, fill: solid(C.paper) }}>{v.accent}</Text>
      </Group>
      <Group x={400} y={120} opacity={panel}>
        <Rect width={430} height={470} cornerRadius={14} fill={C.paper} />
        <Rect width={430} height={12} cornerRadius={6} fill={v.accent} />
        <Text x={32} y={46} style={{ fontFamily: FONT.mono, fontSize: 18, fontWeight: 700, fill: solid(C.dim) }}>TOP 3</Text>
        <Text x={32} y={74} opacity={switched} style={{ fontFamily: FONT.display, fontSize: 120, fill: solid(C.ink) }}>{v.title}</Text>
        {ROWS.map(([name, score], i) => {
          const grow = enter(frame, 24 + i * 4, 20) * switched;
          return (
            <Group key={name} x={32} y={250 + i * 70}>
              <Text anchorY={0.5} y={14} style={{ fontFamily: FONT.display, fontSize: 40, fill: solid(v.accent) }}>{`0${i + 1}`}</Text>
              <Text x={60} anchorY={0.5} y={14} style={{ fontFamily: FONT.sans, fontSize: 24, fontWeight: 700, fill: solid(C.ink) }}>{name}</Text>
              <Rect x={60} y={36} width={300 * (score / 100) * grow} height={10} cornerRadius={5} fill={v.accent} />
              <Text x={366} anchorX={1} y={14} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.ink) }}>{String(score)}</Text>
            </Group>
          );
        })}
      </Group>
      <Group x={30} y={640} opacity={enter(frame, 30)}>
        <Text anchorY={0.5} style={{ ...mono, fill: solid(C.dim) }}>Celesta-export --react film.tsx out.mp4 \</Text>
        <Text x={36} y={34} anchorY={0.5} style={{ ...mono, fill: solid(C.paper) }}>
          {`--props '{"title":"${v.title.charAt(0) + v.title.slice(1).toLowerCase()}","accent":"${v.accent}"}'`}
        </Text>
      </Group>
    </Group>
  );
}
