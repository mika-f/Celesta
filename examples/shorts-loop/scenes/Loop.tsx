import { Group, useCurrentFrame } from '@celesta/react';
import { Bloom } from '../components/Bloom';
import { Copy } from '../components/Copy';
import { Field } from '../components/Field';
import { BAR, BEAT, CX, CY, CYAN } from '../constants';
import { hit, within } from '../loop';
import { lens } from '../shaders';

// The whole film is one scene with no cuts: a loop has nowhere to cut to.
// A hit lands on every beat; a lens ring leaves the centre on every bar.
// Both depend only on where the frame sits in its beat or bar, and every
// bar is the same, so the bar line at the loop point is like any other.

export function Loop() {
  const frame = useCurrentFrame();
  const pulse = hit(frame, BEAT, 4);
  const p = within(frame, BAR);
  // Out fast, then easing off; strongest a third of the way through the bar
  // (about one second in) and zero at both of its ends.
  const radius = 40 + 1300 * (1 - (1 - p) ** 2);
  const strength = 44 * 6.75 * p * (1 - p) ** 2;
  return <Group shader={lens({ center: [CX, CY], radius, width: 60 + 100 * p, strength, tint: CYAN })}>
    <Field frame={frame} pulse={pulse} />
    <Bloom frame={frame} pulse={pulse} />
    <Copy />
  </Group>;
}
