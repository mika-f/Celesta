import { PatternBackground } from '@celesta.studio/components';
import type { PatternTile } from '@celesta.studio/components';
import { Composition, Font, Group, Text, Theme } from '@celesta.studio/react';
import type { ReactNode } from 'react';

// <PatternBackground> fills an area with dots, a grid, stripes, or marks of
// your own, clipped to it and drawn as a few layers however many marks
// there are. `speedX`/`speedY` move it without gaps at the edges, and
// `variation` varies each mark by a seed, the same on every frame.

const colors = { bg: '#101820', ink: '#F4F1EA', accent: '#FF4D1F' };

// Crosses on a checkerboard: a function of the cell gives each cell its marks.
const crosses: PatternTile = {
  marks: ({ column, row }) => (column + row) % 2 === 0
    ? [{ width: 18, height: 3, rotation: 45 }, { width: 18, height: 3, rotation: -45 }]
    : [{ shape: 'circle', width: 6, color: 'accent' }],
};

function Panel({ x, y, label, children }: { x: number; y: number; label: string; children: ReactNode }) {
  return (
    <Group x={x} y={y}>
      {children}
      <Text x={16} y={16} fontFamily="mono" fontSize={14} fontWeight={700} color="ink">{label}</Text>
    </Group>
  );
}

export default function WithPatternBackground() {
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="JetBrains Mono" weights={[700]} />
      <Theme fonts={{ mono: 'JetBrains Mono' }} colors={colors}>
        <PatternBackground background="bg" pattern="grid" spacing={8} color="ink" opacity={0.04} />
        <Panel x={24} y={24} label="dots">
          <PatternBackground width={444} height={234} cornerRadius={8} pattern="dots" spacing={24} size={4}
            background="#18222C" color="ink" opacity={0.5} variation={{ opacity: 0.8, scale: 0.5, seed: 1 }} />
        </Panel>
        <Panel x={492} y={24} label="grid">
          <PatternBackground width={444} height={234} cornerRadius={8} pattern="grid" spacing={32} size={1}
            background="#18222C" color="ink" opacity={0.3} speedX={-16} speedY={-16} />
        </Panel>
        <Panel x={24} y={282} label="stripes">
          <PatternBackground width={444} height={234} cornerRadius={8} pattern="stripes" spacing={28} size={10} angle={-30}
            background="#18222C" color="accent" opacity={0.6} speedX={28} />
        </Panel>
        <Panel x={492} y={282} label="custom">
          <PatternBackground width={444} height={234} cornerRadius={8} pattern={crosses} spacing={36}
            background="#18222C" color="ink" opacity={0.7} speedY={18} />
        </Panel>
      </Theme>
    </Composition>
  );
}
