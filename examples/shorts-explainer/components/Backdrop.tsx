import { Instances, Rect, useCurrentFrame } from '@celesta.studio/react';
import { C, H, STAGE_Y, W } from '../constants';

// The dot grid: rows offset by half a step, one row above the top so the
// grid can drift upward without a gap. Its centres are refilled every frame.
const STEP = 72;
const FIRST_ROW = -1;
const ROWS = Math.ceil(H / STEP + 1) - FIRST_ROW;
const COLUMNS = Math.ceil(W / STEP + 1);
const DOTS = ROWS * COLUMNS;
const xs = new Float32Array(DOTS);
const ys = new Float32Array(DOTS);

// The whole-video background behind the panel and portraits: a sky-to-cream
// gradient, a dot grid drifting upward, and a soft floor.
export function Backdrop() {
  const frame = useCurrentFrame();
  const drift = (frame * 0.8) % STEP;
  for (let r = 0; r < ROWS; r++) {
    const row = FIRST_ROW + r;
    for (let col = 0; col < COLUMNS; col++) {
      const i = r * COLUMNS + col;
      xs[i] = col * STEP + (row % 2 === 0 ? 0 : STEP / 2);
      ys[i] = row * STEP - drift;
    }
  }
  return (
    <>
      <Rect width={W} height={H} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: H },
        stops: [{ offset: 0.4, color: C.sky }, { offset: 0.7, color: '#EEF7FF' }, { offset: 1, color: C.cream }],
      }} />
      <Instances count={DOTS} x={xs} y={ys} shape="circle" width={8} height={8} fill={C.line} />
      <Rect y={STAGE_Y + 160} width={W} height={H - STAGE_Y - 160} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: H - STAGE_Y - 160 },
        stops: [{ offset: 0, color: '#FFC93C00' }, { offset: 1, color: '#FFC93C55' }],
      }} />
    </>
  );
}
