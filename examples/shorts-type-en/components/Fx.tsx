import type { ReactNode } from 'react';
import { Easings, Group, cueAt, progress, useBeat, useCurrentFrame } from '@celesta/react';
import { BPM } from '../constants';
import { fx } from '../fx';
import { SCENE_CUES } from '../timeline';

// Drives the finishing pass: every beat kicks the RGB split, and every cut
// gets a burst of glitch slices and a four-frame flash. Flashes stay at one
// per scene (every four seconds), well under three per second.
export function Fx({ children }: { children: ReactNode }) {
  const f = useCurrentFrame();
  const { pulse, beat } = useBeat({ bpm: BPM });
  const cut = cueAt(SCENE_CUES, f);
  const sinceCut = cut ? cut.frame : 99;
  const hit = 1 - progress(sinceCut, 0, 7, Easings.easeOutCubic);
  const glitch = cut && cut.index > 0 ? hit : 0;
  return (
    <Group shader={fx({
      split: 5 * pulse + 30 * glitch,
      glitch,
      // A new slice pattern every two frames while it lasts.
      seed: Math.floor(f / 2) + beat * 0.37,
      flash: cut && cut.index > 0 ? 0.55 * (1 - progress(sinceCut, 0, 4)) : 0,
    })}>
      {children}
    </Group>
  );
}
