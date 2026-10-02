import { Easings, Rect, useCurrentFrame } from '@celesta/react';
import { C, H, W } from '../constants';
import { progress } from '../helpers';

// A slanted accent band that sweeps across a cut. Place it so the cut lands
// at its midpoint.
export function Wipe() {
  const f = useCurrentFrame();
  const p = progress(f, 0, 14, Easings.easeInOutCubic);
  const x = -700 + (W + 1400) * p;
  return (
    <>
      <Rect x={x} y={H / 2} anchorX={0.5} anchorY={0.5} width={620} height={H * 1.6} rotation={14} fill={C.accent} />
      <Rect x={x - 380} y={H / 2} anchorX={0.5} anchorY={0.5} width={60} height={H * 1.6} rotation={14} fill={C.paper} />
    </>
  );
}
