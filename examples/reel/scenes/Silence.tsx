import { Easings, Rect, useCurrentFrame } from '@celesta/react';
import { BEAT, C, H, W } from '../constants';
import { progress } from '../helpers';

export function Silence() {
  const f = useCurrentFrame();
  const r = 6 + 10 * progress(f, 0, BEAT, Easings.easeInExpo);
  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Rect x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} width={r * 2} height={r * 2} cornerRadius={r} fill={C.accent} />
    </>
  );
}
