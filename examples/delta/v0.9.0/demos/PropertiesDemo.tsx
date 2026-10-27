import { Easings, Group, Rect, Text, interpolate, progress } from '@celesta/react';
import { Panel } from '@celesta/components';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR, SCENE_LIST } from '../timeline';
import { Pointer, mono } from './common';

// PROPERTIES: the editor edits what the code says (#516 #525 #550). Drawn:
// a card is selected; its x is scrubbed from the row's label and its title
// typed into a text row, each written back into the call in the source; the
// timeline marks the scenes the selection appears in, which an edit changes
// (#508). The card on the canvas is a real Panel drawn at the values shown.

const CANVAS = { x: 0, y: 36, width: 600, height: 338 };
const SIDE = 612;
const ROW = 38;
const FROM_X = 60;
const TO_X = 300;
const TITLES = ['ON AIR', 'ON AIR 0.9.0'];

function Row({ y, label, value, active, hot }: { y: number; label: string; value: string; active?: boolean; hot?: boolean }) {
  return (
    <Group y={y}>
      <Text x={12} y={ROW / 2} anchorY={0.5} style={mono(15, hot ? C.orange : C.grey, hot ? 700 : 400)}>{label}</Text>
      <Rect x={130} y={5} width={240} height={ROW - 10} cornerRadius={3} fill="#08090B" stroke={active ? 'orange' : 'line'} strokeWidth={active ? 2 : 1} />
      <Text x={142} y={ROW / 2} anchorY={0.5} style={mono(15, C.paper)}>{value}</Text>
    </Group>
  );
}

function Section({ y, title }: { y: number; title: string }) {
  return (
    <Group y={y}>
      <Rect width={STAGE.width - SIDE} height={28} fill="raised" />
      <Text x={12} y={14} anchorY={0.5} style={mono(14, C.paper, 700)}>{title}</Text>
    </Group>
  );
}

