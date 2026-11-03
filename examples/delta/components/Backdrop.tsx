import { Instances, Rect, Static } from '@celesta.studio/react';
import { C, H, W } from './theme';

// The dotted ground every scene stands on: one <Instances> layer of dots,
// inside a <Static>, so it is built and sent to the renderer once.
const STEP = 80;
const grids = new Map<string, { xs: Float32Array; ys: Float32Array; count: number }>();

/** The dot positions for a canvas size, built once per size. */
function grid(width: number, height: number) {
  const key = `${width}x${height}`;
  let found = grids.get(key);
  if (!found) {
    const cols = Math.ceil(width / STEP);
    const count = cols * Math.ceil(height / STEP);
    const xs = new Float32Array(count);
    const ys = new Float32Array(count);
    for (let i = 0; i < count; i++) {
      xs[i] = STEP / 2 + (i % cols) * STEP;
      ys[i] = STEP / 2 + Math.floor(i / cols) * STEP;
    }
    found = { xs, ys, count };
    grids.set(key, found);
  }
  return found;
}

export function Backdrop({ fill = C.ink, dots = '#FFFFFF14', width = W, height = H }:
  { fill?: string; dots?: string; width?: number; height?: number }) {
  const { xs, ys, count } = grid(width, height);
  return (
    <Static deps={[fill, dots, width, height]}>
      <Rect width={width} height={height} fill={fill} />
      <Instances count={count} x={xs} y={ys} width={3} height={3} shape="circle" fill={dots} />
    </Static>
  );
}
