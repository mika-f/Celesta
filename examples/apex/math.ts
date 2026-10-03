// Small 3D helpers: rotate a point about X then Y, and project it with simple perspective.

export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
export const rot = ([x, y, z]: number[], ax: number, ay: number): number[] => {
  const y1 = y * Math.cos(ax) - z * Math.sin(ax), z1 = y * Math.sin(ax) + z * Math.cos(ax);
  return [x * Math.cos(ay) + z1 * Math.sin(ay), y1, -x * Math.sin(ay) + z1 * Math.cos(ay)];
};
export const proj = (p: number[], cx: number, cy: number, d = 1400): [number, number] => {
  const k = d / (d + p[2]);
  return [cx + p[0] * k, cy + p[1] * k];
};
