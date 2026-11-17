import type { ReactNode } from 'react';
import { Easings, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import { Circle, Path } from '@celesta.studio/shapes';
import { C, FONT, solid } from '../../components/theme';

// What the demos share: text styles, the app's window chrome and its rows,
// a pointer, a check mark, and a switch.

export const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
export const sans = (size: number, color: string, weight = 500) =>
  ({ fontFamily: FONT.sans, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** 0 → 1 over `duration` frames from `start`, eased out. */
export const rise = (f: number, start: number, duration = 8) =>
  interpolate(f, [start, start + duration], [0, 1], { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

/** A code note in a stage corner: what the demo is showing, as written. */
export function Note({ x, y, children, color = C.grey, anchorX = 0 }:
  { x: number; y: number; children: string; color?: string; anchorX?: number }) {
  return <Text x={x} y={y} anchorX={anchorX} style={mono(17, color)}>{children}</Text>;
}

/** A mouse pointer whose tip is at x/y. */
export function Pointer({ x, y, pressed = false }: { x: number; y: number; pressed?: boolean }) {
  return (
    <Group x={x} y={y} scale={pressed ? 0.9 : 1}>
      <Path points={[[0, 0], [0, 26], [7, 20], [12, 31], [17, 29], [12, 18], [21, 18]]} closed join="round"
        fill={C.paper} stroke={C.ink} strokeWidth={2} />
    </Group>
  );
}

/** A pointer travelling through `keys` ([frame, x, y]), pressed on `presses` frames' spans. */
export function PointerPath({ f, keys, presses = [] }: { f: number; keys: readonly (readonly [number, number, number])[]; presses?: readonly (readonly [number, number])[] }) {
  const frames = keys.map((k) => k[0]);
  const opts = { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;
  const x = interpolate(f, frames, keys.map((k) => k[1]), opts);
  const y = interpolate(f, frames, keys.map((k) => k[2]), opts);
  return <Pointer x={x} y={y} pressed={presses.some(([a, b]) => f >= a && f < b)} />;
}

/** A check mark in a circle, drawn on as `p` goes 0 → 1. */
export function Check({ x, y, p, color = C.green, size = 26 }: { x: number; y: number; p: number; color?: string; size?: number }) {
  if (p <= 0) return null;
  const r = size / 2;
  return (
    <Group x={x} y={y} opacity={Math.min(1, p * 2)}>
      <Circle x={-r} y={-r} radius={r} fill={`${color}33`} stroke={color} strokeWidth={2} />
      <Path points={[[-r * 0.45, 0], [-r * 0.1, r * 0.35], [r * 0.5, -r * 0.35]]} stroke={color} strokeWidth={3} cap="round" join="round" progress={p} />
    </Group>
  );
}

/** A switch, on as `on` goes 0 → 1. */
export function Switch({ x, y, on, color = C.orange }: { x: number; y: number; on: number; color?: string }) {
  return (
    <Group x={x} y={y}>
      <Rect width={44} height={24} cornerRadius={12} fill={on > 0.5 ? color : C.line} />
      <Circle x={3 + 20 * on} y={3} radius={9} fill={on > 0.5 ? C.ink : C.grey} />
    </Group>
  );
}

/** The app's panel: a header band with its title, hairlines, and the rows under it. */
export function Panel({ x, y, width, height, title, right, children }: {
  x: number; y: number; width: number; height: number; title: string; right?: string; children?: ReactNode;
}) {
  return (
    <Group x={x} y={y}>
      <Rect width={width} height={height} fill={C.ink} stroke={C.line} strokeWidth={1} />
      <Rect width={width} height={40} fill={C.raised} />
      <Rect y={40} width={width} height={1} fill={C.line} />
      <Text x={14} y={20} anchorY={0.5} style={mono(16, C.paper, 700)}>{title}</Text>
      {right ? <Text x={width - 14} y={20} anchorX={1} anchorY={0.5} style={mono(15, C.dim)}>{right}</Text> : null}
      <Group y={41}>{children}</Group>
    </Group>
  );
}

/** A button: an outline, or filled when `primary`. */
export function Button({ x, y, label, primary = false, pressed = false, width }: {
  x: number; y: number; label: string; primary?: boolean; pressed?: boolean; width?: number;
}) {
  const w = width ?? label.length * 10 + 28;
  return (
    <Group x={x} y={y}>
      <Rect width={w} height={32} fill={primary ? (pressed ? C.amber : C.orange) : pressed ? C.line : C.raised} stroke={primary ? undefined : C.line} strokeWidth={1} />
      <Text x={w / 2} y={16} anchorX={0.5} anchorY={0.5} style={mono(15, primary ? C.ink : C.paper, 700)}>{label}</Text>
    </Group>
  );
}

/** Typed text: the first `n` characters, with a caret while typing. */
export function Typed({ x, y, text, n, style, caret = true }: {
  x: number; y: number; text: string; n: number;
  style: ReturnType<typeof mono>; caret?: boolean;
}) {
  const shown = text.slice(0, Math.max(0, Math.min(text.length, Math.floor(n))));
  const advance = 0.6 * style.fontSize;
  return (
    <Group x={x} y={y}>
      {shown ? <Text style={style}>{shown}</Text> : null}
      {caret && n < text.length + 6 ? <Rect x={shown.length * advance + 1} y={1} width={advance * 0.9} height={style.fontSize * 1.15} fill={C.orange} /> : null}
    </Group>
  );
}
