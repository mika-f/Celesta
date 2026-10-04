import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Text } from './components';
import type { CommonProps } from './components';
import type { TextStyle } from './scene';
import {
  asynchronousMeasurer,
  prepareFonts,
  synchronousMeasurer,
  useMeasurementFonts,
} from './text-measure';
import type { MeasureTextRequest, TextMetrics } from './text-measure';

/** The text style a fitted box draws with; the box chooses `fontSize` and `lineHeight`. */
export type FitTextStyle = Omit<TextStyle, 'fontSize' | 'lineHeight'>;

export interface FitTextOptions {
  /** Box width in pixels. Lines wrap at it, like `<Text maxWidth>`. */
  width: number;
  /** Box height in pixels. */
  height: number;
  /** The smallest font size to use, in px. */
  minFontSize: number;
  /** The largest font size to use, in px; text that fits at it uses it. */
  maxFontSize: number;
  /** Most lines the text may take, a whole number ≥ 1. Defaults to no limit. */
  maxLines?: number;
  /**
   * Line height as a multiple of the font size, like CSS's unitless
   * `line-height`, so it shrinks with the text. Defaults to the font's own.
   */
  lineHeight?: number;
  /** Font sizes tried are `minFontSize + n * step`, and `maxFontSize`. Defaults to 1. */
  step?: number;
  style?: FitTextStyle;
  /** Extra font files to load before measuring, as for `measureText()`. */
  fonts?: string[];
}

export interface FitTextResult {
  /**
   * A font size that fits, or `minFontSize` when none does. It is the
   * largest that fits unless wrapping lets a larger size fit after a smaller
   * one does not, which the bisection can miss.
   */
  fontSize: number;
  /** `options.style` with `fontSize` (and `lineHeight`) set: pass it to `<Text>`. */
  style: TextStyle;
  /** The text measured at `fontSize`, wrapped at `options.width`. */
  metrics: TextMetrics;
  /** Whether the text fits the box and `maxLines` at `fontSize`. */
  fits: boolean;
}

/**
 * Finds the largest font size that fits `text` inside the box, during an
 * entry's async `prepare()`. Draw it with `<Text maxWidth={width}
 * style={result.style}>`.
 */
export async function fitText(text: string, options: FitTextOptions): Promise<FitTextResult> {
  const measure = asynchronousMeasurer('fitText()');
  const fonts = prepareFonts(options.fonts);
  const search = searchFontSize(options);
  let step = search.next();
  while (!step.done) {
    step = search.next(await measure(fitRequest(text, options, step.value, fonts)));
  }
  return step.value;
}

/**
 * Finds the largest font size that fits `text` inside the box during render,
 * with the same fonts and shaping as `<Text>`. Unchanged inputs reuse this
 * hook's last result.
 */
export function useFitText(text: string, options: FitTextOptions): FitTextResult {
  const fonts = useMeasurementFonts(options.fonts);
  const key = JSON.stringify({ text, options: { ...options, fonts: undefined }, fonts });
  return React.useMemo(() => {
    const { text, options, fonts } = JSON.parse(key) as {
      text: string;
      options: FitTextOptions;
      fonts: MeasureTextRequest['fonts'];
    };
    const measure = synchronousMeasurer('useFitText()');
    const search = searchFontSize(options);
    let step = search.next();
    while (!step.done) {
      step = search.next(measure(fitRequest(text, options, step.value, fonts)));
    }
    return step.value;
  }, [key]);
}

function fitRequest(
  text: string,
  options: FitTextOptions,
  fontSize: number,
  fonts: MeasureTextRequest['fonts'],
): MeasureTextRequest {
  return { text, style: fittedStyle(options, fontSize), maxWidth: options.width, fonts };
}

function fittedStyle(options: FitTextOptions, fontSize: number): TextStyle {
  const style: TextStyle = { ...options.style, fontSize };
  delete style.lineHeight;
  if (options.lineHeight !== undefined) {
    style.lineHeight = options.lineHeight * fontSize;
  }
  return style;
}

/**
 * Yields font sizes to measure and receives their metrics. Tries the largest
 * size first and the smallest second, so short text takes one measurement
 * and text that cannot fit takes two; otherwise bisects between them.
 */
function* searchFontSize(options: FitTextOptions): Generator<number, FitTextResult, TextMetrics> {
  validate(options);
  const { width, height, minFontSize, maxFontSize, maxLines } = options;
  const step = options.step ?? 1;
  const last = Math.max(0, Math.ceil((maxFontSize - minFontSize) / step - 1e-9));
  // Rounded so 0.1 steps give 24.3, not 24.299999999999997.
  const size = (index: number) => index >= last
    ? maxFontSize
    : Math.round((minFontSize + index * step) * 1e6) / 1e6;
  // Shaping works in f32, so allow its rounding at the edges.
  const fits = (metrics: TextMetrics) => metrics.width <= width + 1e-3
    && metrics.height <= height + 1e-3
    && (maxLines === undefined || metrics.lines <= maxLines);
  const result = (index: number, metrics: TextMetrics, fitted: boolean): FitTextResult => ({
    fontSize: size(index),
    style: fittedStyle(options, size(index)),
    metrics,
    fits: fitted,
  });

  const largest = yield size(last);
  if (fits(largest) || last === 0) {
    return result(last, largest, fits(largest));
  }
  const smallest = yield size(0);
  if (!fits(smallest)) {
    return result(0, smallest, false);
  }
  let low = 0;
  let lowMetrics = smallest;
  let high = last;
  while (high - low > 1) {
    const middle = Math.floor((low + high) / 2);
    const metrics = yield size(middle);
    if (fits(metrics)) {
      low = middle;
      lowMetrics = metrics;
    } else {
      high = middle;
    }
  }
  return result(low, lowMetrics, true);
}

