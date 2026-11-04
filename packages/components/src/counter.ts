// A number that counts to its value, printed as text.

import * as React from 'react';

import { Easings, Text, useCurrentFrame } from '@celesta.studio/react';
import type { TextProps } from '@celesta.studio/react';

import { countProgress, finiteValue, formatNumber } from './count';
import type { CountTiming, NumberFormat } from './count';

export interface CounterProps extends Omit<TextProps, 'children'>, NumberFormat, CountTiming {
  /** The number it counts to. */
  value: number;
  /** The number it starts from. Defaults to 0; larger than `value` counts down. */
  from?: number;
}

/**
 * A `<Text>` that counts from `from` to `value` over `durationInFrames`,
 * starting at `delay`, printed with `formatNumber()`: rounded to
 * `decimals`, grouped by `locale` (`'en-US'` unless given), between
 * `prefix` and `suffix`, or printed by `formatter`.
 *
 * Every other prop is `<Text>`'s. The text grows as digits are added from
 * the anchor: `anchorX={1}` keeps the last digit and the suffix in place,
 * `anchorX={0}` (the default) the first. A proportional font's digits
 * differ in width, so the other edge still moves a little as they change;
 * a monospaced font keeps every digit the same width.
 *
 * `value` and `from` must be finite.
 *
 * ```tsx
 * <Counter x={1800} y={200} anchorX={1} value={1280} durationInFrames={30}
 *   suffix=" 件" fontSize={96} fontWeight={700} color="ink" />
 * ```
 */
export function Counter(props: CounterProps): ReturnType<typeof React.createElement> {
  const {
    value, from = 0, delay = 0, durationInFrames = 30, easing = Easings.easeOutExpo,
    locale, decimals, prefix, suffix, formatter, ...text
  } = props;
  const frame = useCurrentFrame();
  const to = finiteValue(value, 'value', '<Counter>');
  const start = finiteValue(from, 'from', '<Counter>');
  const p = countProgress(frame, delay, durationInFrames, easing, '<Counter>');
  return React.createElement(Text, {
    ...text,
    children: formatNumber(start + (to - start) * p, { locale, decimals, prefix, suffix, formatter }, '<Counter>'),
  });
}
