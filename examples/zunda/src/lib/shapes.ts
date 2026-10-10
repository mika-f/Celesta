// 点の列で表した図形。<Path points> や <Polyline points> にそのまま渡せる。
// どれも始点と終点が同じ（閉じている）ので、<Polyline progress> で一筆書きに描ける。

export type Point = [number, number];

/** 楕円。真上から時計回りに `segments` 個の線分でたどる。 */
export function ellipse(cx: number, cy: number, rx: number, ry: number, segments = 72): Point[] {
  return Array.from({ length: segments + 1 }, (_, i) => {
    const angle = -Math.PI / 2 + (i / segments) * Math.PI * 2;
    return [cx + rx * Math.cos(angle), cy + ry * Math.sin(angle)];
  });
}

/** 縁がでこぼこした楕円（ずんだあん）。`seed` ででこぼこの位置が変わる。 */
export function blob(cx: number, cy: number, rx: number, ry: number, seed: number, segments = 96): Point[] {
  return Array.from({ length: segments + 1 }, (_, i) => {
    const angle = -Math.PI / 2 + (i / segments) * Math.PI * 2;
    const r = 1 + 0.07 * Math.sin(9 * angle + seed) + 0.04 * Math.sin(5 * angle + seed * 2.3);
    return [cx + rx * r * Math.cos(angle), cy + ry * r * Math.sin(angle)];
  });
}

/** えだまめのさや：3 粒ぶんふくらんだ細長い形。`angle` は度。 */
export function pod(cx: number, cy: number, length: number, angle: number, segments = 48): Point[] {
  const top: Point[] = [];
  const bottom: Point[] = [];
  for (let i = 0; i <= segments; i++) {
    const t = i / segments;
    const taper = Math.pow(Math.sin(Math.PI * t), 0.45);
    const thickness = 30 * taper * (0.78 + 0.22 * Math.abs(Math.sin(3 * Math.PI * t)));
    const x = (t - 0.5) * length;
    top.push([x, -thickness]);
    bottom.push([x, thickness * 0.85]);
  }
  const radians = (angle * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  return [...top, ...bottom.reverse(), top[0]].map(([x, y]) => [cx + x * cos - y * sin, cy + x * sin + y * cos]);
}
