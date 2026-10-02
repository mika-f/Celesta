import type { ReactNode } from 'react';
import { Rect, random, useBeat } from '@celesta/react';
import { BPM, C } from '../constants';

export function DotGrid() {
  const { pulse } = useBeat({ bpm: BPM });
  const dots: ReactNode[] = [];
  for (let gx = 0; gx < 25; gx += 1) {
    for (let gy = 0; gy < 14; gy += 1) {
      dots.push(<Rect key={`${gx}-${gy}`} x={40 + gx * 80} y={20 + gy * 80} width={3} height={3} fill={C.paper}
        opacity={0.08 + 0.12 * pulse * random(gx * 31 + gy)} />);
    }
  }
  return <>{dots}</>;
}
