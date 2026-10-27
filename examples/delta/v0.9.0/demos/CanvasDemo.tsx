import { Easings, Group, Rect, Text, interpolate } from '@celesta/react';
import { PatternBackground, Panel } from '@celesta/components';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { Pointer, mono } from './common';

// CANVAS: the editor's canvas tools (#530) and inline text (#535), drawn.
// Two beats each: Size pulls the right edge alone; Center turns the card
// about its box's middle; Pivot moves the anchor to a corner (the card stays
// where it is, x and y take up the difference) and turns about it; a
// double-click edits the text in place. The source line follows. The card
// is a real Panel with the transform shown.

const H = 120;
const BASE = { x: 330, y: 200 };
const TEXTS = ['DELTA', 'ON AIR'];
const ease = (frame: number, from: number, to: number) =>
  interpolate(frame, [from, to], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

type Vec = { x: number; y: number };
const rotate = (v: Vec, deg: number): Vec => {
  const r = (deg * Math.PI) / 180;
  return { x: v.x * Math.cos(r) - v.y * Math.sin(r), y: v.x * Math.sin(r) + v.y * Math.cos(r) };
};

export function CanvasDemo({ frame }: DemoProps) {
  const size = ease(frame, 6, BAR - 6);
  const turn = ease(frame, BAR + 6, 2 * BAR - 6);
  const move = ease(frame, 2 * BAR + 2, 2 * BAR + 16);
  const pivotTurn = ease(frame, 2 * BAR + 18, 3 * BAR - 2);
  const w = 300 + 140 * size;
  const center = { x: w / 2, y: H / 2 };
  const corner = { x: 0, y: H };
  const origin = { x: center.x + (corner.x - center.x) * move, y: center.y + (corner.y - center.y) * move };
  const turnedAtMove = -10;
  const rotation = turnedAtMove * turn + 22 * pivotTurn;
  // Moving the anchor under a rotation would swing the card; x and y take up the difference.
  const d = { x: center.x - origin.x, y: center.y - origin.y };
  const rd = rotate(d, turnedAtMove);
  const t = { x: BASE.x + (d.x - rd.x) * (frame >= 2 * BAR ? 1 : 0), y: BASE.y + (d.y - rd.y) * (frame >= 2 * BAR ? 1 : 0) };
  const world = (p: Vec): Vec => {
    const r = rotate({ x: p.x - origin.x, y: p.y - origin.y }, rotation);
    return { x: t.x + origin.x + r.x, y: t.y + origin.y + r.y };
  };
  const editing = frame >= 3 * BAR + 4;
  const typed = Math.max(0, Math.min(TEXTS[1].length, Math.floor((frame - 3 * BAR - 12) / 3)));
  const text = !editing ? TEXTS[0] : typed === 0 ? TEXTS[0] : TEXTS[1].slice(0, typed);
  const selectAll = editing && typed === 0;
  const tool = frame < BAR ? 'Size' : frame < 2 * BAR ? 'Center' : frame < 3 * BAR ? 'Pivot' : '';
  const textWidth = text.length * 30;
  // Where the pointer works: the right edge, the turn handle, the anchor, the text.
  const pointer = frame < BAR ? world({ x: w, y: H / 2 })
    : frame < 2 * BAR ? world({ x: w / 2, y: -40 })
      : frame < 2 * BAR + 18 ? world(origin)
        : frame < 3 * BAR ? world({ x: w, y: -10 })
          : world({ x: 24 + textWidth, y: H / 2 + 10 });
  const source = `<Card x={${Math.round(t.x)}} y={${Math.round(t.y)}} width={${Math.round(w)}} rotation={${rotation.toFixed(0)}}`
    + `${move > 0 ? ` originX={${Math.round(origin.x)}} originY={${Math.round(origin.y)}}` : ''}>${text}</Card>`;
  return (
    <Group>
      <PatternBackground width={STAGE.width} height={STAGE.height} pattern="grid" spacing={40} size={1}
        background="#101114" color="paper" opacity={0.06} />
      <Group x={20} y={14}>
        {['Size', 'Center', 'Pivot'].map((name, i) => {
          const on = name === tool;
          return (
            <Group key={name} x={i * 110}>
              <Rect width={104} height={34} fill={on ? 'orange' : 'raised'} stroke={on ? 'orange' : 'line'} strokeWidth={1} />
              <Text x={52} y={17} anchorX={0.5} anchorY={0.5} style={mono(16, on ? C.ink : C.grey, 700)}>{name}</Text>
            </Group>
          );
        })}
        <Text x={350} y={17} anchorY={0.5} style={mono(15, C.dim)}>
          {tool === 'Pivot' ? 'Alt-drag moves the anchor' : tool === 'Size' ? 'one edge at a time' : tool === 'Center' ? 'turns about the box middle' : 'double-click: edit the text in place'}
        </Text>
      </Group>

      <Group x={t.x} y={t.y} rotation={rotation} originX={origin.x} originY={origin.y}>
        <Panel width={w} height={H} fill="orange" cornerRadius={4} padding={{ top: 26, right: 24, bottom: 26, left: 24 }}>
          {selectAll ? <Rect x={-4} y={-2} width={textWidth + 8} height={68} fill="blue" opacity={0.5} /> : null}
          <Text style={{ fontFamily: FONT.display, fontSize: 64, fill: solid(C.ink) }}>{text}</Text>
          {editing && !selectAll && frame % 14 < 8 ? <Rect x={textWidth + 4} y={2} width={4} height={60} fill="ink" /> : null}
        </Panel>
        <Rect x={-4} y={-4} width={w + 8} height={H + 8} stroke="blue" strokeWidth={2} />
        {[[0, 0], [w, 0], [0, H], [w, H], [w, H / 2], [0, H / 2]].map(([hx, hy], i) => (
          <Rect key={i} x={hx - 6} y={hy - 6} width={12} height={12} fill={tool === 'Size' && hx === w && hy === H / 2 ? 'orange' : 'paper'} stroke="blue" strokeWidth={2} />
        ))}
        {tool !== '' ? (
          <Group x={origin.x} y={origin.y}>
            <Rect x={-14} y={-1.5} width={28} height={3} fill={tool === 'Pivot' ? 'amber' : 'blue'} />
            <Rect x={-1.5} y={-14} width={3} height={28} fill={tool === 'Pivot' ? 'amber' : 'blue'} />
          </Group>
        ) : null}
        <Rect x={w / 2 - 1} y={-40} width={2} height={36} fill="blue" />
        <Rect x={w / 2 - 7} y={-46} width={14} height={14} cornerRadius={7} fill="paper" stroke="blue" strokeWidth={2} />
      </Group>

      <Rect y={STAGE.height - 74} width={STAGE.width} height={74} fill="#08090B" />
      <Text x={20} y={STAGE.height - 37} anchorY={0.5} maxWidth={STAGE.width - 40} style={mono(16, C.paper)}>{source}</Text>
      <Pointer x={pointer.x} y={pointer.y} pressed={frame % BAR > 6 && frame % BAR < BAR - 6} />
    </Group>
  );
}
