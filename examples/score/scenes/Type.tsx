import { TypewriterText } from '@celesta.studio/components';
import { Group, Rect, Text } from '@celesta.studio/react';

import { COMMAND, HEIGHT, INK, MUTED, PAPER, TYPING, WIDTH } from '../constants';

/** A command types itself; the score ticks on every character (TYPED in timeline.ts). */
export function Type() {
  return (
    <Group>
      <Rect width={WIDTH} height={HEIGHT} fill={INK} />
      <Text x={160} y={220} style={{ fontSize: 28, fill: { type: 'solid', color: MUTED } }}>
        One tick per character, from typewriterFrames()
      </Text>
      <TypewriterText x={160} y={300} fontSize={72} color={PAPER} {...TYPING}>
        {COMMAND}
      </TypewriterText>
    </Group>
  );
}
