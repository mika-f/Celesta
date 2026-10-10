import { Circle } from '@celesta/shapes';

// A soft light: a radial gradient from a white core through `color` to
// nothing. Cheaper than `glow` on dozens of dots.
export function Gem({ x, y, radius, color, opacity = 1 }: {
  x: number; y: number; radius: number; color: string; opacity?: number;
}) {
  return <Circle x={x} y={y} radius={radius} anchorX={0.5} anchorY={0.5} opacity={opacity} fill={{
    type: 'radial', center: { x: radius, y: radius }, radius,
    stops: [
      { offset: 0, color: '#FFFFFF' },
      { offset: 0.25, color },
      { offset: 1, color: `${color}00` },
    ],
  }} />;
}
