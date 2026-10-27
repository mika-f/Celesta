import { Group, Rect, Text } from '@celesta/react';
import { Path } from '@celesta/shapes';
import { C, FONT, solid } from '../../components/theme';

// What the demos share: text styles, a label in the stage's corner, a
// pointer, and a strip of level bars.

export const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
export const sans = (size: number, color: string, weight = 500) =>
  ({ fontFamily: FONT.sans, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** A code note in a stage corner: what the demo is showing, as written. */
export function Note({ x, y, children, color = C.grey, anchorX = 0 }:
  { x: number; y: number; children: string; color?: string; anchorX?: number }) {
  return <Text x={x} y={y} anchorX={anchorX} style={mono(17, color)}>{children}</Text>;
}

/** A mouse pointer whose tip is at x/y. */
export function Pointer({ x, y, pressed = false }: { x: number; y: number; pressed?: boolean }) {
  const s = pressed ? 0.9 : 1;
  return (
    <Group x={x} y={y} scale={s}>
      <Path points={[[0, 0], [0, 26], [7, 20], [12, 31], [17, 29], [12, 18], [21, 18]]} closed join="round"
        fill={C.paper} stroke={C.ink} strokeWidth={2} />
    </Group>
  );
}

/** Level bars, `values` 0–1, from left to right, growing up from `height`. */
export function Bars({ x, y, width, height, values, color, gap = 2 }:
  { x: number; y: number; width: number; height: number; values: readonly number[]; color: string; gap?: number }) {
  const w = width / values.length;
  return (
    <Group x={x} y={y}>
      {values.map((v, i) => {
        const h = Math.max(1, Math.min(1, v) * height);
        return <Rect key={i} x={i * w} y={height - h} width={Math.max(1, w - gap)} height={h} fill={color} />;
      })}
    </Group>
  );
}

/** A thin rule with a label on its left, for the demos' lanes. */
export function Lane({ x, y, width, label }: { x: number; y: number; width: number; label: string }) {
  return (
    <Group x={x} y={y}>
      <Rect width={width} height={1} fill={C.line} />
      <Text y={-8} anchorY={1} style={mono(15, C.dim)}>{label}</Text>
    </Group>
  );
}
