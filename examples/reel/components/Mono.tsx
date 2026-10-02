import { Group } from '@celesta/react';
import { Label } from './Label';
import { monoAdvance } from '../metrics';

export type Run = { text: string; color: string; weight?: number };

// Mono text in colored runs, vertically centered on `y`, showing its first
// `visible` characters. Each run starts at its column's cell and sits
// on a shared baseline, so a caret can be placed after any character.
export function Mono({ runs, x, y, size, visible = Infinity, opacity = 1 }: {
  runs: Run[];
  x: number;
  y: number;
  size: number;
  visible?: number;
  opacity?: number;
}) {
  let column = 0;
  return (
    <Group opacity={opacity}>
      {runs.map(({ text, color, weight = 400 }, i) => {
        const start = column;
        column += text.length;
        const shown = text.slice(0, Math.max(0, visible - start));
        return shown ? (
          <Label key={i} x={x + start * size * monoAdvance} y={y + size * 0.24} size={size} font="mono"
            weight={weight} color={color} ay="baseline">{shown}</Label>
        ) : null;
      })}
    </Group>
  );
}
export const runLength = (runs: Run[]) => runs.reduce((n, r) => n + r.text.length, 0);