export function PropertiesDemo({ frame }: DemoProps) {
  const scrub = interpolate(frame, [BAR + 4, 2 * BAR - 4], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const x = Math.round(FROM_X + (TO_X - FROM_X) * scrub);
  const typing = Math.max(0, Math.min(TITLES[1].length, Math.floor((frame - 2 * BAR - 6) / 2)));
  const title = frame < 2 * BAR + 6 ? TITLES[0] : TITLES[1].slice(0, Math.max(TITLES[0].length, typing));
  const editingTitle = frame >= 2 * BAR && frame < 3 * BAR + 4;
  const bands = progress(frame, 3 * BAR, 12, Easings.easeOutCubic);
  // The pointer: to the x label, a drag to the right, then to the title row.
  const labelAt = { x: SIDE + 24, y: 36 + 34 + ROW * 0.5 };
  const titleAt = { x: SIDE + 300, y: 36 + 34 + ROW * 4 + 34 + ROW * 0.5 };
  const p = frame < BAR
    ? { x: interpolate(frame, [0, BAR - 8], [470, labelAt.x], { extrapolateRight: 'clamp', easing: Easings.easeInOutCubic }),
      y: interpolate(frame, [0, BAR - 8], [300, labelAt.y], { extrapolateRight: 'clamp', easing: Easings.easeInOutCubic }) }
    : frame < 2 * BAR
      ? { x: labelAt.x + 120 * scrub, y: labelAt.y }
      : { x: interpolate(frame, [2 * BAR, 2 * BAR + 8], [labelAt.x + 120, titleAt.x], { extrapolateRight: 'clamp', easing: Easings.easeInOutCubic }),
        y: interpolate(frame, [2 * BAR, 2 * BAR + 8], [labelAt.y, titleAt.y], { extrapolateRight: 'clamp', easing: Easings.easeInOutCubic }) };
  const scenes = SCENE_LIST.scenes;
  const total = SCENE_LIST.durationInFrames;
  const appears = new Set([1, 3, 5]);
  const source = `<Card x={${x}} y={120} title="${title}" tone="tally" />`;
  return (
    <Group>
      <Rect width={STAGE.width} height={36} fill="raised" />
      <Text x={14} y={18} anchorY={0.5} style={mono(15, C.grey)}>film.tsx — Celesta</Text>
      <Text x={STAGE.width - 14} y={18} anchorX={1} anchorY={0.5} style={mono(15, C.dim)}>Properties</Text>

      <Group x={CANVAS.x} y={CANVAS.y} clip={{ width: CANVAS.width, height: CANVAS.height }}>
        <Rect width={CANVAS.width} height={CANVAS.height} fill="#101114" />
        <Panel x={x} y={120} width={250} height={96} fill="tally" cornerRadius={4} padding={18}>
          <Text style={{ fontFamily: FONT.display, fontSize: 52, fill: solid(C.paper) }}>{title}</Text>
        </Panel>
        <Rect x={x - 5} y={115} width={260} height={106} stroke="blue" strokeWidth={2} dash={[6, 4]} />
        {[[x - 5, 115], [x + 255, 115], [x - 5, 221], [x + 255, 221]].map(([hx, hy], i) => (
          <Rect key={i} x={hx - 5} y={hy - 5} width={10} height={10} fill="paper" stroke="blue" strokeWidth={2} />
        ))}
      </Group>

      <Group x={SIDE} y={36}>
        <Rect width={STAGE.width - SIDE} height={CANVAS.height} fill="panel" />
        <Text x={12} y={17} anchorY={0.5} style={mono(14, C.paper, 700)}>{'<Card> · film.tsx:42'}</Text>
        <Section y={34 - 4} title="Transform" />
        <Row y={34 + 26} label="x" value={String(x)} active={frame >= BAR && frame < 2 * BAR} hot={frame >= BAR - 6 && frame < 2 * BAR} />
        <Row y={34 + 26 + ROW} label="y" value="120" />
        <Row y={34 + 26 + ROW * 2} label="scale" value="1" />
        <Section y={34 + 26 + ROW * 3 + 4} title="Component · Card" />
        <Row y={34 + 26 + ROW * 3 + 34} label="title" value={editingTitle && frame % 16 < 8 ? `${title}|` : title} active={editingTitle} />
        <Row y={34 + 26 + ROW * 4 + 34} label="tone" value="tally" />
      </Group>

      <Group y={CANVAS.y + CANVAS.height + 14}>
        <Text x={14} y={10} anchorY={0.5} style={mono(14, C.dim)}>Appears in</Text>
        <Group x={110}>
          {scenes.map((s, i) => {
            const sx = (s.from / total) * (STAGE.width - 130);
            const sw = (s.durationInFrames / total) * (STAGE.width - 130) - 2;
            const on = appears.has(i);
            return (
              <Group key={s.id} x={sx}>
                <Rect width={sw} height={20} fill={on ? 'orange' : 'raised'} opacity={on ? 0.35 + 0.65 * bands : 1} />
              </Group>
            );
          })}
        </Group>
        <Text x={14} y={44} anchorY={0.5} style={mono(14, bands > 0.5 ? C.orange : C.dim)}>
          {bands > 0.5 ? `an edit here changes ${appears.size} of ${scenes.length} scenes` : 'scenes'}
        </Text>
        <Rect x={0} y={66} width={STAGE.width} height={98} fill="#08090B" />
        <Text x={14} y={92} anchorY={0.5} style={mono(15, C.dim)}>42</Text>
        <Text x={56} y={92} anchorY={0.5} style={mono(17, C.paper)}>{source}</Text>
        <Text x={56} y={130} anchorY={0.5} style={mono(14, C.dim)}>written back into the call · Cmd+Z undoes it</Text>
      </Group>
      <Pointer x={p.x} y={p.y} pressed={frame >= BAR && frame < 2 * BAR} />
    </Group>
  );
}
