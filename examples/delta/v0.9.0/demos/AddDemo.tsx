import { Easings, Group, Rect, Text, interpolate } from '@celesta/react';
import { PatternBackground, Panel } from '@celesta/components';
import { Ellipse } from '@celesta/shapes';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { Pointer, mono } from './common';

// ADD: elements added from the canvas (#553). Drawn: Add opens on the
// parent it will write into; Ellipse is chosen and drawn as a box, and
// written as the group's last child with its import, in a <Sequence> from
// the playhead; then a Text is clicked onto it, white so it reads on the
// shape. The ellipse and the text are real, at the values written.

const ITEMS = ['Rect', 'Ellipse', 'Circle', 'Text', 'Image', 'Video'];
const MENU = { x: 20, y: 52, width: 250, row: 30 };
const FROM = { x: 340, y: 150 };
const TO = { x: 640, y: 320 };
const TEXT_AT = { x: 420, y: 206 };
const BLUE = '#4F7CFF';
const SOURCE_Y = STAGE.height - 120;

const ease = (frame: number, from: number, to: number) =>
  interpolate(frame, [from, to], [0, 1], { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

export function AddDemo({ frame }: DemoProps) {
  // Bar 0: Add, Ellipse. Bar 1: the box drawn. Bar 2: Add, Text, a click. Bar 3: held.
  const menuOpen = (frame >= 8 && frame < BAR - 4) || (frame >= 2 * BAR + 4 && frame < 2 * BAR + 22);
  const picking = frame < 2 * BAR ? 'Ellipse' : 'Text';
  const hover = frame < 2 * BAR ? (frame >= 18 ? 1 : -1) : frame >= 2 * BAR + 12 ? 3 : -1;
  const drawing = frame >= BAR && frame < 2 * BAR - 6;
  const drag = ease(frame, BAR + 4, 2 * BAR - 10);
  const box = { x: FROM.x, y: FROM.y, width: Math.round((TO.x - FROM.x) * drag), height: Math.round((TO.y - FROM.y) * drag) };
  const ellipse = frame >= 2 * BAR - 6;
  const text = frame >= 2 * BAR + 30;
  const adding = (frame >= BAR - 4 && frame < 2 * BAR - 6) ? 'Ellipse' : (frame >= 2 * BAR + 22 && frame < 2 * BAR + 30) ? 'Text' : '';

  const addAt = { x: MENU.x + 60, y: 30 };
  const itemAt = (i: number) => ({ x: MENU.x + 120, y: MENU.y + 40 + i * MENU.row + MENU.row / 2 });
  const pointer = frame < 8 ? { x: interpolate(frame, [0, 8], [500, addAt.x], { extrapolateRight: 'clamp' }), y: addAt.y }
    : frame < BAR - 4 ? itemAt(1)
      : frame < 2 * BAR ? { x: FROM.x + box.width, y: FROM.y + box.height }
        : frame < 2 * BAR + 6 ? addAt
          : frame < 2 * BAR + 22 ? itemAt(3)
            : frame < 3 * BAR ? TEXT_AT
              : { x: 760, y: 420 };
  const pressed = drawing || (frame >= 4 && frame < 8) || (frame >= BAR - 8 && frame < BAR - 4)
    || (frame >= 2 * BAR + 2 && frame < 2 * BAR + 6) || (frame >= 2 * BAR + 18 && frame < 2 * BAR + 22) || (frame >= 2 * BAR + 26 && frame < 2 * BAR + 30);

  const written = text
    ? ['<Sequence from={45} durationInFrames={75}>', `  <Text x={${TEXT_AT.x}} y={${TEXT_AT.y}} fontSize={48} color="#FFFFFF">Text</Text>`, '</Sequence>']
    : ellipse
      ? ["import { Ellipse } from '@celesta/shapes';", '<Sequence from={45} durationInFrames={75}>',
        `  <Ellipse x={${FROM.x}} y={${FROM.y}} width={${TO.x - FROM.x}} height={${TO.y - FROM.y}} fill="${BLUE}" />`]
      : ['<Group x={0} y={0}>', '  <Card title="ON AIR" />', '</Group>'];
  const status = adding !== '' ? `to <Group> in <Main> · ${adding === 'Text' ? 'Click where the text starts' : 'Drag a box, or click'} · Esc cancels`
    : ellipse ? 'added as the group\'s last child · one undo removes it' : '';
  return (
    <Group>
      <PatternBackground width={STAGE.width} height={STAGE.height} pattern="grid" spacing={40} size={1}
        background="#101114" color="paper" opacity={0.06} />

      <Panel x={70} y={318} width={250} height={86} fill="tally" cornerRadius={4} padding={18}>
        <Text style={{ fontFamily: FONT.display, fontSize: 48, fill: solid(C.paper) }}>ON AIR</Text>
      </Panel>
      {ellipse ? <Ellipse x={FROM.x} y={FROM.y} width={TO.x - FROM.x} height={TO.y - FROM.y} fill={BLUE} /> : null}
      {text ? <Text x={TEXT_AT.x} y={TEXT_AT.y} fontFamily={FONT.sans} fontSize={48} color="#FFFFFF">Text</Text> : null}
      {drawing ? <Rect x={box.x} y={box.y} width={Math.max(1, box.width)} height={Math.max(1, box.height)} stroke="orange" strokeWidth={2} dash={[8, 5]} /> : null}
      {ellipse && !text ? <Rect x={FROM.x - 4} y={FROM.y - 4} width={TO.x - FROM.x + 8} height={TO.y - FROM.y + 8} stroke="blue" strokeWidth={2} /> : null}
      {text ? <Rect x={TEXT_AT.x - 4} y={TEXT_AT.y - 4} width={112} height={68} stroke="blue" strokeWidth={2} /> : null}

      <Rect width={STAGE.width} height={60} fill="raised" />
      <Group x={MENU.x} y={13}>
        <Rect width={120} height={34} fill={menuOpen || adding !== '' ? 'orange' : '#08090B'} stroke={menuOpen || adding !== '' ? 'orange' : 'line'} strokeWidth={1} />
        <Text x={60} y={17} anchorX={0.5} anchorY={0.5} style={mono(15, menuOpen || adding !== '' ? C.ink : C.paper, 700)}>{adding !== '' ? `Adding ${adding}` : 'Add'}</Text>
        <Text x={136} y={17} anchorY={0.5} maxWidth={STAGE.width - 180} style={mono(14, adding !== '' ? C.orange : C.dim)}>{status}</Text>
      </Group>
      {menuOpen ? (
        <Group x={MENU.x} y={MENU.y}>
          <Rect width={MENU.width} height={40 + ITEMS.length * MENU.row + 8} fill="panel" stroke="line" strokeWidth={1} />
          <Text x={14} y={20} anchorY={0.5} style={mono(13, C.grey)}>{'Add to <Group> in <Main>'}</Text>
          {ITEMS.map((item, i) => (
            <Group key={item} y={40 + i * MENU.row}>
              {i === hover ? <Rect x={4} width={MENU.width - 8} height={MENU.row} fill="orange" /> : null}
              <Text x={14} y={MENU.row / 2} anchorY={0.5} style={mono(15, i === hover ? C.ink : C.paper, i === hover ? 700 : 400)}>{item}</Text>
              {item === picking && i !== hover ? <Rect x={MENU.width - 20} y={MENU.row / 2 - 3} width={6} height={6} fill="orange" /> : null}
            </Group>
          ))}
        </Group>
      ) : null}

      <Rect y={SOURCE_Y} width={STAGE.width} height={120} fill="#08090B" />
      <Text x={20} y={SOURCE_Y + 20} anchorY={0.5} style={mono(13, C.dim)}>Main.tsx</Text>
      {written.map((line, i) => (
        <Text key={i} x={20} y={SOURCE_Y + 48 + i * 26} anchorY={0.5} style={mono(16, ellipse ? C.orange : C.paper)}>{line}</Text>
      ))}
      <Pointer x={pointer.x} y={pointer.y} pressed={pressed} />
    </Group>
  );
}
