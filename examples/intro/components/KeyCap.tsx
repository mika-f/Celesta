import { Group, Rect } from '@celesta/react';
import { Path } from '@celesta/shapes';
import { C } from '../constants';
import { Headline } from './Headline';

type Command =
  | { type: 'moveTo'; x: number; y: number }
  | { type: 'lineTo'; x: number; y: number };

// The ⌘ symbol as strokes (neither font has the glyph): the four sides of a
// square, each run on into a three-quarter loop at its corner.
function commandSymbol(a: number, r: number): Command[] {
  const out: Command[] = [];
  for (const s of [-1, 1]) {
    out.push({ type: 'moveTo', x: s * a, y: -a - r }, { type: 'lineTo', x: s * a, y: a + r });
    out.push({ type: 'moveTo', x: -a - r, y: s * a }, { type: 'lineTo', x: a + r, y: s * a });
  }
  for (const sx of [-1, 1]) {
    for (const sy of [-1, 1]) {
      const cx = sx * (a + r);
      const cy = sy * (a + r);
      // From the point on the vertical side round the outside to the point on
      // the horizontal side, skipping the quarter that faces the square.
      const start = Math.atan2(0, -sx);
      const sweep = (sx * sy > 0 ? -1 : 1) * (Math.PI * 1.5);
      for (let i = 0; i <= 24; i += 1) {
        const t = start + (sweep * i) / 24;
        out.push({ type: i === 0 ? 'moveTo' : 'lineTo', x: cx + Math.cos(t) * r, y: cy + Math.sin(t) * r });
      }
    }
  }
  return out;
}

const SYMBOL = commandSymbol(8, 7);

// A ⌘S key cap, 120 px square. Pressed, it drops 6 px and lights up.
export function KeyCap({ x, y, pressed }: { x: number; y: number; pressed: boolean }) {
  const ink = pressed ? C.bg : C.fg;
  return (
    <Group x={x} y={y + (pressed ? 6 : 0)}>
      <Rect width={120} height={120} fill={pressed ? C.accent : C.panel} stroke={pressed ? C.accent : C.line} strokeWidth={1} />
      <Path x={38} y={60} commands={SYMBOL} stroke={ink} strokeWidth={3} cap="round" join="round" />
      <Headline x={84} y={30} size={56} color={ink} anchorX={0.5}>S</Headline>
    </Group>
  );
}
