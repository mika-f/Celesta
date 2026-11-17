import type { ReactNode } from 'react';
import { Group, Rect, Text } from '@celesta.studio/react';
import type { PathCommand } from '@celesta.studio/shapes';
import { Circle, Path } from '@celesta.studio/shapes';
import { C, FONT, solid } from '../components/theme';
import { CABLE } from './theme';

// A module of the rack: a faceplate with four screws, a header with the
// module's number, name and run LED, a screen the demo draws into, a strip
// under it for the caption, and a row of jacks the cables plug into. The
// program draws it at full size on the right of the frame; the rack draws
// the same faceplate a fifth of that size in every slot.

/** The faceplate, where the program shows it. */
export const MODULE = { x: 820, y: 112, width: 1048, height: 860 } as const;
/** The screen inside the faceplate. Every demo draws into STAGE, the screen's size. */
export const SCREEN = { x: 24, y: 64, width: 1000, height: 563 } as const;
export const STAGE = { width: SCREEN.width, height: SCREEN.height } as const;
/** The jacks' centre line, and the strip the caption sits on. */
export const JACK_Y = 806;
export const STRIP = { x: SCREEN.x, y: SCREEN.y + SCREEN.height + 22, width: SCREEN.width } as const;
const JACK_R = 17;

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

export const number = (index: number) => String(index + 1).padStart(2, '0');

export interface Ports {
  /** What goes into the module, left to right from the left edge. */
  in: readonly string[];
  /** What comes out of it, right to left from the right edge. */
  out: readonly string[];
}

/** Where jack `n` of a side sits on the faceplate. */
export function jackX(side: 'in' | 'out', n: number) {
  return side === 'in' ? 70 + n * 130 : MODULE.width - 70 - n * 130;
}

function Screw({ x, y, turn }: { x: number; y: number; turn: number }) {
  return (
    <Group x={x} y={y}>
      <Circle x={-9} y={-9} radius={9} fill="#2B2E35" stroke="#3A3E46" strokeWidth={1} />
      <Rect x={-6} y={-1.5} width={12} height={3} rotation={turn} fill="#0E0F12" />
    </Group>
  );
}

/** A 3.5 mm jack: a nut, a ring, the hole; its label above. */
export function Jack({ x, y, label }: { x: number; y: number; label: string }) {
  return (
    <Group x={x} y={y}>
      <Circle x={-JACK_R} y={-JACK_R} radius={JACK_R} fill="#2E3138" stroke="#43474F" strokeWidth={2} />
      <Circle x={-10} y={-10} radius={10} fill="hole" />
      <Text y={-JACK_R - 12} anchorX={0.5} anchorY={1} style={mono(15, C.grey)}>{label}</Text>
    </Group>
  );
}

/** An LED with its glow when lit. */
export function Led({ x, y, on, color = C.green, size = 9 }: { x: number; y: number; on: number; color?: string; size?: number }) {
  return (
    <Group x={x} y={y}>
      {on > 0 ? <Circle x={-size * 2.4} y={-size * 2.4} radius={size * 2.4} fill={`${color}${Math.round(on * 0x30).toString(16).padStart(2, '0')}`} /> : null}
      <Circle x={-size} y={-size} radius={size} fill={on > 0.5 ? color : 'unlit'} />
    </Group>
  );
}

export function Faceplate({ width, height, fill = 'raised', children }: { width: number; height: number; fill?: string; children?: ReactNode }) {
  return (
    <Group>
      <Rect width={width} height={height} cornerRadius={6} fill={fill} stroke="line" strokeWidth={2} />
      <Screw x={22} y={22} turn={30} />
      <Screw x={width - 22} y={22} turn={-20} />
      <Screw x={22} y={height - 22} turn={70} />
      <Screw x={width - 22} y={height - 22} turn={10} />
      {children}
    </Group>
  );
}

/**
 * One feature's module. `power` 0–1 lights the LED and the screen; the
 * screen's content is `children`, drawn into STAGE.
 */
