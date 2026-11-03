import type { PathCommand } from '@celesta.studio/react';
import { heartPath, polygonPath, starPath, trianglePath } from '@celesta.studio/shapes';

// Outlines centred on 0,0, for morphing one into another: the shape
// generators measure from their box's top-left corner.
export function centered(commands: readonly PathCommand[], width: number, height: number): PathCommand[] {
  const dx = -width / 2;
  const dy = -height / 2;
  return commands.map((c) => {
    switch (c.type) {
      case 'moveTo':
      case 'lineTo':
        return { ...c, x: c.x + dx, y: c.y + dy };
      case 'quadTo':
        return { ...c, x1: c.x1 + dx, y1: c.y1 + dy, x: c.x + dx, y: c.y + dy };
      case 'cubicTo':
        return { ...c, x1: c.x1 + dx, y1: c.y1 + dy, x2: c.x2 + dx, y2: c.y2 + dy, x: c.x + dx, y: c.y + dy };
      default:
        return c;
    }
  });
}

/** The Δ, a star, a heart and a hexagon, all about `size` across. */
export function outlines(size: number) {
  const r = size / 2;
  const h = size * Math.sqrt(3) / 2;
  return {
    delta: centered(trianglePath({ width: size, height: h }), size, h),
    star: centered(starPath({ radius: r * 1.05, points: 5, innerRatio: 0.46 }), size * 1.05, size * 1.05),
    heart: centered(heartPath({ width: size, height: size * 0.9 }), size, size * 0.9),
    hexagon: centered(polygonPath({ radius: r, sides: 6 }), size, size),
  };
}
