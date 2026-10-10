import { Rect } from '@celesta/react';
import { CY, H, INDIGO, NIGHT, VIOLET, W } from '../constants';
import { theta } from '../loop';
import { silk } from '../shaders';

// The background: one full-frame rect whose pixels `silk` computes. The
// rect's own fill is never seen.
export function Field({ frame, pulse }: { frame: number; pulse: number }) {
  return <Rect width={W} height={H} fill={NIGHT} shader={silk({
    theta: theta(frame),
    pulse,
    focus: [0, (CY - H / 2) / W],
    deep: NIGHT,
    low: INDIGO,
    high: VIOLET,
  })} />;
}
