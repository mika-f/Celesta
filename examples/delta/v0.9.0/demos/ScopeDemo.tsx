import { Easings, Group, Rect, Text, interpolate } from '@celesta/react';
import { Panel } from '@celesta/components';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { Pointer, mono } from './common';

// EDIT SCOPE: an element inside a component three scenes call, and the
// choice of what an edit changes (#555). Drawn: under This scene, the
// width is scrubbed in Price only: the value is lifted into a new prop,
// rectWidth, and passed at Price's call. Under Shared value, the tone is
// changed for every scene, while the width row is locked, as Price sets
// its own; its × hands Price back to the shared value. The tags are real
// Panels drawn at the values shown.

const SIDE = 600;
const ROW = 36;
const SCENES = [
  { name: 'Intro', label: '¥480' },
  { name: 'Price', label: 'Sale' },
  { name: 'Outro', label: '¥1,280' },
];
const HERE = 1;
const SHARED_WIDTH = 200;
const OWN_WIDTH = 260;

const ease = (frame: number, from: number, to: number) =>
  interpolate(frame, [from, to], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

function Tag({ width, tone, label, size }: { width: number; tone: string; label: string; size: number }) {
  return (
    <Panel width={width * size} height={64 * size} fill={tone} cornerRadius={4} padding={{ top: 10 * size, right: 16 * size, bottom: 10 * size, left: 16 * size }}>
      <Text style={{ fontFamily: FONT.display, fontSize: 44 * size, fill: solid(C.ink) }}>{label}</Text>
    </Panel>
  );
}

function Row({ y, label, value, active, locked, note }:
  { y: number; label: string; value: string; active?: boolean; locked?: boolean; note?: string }) {
  return (
    <Group y={y}>
      <Text x={12} y={ROW / 2} anchorY={0.5} style={mono(15, active ? C.orange : C.grey, active ? 700 : 400)}>{label}</Text>
      <Rect x={120} y={5} width={210} height={ROW - 10} cornerRadius={3} fill="#08090B" stroke={active ? 'orange' : 'line'} strokeWidth={active ? 2 : 1} opacity={locked ? 0.6 : 1} />
      <Text x={132} y={ROW / 2} anchorY={0.5} style={mono(15, locked ? C.grey : C.paper)}>{value}</Text>
      {locked ? <Text x={346} y={ROW / 2} anchorX={0.5} anchorY={0.5} style={mono(17, C.paper, 700)}>×</Text> : null}
      {note ? <Text x={120} y={ROW + 4} style={mono(13, C.amber)}>{note}</Text> : null}
    </Group>
  );
}

export function ScopeDemo({ frame }: DemoProps) {
  // Bar 1: This scene, the width scrubbed. Bar 2: Shared value, the tone
  // switched. Bar 3: the width row's × in Price.
  const shared = frame >= 2 * BAR - 4;
  const scrub = ease(frame, BAR + 2, 2 * BAR - 8);
  const tone = frame >= 2 * BAR + 18 ? 'blue' : 'orange';
  const cleared = frame >= 3 * BAR + 14;
  const own = cleared ? null : frame < BAR ? null : Math.round(SHARED_WIDTH + (OWN_WIDTH - SHARED_WIDTH) * scrub);
  const widthOf = (i: number) => (i === HERE && own !== null ? own : SHARED_WIDTH);
  const lit = (i: number) => (shared ? i !== HERE || own === null : i === HERE);

  const tabsAt = { x: SIDE + 12, y: 36 + 44 };
  const widthRow = { x: SIDE + 150, y: 36 + 140 + ROW / 2 };
  const toneRow = { x: SIDE + 230, y: 36 + 140 + ROW * 2 + ROW / 2 };
  const pointer = frame < BAR
    ? { x: interpolate(frame, [0, BAR - 8], [300, tabsAt.x + 60], { extrapolateRight: 'clamp', easing: Easings.easeInOutCubic }), y: tabsAt.y + 16 }
    : frame < 2 * BAR - 8
      ? { x: widthRow.x - 120 + 100 * scrub, y: widthRow.y }
      : frame < 2 * BAR + 8
        ? { x: tabsAt.x + 210, y: tabsAt.y + 16 }
        : frame < 3 * BAR
          ? { x: toneRow.x, y: toneRow.y }
          : { x: SIDE + 346, y: widthRow.y };
  const pressed = (frame >= BAR + 2 && frame < 2 * BAR - 8) || (frame >= 2 * BAR - 6 && frame < 2 * BAR - 2)
    || (frame >= 2 * BAR + 14 && frame < 2 * BAR + 18) || (frame >= 3 * BAR + 10 && frame < 3 * BAR + 14);

  const component = `function PriceTag({ label, rectWidth = ${SHARED_WIDTH}, tone = '${tone}' })`;
  const call = `<PriceTag label="Sale"${own !== null ? ` rectWidth={${own}}` : ''} />`;
  const line = shared
    ? ['Shared value is used in Intro, Outro', own !== null ? '· Price sets its own' : 'and Price']
    : ['Edits change Price only'];
  return (
    <Group>
      <Rect width={STAGE.width} height={36} fill="raised" />
      <Text x={14} y={18} anchorY={0.5} style={mono(15, C.grey)}>Price.tsx — Celesta</Text>
      <Text x={STAGE.width - 14} y={18} anchorX={1} anchorY={0.5} style={mono(15, C.dim)}>Properties</Text>

      <Group y={36}>
        <Rect width={SIDE - 12} height={300} fill="#101114" />
        <Text x={20} y={22} anchorY={0.5} style={mono(14, C.dim)}>scene: Price</Text>
        <Group x={60} y={110}>
          <Tag width={widthOf(HERE)} tone={tone} label={SCENES[HERE].label} size={1.5} />
          <Rect x={-5} y={-5} width={widthOf(HERE) * 1.5 + 10} height={106} stroke="blue" strokeWidth={2} dash={[6, 4]} />
        </Group>
      </Group>

      <Group x={SIDE} y={36}>
        <Rect width={STAGE.width - SIDE} height={300} fill="panel" />
        <Text x={12} y={18} anchorY={0.5} style={mono(14, C.paper, 700)}>{'<Rect> in <PriceTag>'}</Text>
        <Text x={12} y={44 + 16} anchorY={0.5} style={mono(14, C.grey)}>Edits change</Text>
        {['This scene', 'Shared value'].map((name, i) => {
          const on = shared === (i === 1);
          return (
            <Group key={name} x={130 + i * 128} y={44}>
              <Rect width={122} height={32} fill={on ? 'orange' : '#08090B'} stroke={on ? 'orange' : 'line'} strokeWidth={1} />
              <Text x={61} y={16} anchorX={0.5} anchorY={0.5} style={mono(14, on ? C.ink : C.grey, 700)}>{name}</Text>
            </Group>
          );
        })}
        {line.map((text, i) => <Text key={i} x={12} y={88 + i * 18} style={mono(13, shared ? C.orange : C.grey)}>{text}</Text>)}
        <Rect y={126} width={STAGE.width - SIDE} height={1} fill="line" />
        <Row y={140} label="width" value={String(widthOf(HERE))} active={!shared && frame >= BAR}
          locked={shared && own !== null} note={shared && own !== null ? 'This scene sets its own rectWidth' : undefined} />
        <Row y={140 + ROW * 2} label="fill" value={`tone · ${tone}`} active={shared && frame >= 2 * BAR + 8 && frame < 3 * BAR} />
      </Group>

      <Group y={350}>
        {SCENES.map((scene, i) => (
          <Group key={scene.name} x={i * 334}>
            <Rect width={322} height={96} fill="#101114" stroke={i === HERE ? 'paper' : 'line'} strokeWidth={i === HERE ? 2 : 1} />
            <Rect width={322} height={96} fill="orange" opacity={lit(i) ? 0.14 : 0} />
            <Text x={12} y={16} anchorY={0.5} style={mono(13, i === HERE ? C.paper : C.grey, 700)}>{i === HERE ? `${scene.name} · this scene` : scene.name}</Text>
            <Group x={14} y={34}>
              <Tag width={widthOf(i)} tone={tone} label={scene.label} size={0.8} />
            </Group>
          </Group>
        ))}
      </Group>

      <Group y={458}>
        <Rect width={STAGE.width} height={105} fill="#08090B" />
        <Text x={14} y={28} anchorY={0.5} style={mono(13, C.dim)}>PriceTag.tsx</Text>
        <Text x={130} y={28} anchorY={0.5} style={mono(15, shared && frame >= 2 * BAR + 18 && frame < 3 * BAR + 8 ? C.orange : C.paper)}>{component}</Text>
        <Text x={14} y={70} anchorY={0.5} style={mono(13, C.dim)}>Price.tsx</Text>
        <Text x={130} y={70} anchorY={0.5} style={mono(15, !shared && frame >= BAR ? C.orange : C.paper)}>{call}</Text>
      </Group>
      <Pointer x={pointer.x} y={pointer.y} pressed={pressed} />
    </Group>
  );
}
