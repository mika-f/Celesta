// Shared constants and helpers for SPECTRA. Every value is derived from the
// frame, so any frame renders on its own (the bench and the exporter request
// frames out of order).
import type { PathCommand, TextStyle } from '@celesta/react';
import { random } from '@celesta/math';

export const W = 1920;
export const H = 1080;
export const FPS = 60;

/** Each chapter's length, and how much it overlaps the one before it. */
export const CHAPTER = 150;
export const OVERLAP = 15;
export const CHAPTERS = ['IGNITION', 'GEOMETRY', 'TYPOGRAPHY', 'SOURCE', 'SIGNAL'] as const;
export const DURATION = CHAPTERS.length * CHAPTER - (CHAPTERS.length - 1) * OVERLAP;

export const PALETTE = ['#FF3D7F', '#3DA5FF', '#7B5CFF', '#00E0B8', '#FFB13D', '#FF6B3D'];
export const INK = '#E8ECFF';
export const DIM = '#8C96C8';

export const DISPLAY = 'Bebas Neue';
export const MONO = 'IBM Plex Mono';

/** Deterministic random in [0, 1) for item `i`, channel `k`. */
export const rand = (i: number, k: number) => random(i * 7919 + k * 104729);

export const style = (fontFamily: string, fontSize: number, color: string, extra: TextStyle = {}): TextStyle => ({
  fontFamily,
  fontSize,
  fill: { type: 'solid', color },
  ...extra,
});

/** A regular polygon (or a star, when `inner` is set) around the origin. */
export const polygon = (points: number, radius: number, inner?: number): [number, number][] =>
  Array.from({ length: inner === undefined ? points : points * 2 }, (_, i) => {
    const r = inner !== undefined && i % 2 === 1 ? inner : radius;
    const a = (i / (inner === undefined ? points : points * 2)) * Math.PI * 2 - Math.PI / 2;
    return [Math.cos(a) * r, Math.sin(a) * r];
  });

/** An annular sector from `start` to `end` (radians), as cubic Béziers. */
export const arc = (inner: number, outer: number, start: number, end: number): PathCommand[] => {
  const segments = Math.max(1, Math.ceil(Math.abs(end - start) / (Math.PI / 2)));
  const commands: PathCommand[] = [];
  // Quarter turns at most, each a cubic with tangent handles k * r long.
  const sweep = (r: number, from: number, to: number) => {
    const by = (to - from) / segments;
    const k = (4 / 3) * Math.tan(by / 4) * r;
    for (let s = 0; s < segments; s++) {
      const a0 = from + s * by;
      const a1 = a0 + by;
      commands.push({
        type: 'cubicTo',
        x1: Math.cos(a0) * r - Math.sin(a0) * k,
        y1: Math.sin(a0) * r + Math.cos(a0) * k,
        x2: Math.cos(a1) * r + Math.sin(a1) * k,
        y2: Math.sin(a1) * r - Math.cos(a1) * k,
        x: Math.cos(a1) * r,
        y: Math.sin(a1) * r,
      });
    }
  };
  commands.push({ type: 'moveTo', x: Math.cos(start) * outer, y: Math.sin(start) * outer });
  sweep(outer, start, end);
  commands.push({ type: 'lineTo', x: Math.cos(end) * inner, y: Math.sin(end) * inner });
  sweep(inner, end, start);
  commands.push({ type: 'close' });
  return commands;
};
