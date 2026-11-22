import { Group, Rect, Text, useBeat } from '@celesta.studio/react';

import { ACCENT, GRID, HEIGHT, INK, MUTED, PAPER, WIDTH } from '../constants';

/** Four squares, one per beat of the bar, bumped by the same grid the score plays on. */
export function Beat() {
  const { beatInBar, pulse } = useBeat(GRID);
  return (
    <Group>
      <Rect width={WIDTH} height={HEIGHT} fill={INK} />
      <Text x={160} y={160} style={{ fontSize: 28, fill: { type: 'solid', color: MUTED } }}>
        s.tempo(GRID) and useBeat(GRID): one grid for picture and sound
      </Text>
      {[0, 1, 2, 3].map(i => {
        const on = i === beatInBar;
        const size = 160 * (on ? 1 + 0.25 * pulse : 1);
        return (
          <Rect
            key={i}
            x={260 + i * 250}
            y={400}
            width={size}
            height={size}
            anchorX={0.5}
            anchorY={0.5}
            cornerRadius={0}
            fill={on ? ACCENT : PAPER}
            opacity={on ? 1 : 0.35}
          />
        );
      })}
    </Group>
  );
}
