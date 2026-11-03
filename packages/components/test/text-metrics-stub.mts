// A stand-in for the shaper, installed with setTextMeasurer(): Japanese
// characters are a full em wide, others half an em (0.6 em in "mono"); `\n`
// breaks lines; the line box is 1.25 em with the baseline 1 em down.

import type { TextMetrics, TextStyle } from '@celesta.studio/react';
import { setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';

function advance(character: string, style: TextStyle): number {
  const size = style.fontSize ?? 20;
  if (/[　-鿿]/u.test(character)) return size;
  return size * (style.fontFamily === 'mono' ? 0.6 : 0.5);
}

export function stubMetrics(request: MeasureTextRequest): TextMetrics {
  const { text, style } = request;
  const size = style.fontSize ?? 20;
  const lineHeight = style.lineHeight ?? size * 1.25;
  const lines = text.split('\n');
  const widths = lines.map((line) => Array.from(line).reduce((sum, c) => sum + advance(c, style), 0));
  return {
    width: Math.max(...widths), height: lines.length * lineHeight,
    ascent: size, descent: lineHeight - size, lineHeight, capHeight: size * 0.7, xHeight: size * 0.5,
    lines: lines.length, glyphs: [],
  };
}

/** Installs the stub; returns the requests it measures. */
export function installStubMeasurer(): MeasureTextRequest[] {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async (request) => stubMetrics(request), (request) => {
    requests.push(request);
    return stubMetrics(request);
  });
  return requests;
}
