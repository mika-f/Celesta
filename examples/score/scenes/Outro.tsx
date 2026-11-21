import { Group, Rect, Text, interpolate, useCurrentFrame } from '@celesta.studio/react';

import { ACCENT, BEAT, HEIGHT, INK, PAPER, WIDTH, WORDS } from '../constants';

/** One word a beat, each on its pop (score.ts). */
export function Outro() {
  const frame = useCurrentFrame();
  return (
    <Group>
      <Rect width={WIDTH} height={HEIGHT} fill={INK} />
      {WORDS.map((word, i) => {
        const at = (i + 1) * BEAT;
        const t = interpolate(frame, [at, at + 6], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
        return (
          <Text
            key={word}
            x={WIDTH / 2 + (i - 1) * 300}
            y={400 - 20 * (1 - t)}
            anchorX={0.5}
            anchorY="baseline"
            opacity={t}
            style={{ fontSize: 96, fontWeight: 700, fill: { type: 'solid', color: i === 2 ? ACCENT : PAPER } }}
          >
            {word}
          </Text>
        );
      })}
    </Group>
  );
}
