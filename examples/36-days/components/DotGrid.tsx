import { PatternBackground } from '@celesta.studio/components';
import { useBeat } from '@celesta.studio/react';
import { BPM, C } from '../constants';

// Faint square dots every 80 px that brighten on the beat, each by its own
// random amount.
export function DotGrid() {
  const { pulse } = useBeat({ bpm: BPM });
  const peak = 0.08 + 0.12 * pulse;
  return (
    <PatternBackground pattern="dots" dotShape="square" spacing={80} size={3} offsetX={1.5} offsetY={-18.5}
      color={C.paper} opacity={peak} variation={{ opacity: (0.12 * pulse) / peak }} />
  );
}