function validate(options: FitTextOptions): void {
  const positive = (name: keyof FitTextOptions, value: unknown) => {
    if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
      throw new RangeError(`fitText: ${name} must be a finite number greater than 0, got ${String(value)}`);
    }
  };
  positive('width', options.width);
  positive('height', options.height);
  positive('minFontSize', options.minFontSize);
  positive('maxFontSize', options.maxFontSize);
  if (options.maxFontSize < options.minFontSize) {
    throw new RangeError(
      `fitText: maxFontSize (${options.maxFontSize}) must not be less than minFontSize (${options.minFontSize})`,
    );
  }
  if (options.maxLines !== undefined && (!Number.isInteger(options.maxLines) || options.maxLines < 1)) {
    throw new RangeError(`fitText: maxLines must be a whole number of at least 1, got ${String(options.maxLines)}`);
  }
  if (options.lineHeight !== undefined) {
    positive('lineHeight', options.lineHeight);
  }
  if (options.step !== undefined) {
    positive('step', options.step);
  }
}

export interface TextBoxProps
  extends Omit<CommonProps, 'anchorX' | 'anchorY'>, Omit<FitTextOptions, 'fonts'> {
  /** Strings or numbers, as for `<Text>`; `null`, `undefined`, and booleans draw nothing. */
  children: ReactNode;
  /** Where the text sits in the box's height when it is shorter. Defaults to `'top'`. */
  verticalAlign?: 'top' | 'middle' | 'bottom';
  /**
   * What happens when the text does not fit even at `minFontSize`:
   * `'clip'` (the default) draws it at `minFontSize` and cuts it off at the
   * box's edges and after `maxLines`, aligning the lines it keeps;
   * `'visible'` draws all its lines, past the box's height, aligned by
   * `verticalAlign` when they fit the height (a word wider than `width` is
   * still cut off at it, as with `<Text maxWidth>`); `'error'` fails the
   * render.
   */
  overflow?: 'clip' | 'visible' | 'error';
}

/**
 * Text drawn at the largest font size from `minFontSize` to `maxFontSize`
 * that fits `width`, `height`, and `maxLines`. The box's top-left corner is
 * at `x`/`y`; `style.align` positions each line inside `width`.
 */
export function TextBox(props: TextBoxProps): ReturnType<typeof React.createElement> {
  const {
    children, width, height, minFontSize, maxFontSize, maxLines, lineHeight, step, style,
    verticalAlign = 'top', overflow = 'clip', ...common
  } = props;
  const text = textBoxText(children);
  const fit = useFitText(text, { width, height, minFontSize, maxFontSize, maxLines, lineHeight, step, style });
  if (!fit.fits && overflow === 'error') {
    const preview = text.length > 40 ? `${text.slice(0, 40)}…` : text;
    const lines = maxLines === undefined ? '' : ` in ${maxLines} line${maxLines === 1 ? '' : 's'}`;
    throw new Error(
      `<TextBox> text "${preview}" does not fit ${width}×${height}${lines} even at minFontSize ${minFontSize}`,
    );
  }
  const clipped = !fit.fits && overflow === 'clip';
  // Clipped text shows its first `maxLines` lines, aligned like fitting text.
  const shown = !clipped
    ? fit.metrics.height
    : maxLines !== undefined && fit.metrics.lines > maxLines
      ? Math.min(height, maxLines * fit.metrics.lineHeight)
      : height;
  const room = Math.max(0, height - shown);
  const top = verticalAlign === 'middle' ? room / 2 : verticalAlign === 'bottom' ? room : 0;
  return React.createElement(
    Group,
    { ...common, ...(clipped ? { clip: { y: top, width, height: shown } } : {}) },
    text === ''
      ? null
      : React.createElement(Text, {
        y: top + fit.metrics.ascent,
        anchorY: 'baseline',
        maxWidth: width,
        style: fit.style,
        children: text,
      }),
  );
}

function textBoxText(children: ReactNode): string {
  if (children === null || children === undefined || typeof children === 'boolean') {
    return '';
  }
  if (typeof children === 'string' || typeof children === 'number') {
    return String(children);
  }
  if (Array.isArray(children)) {
    return children.map(textBoxText).join('');
  }
  throw new Error('<TextBox> children must be a string, a number, or an array of those');
}