export function Module({ index, title, meta, ports, power, cable, children, caption }: {
  index: number; title: string; meta?: string; ports: Ports; power: number; cable: string;
  children?: ReactNode;
  /** Drawn on the strip under the screen. */
  caption?: ReactNode;
}) {
  const color = CABLE[cable];
  return (
    <Faceplate width={MODULE.width} height={MODULE.height}>
      <Rect x={44} y={20} width={8} height={24} fill={cable} />
      <Text x={66} y={33} anchorY={0.5} style={mono(22, C.orange, 700)}>{`Δ${number(index)}`}</Text>
      <Text x={140} y={33} anchorY={0.5} style={mono(22, C.paper, 700)}>{title}</Text>
      {meta ? <Text x={MODULE.width - 160} y={33} anchorX={1} anchorY={0.5} style={mono(18, C.dim)}>{meta}</Text> : null}
      <Led x={MODULE.width - 118} y={33} on={power} />
      <Text x={MODULE.width - 98} y={33} anchorY={0.5} style={mono(18, power > 0.5 ? C.green : C.dim, 700)}>{power > 0.5 ? 'RUN' : 'OFF'}</Text>
      <Rect x={SCREEN.x - 6} y={SCREEN.y - 6} width={SCREEN.width + 12} height={SCREEN.height + 12} fill="hole" stroke="line" strokeWidth={2} />
      <Group x={SCREEN.x} y={SCREEN.y} clip={{ width: SCREEN.width, height: SCREEN.height }}>
        <Rect width={SCREEN.width} height={SCREEN.height} fill="panel" />
        <Group opacity={power}>{children}</Group>
      </Group>
      {caption}
      <Rect x={44} y={JACK_Y - 62} width={MODULE.width - 88} height={1} fill="line" />
      {ports.in.map((label, n) => <Jack key={`in-${label}`} x={jackX('in', n)} y={JACK_Y} label={label} />)}
      {ports.out.map((label, n) => <Jack key={`out-${label}`} x={jackX('out', n)} y={JACK_Y} label={label} />)}
      <Signal from={jackX('in', ports.in.length - 1) + 50} to={jackX('out', ports.out.length - 1) - 50} y={JACK_Y} color={color} power={power} />
    </Faceplate>
  );
}

/** The signal path printed between the jacks: a rule with a head, lit when the module runs. */
function Signal({ from, to, y, color, power }: { from: number; to: number; y: number; color: string; power: number }) {
  const lit = power > 0.5;
  return (
    <Group y={y}>
      <Rect x={from} y={-1.5} width={to - from - 14} height={3} fill={lit ? color : C.line} />
      <Path points={[[to - 16, -9], [to, 0], [to - 16, 9]]} closed fill={lit ? color : C.line} />
    </Group>
  );
}

/**
 * A patch cable hanging from a jack: a cubic from the plug down to `end`,
 * with a dark sleeve under the colored jacket, and the plug on top.
 * `sway` swings the low end sideways, in pixels.
 */
export function Cable({ x, y, end, color, sway = 0, width = 13, plug = 0 }: {
  x: number; y: number; end: readonly [number, number]; color: string; sway?: number; width?: number;
  /** How far the plug is still out of its jack, in pixels (0: in). */
  plug?: number;
}) {
  const py = y + plug;
  const [ex, ey] = end;
  const ctrl: PathCommand[] = [
    { type: 'moveTo', x, y: py + 30 },
    { type: 'cubicTo', x1: x + sway * 0.4, y1: py + 30 + (ey - py) * 0.55, x2: ex + sway, y2: ey - (ey - py) * 0.5, x: ex + sway * 0.6, y: ey },
  ];
  return (
    <Group>
      <Path commands={ctrl} stroke="#060708" strokeWidth={width + 6} cap="round" />
      <Path commands={ctrl} stroke={color} strokeWidth={width} cap="round" />
      <Group x={x} y={py}>
        <Rect x={-9} y={-6} width={18} height={40} cornerRadius={3} fill="#3A3E46" />
        <Rect x={-12} y={10} width={24} height={30} cornerRadius={4} fill={color} />
        <Rect x={-12} y={22} width={24} height={3} fill="#00000055" />
      </Group>
    </Group>
  );
}
