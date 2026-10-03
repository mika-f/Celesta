import { Rect } from '@celesta/react';

// The same evolving signal recurs in the source scene and final title.
export function Signal({ frame, top, color, count = 96, amplitude = 150 }: {
  frame: number; top: number; color: string; count?: number; amplitude?: number;
}) {
  return <>{Array.from({ length: count }, (_, i) => {
    const u = i / (count - 1);
    const envelope = Math.sin(u * Math.PI) ** 2;
    const phase = u * 21 - frame * 0.14;
    const y = top + Math.sin(phase) * amplitude * envelope
      + Math.sin(phase * 0.49 + frame * 0.045) * amplitude * 0.26;
    const h = 20 + 140 * Math.abs(Math.sin(phase * 0.58)) * envelope;
    return <Rect key={i} x={72 + i * (1776 / (count - 1))} y={y - h / 2}
      width={i % 7 === 0 ? 8 : 4} height={h} fill={color}
      opacity={0.28 + 0.72 * envelope} />;
  })}</>;
}
