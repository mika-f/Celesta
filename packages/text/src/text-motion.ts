// Text animation: lines that rise from behind a mask, typing, and counting.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Easings, Group, Text, progress, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import type { CommonProps, TextStyle } from '@celesta.studio/react';
import {
  CompositionRuntimeContext,
  ThemeContext,
  flattenTextContent,
  sliceTextRuns,
  synchronousMeasurer,
  themeTextContent,
  themedTextStyle,
  useMeasurementFonts,
  withInheritedText,
  withTextRuns,
} from '@celesta.studio/react/internal';
import type { FlatText, MeasureTextRequest } from '@celesta.studio/react/internal';

export interface TextRevealProps extends CommonProps {
  /**
   * The text; may hold `<Span>`. `\n` starts a new line, and each line is
   * revealed on its own; a span crossing a line break continues on the next
   * line.
   */
  children: ReactNode;
  /** `style.lineBreak` chooses where `maxWidth` wraps, as for `<Text>`. */
  style?: TextStyle;
  /**
   * Wrap lines at this width, at the same places as `<Text maxWidth>`, and
   * reveal each wrapped line on its own. Without it, only `\n` breaks lines.
   */
  maxWidth?: number;
  /**
   * Height of each line's mask, and the distance between baselines. Defaults
   * to `style.lineHeight`, else `style.fontSize`, else 48.
   */
  lineHeight?: number;
  /**
   * Where the baseline sits inside each line box, as a fraction of
   * `lineHeight` from its top. Raise it for fonts with tall accents; lower it
   * for deep descenders. Defaults to 0.8.
   */
  baseline?: number;
  /** Horizontal pivot of every line: 0 left, 0.5 center, 1 right. Defaults to 0. */
  align?: number;
  /** Frame the first line starts moving. Defaults to 0. */
  from?: number;
  /** Frames between one line and the next. Defaults to 4. */
  stagger?: number;
  /** Frames each line takes to arrive. Defaults to 20. */
  durationInFrames?: number;
  easing?: (t: number) => number;
  /** `'in'` (default) rises into place; `'out'` sinks away. */
  direction?: 'in' | 'out';
}

/**
 * Headline lines that slide up into view from behind an invisible edge, one
 * after another: the classic editorial title reveal. Each line is masked by
 * its own line box with `<Group clip>`, so nothing shows outside it; `x`/`y`
 * place the top of the first line box.
 *
 * ```tsx
 * <TextReveal x={160} y={300} lineHeight={200} stagger={6}
 *   style={{ fontFamily: 'Archivo Black', fontSize: 210 }}>
 *   {'36 DAYS\nOF CELESTA'}
 * </TextReveal>
 * ```
 */
export function TextReveal({
  children,
  style,
  maxWidth,
  lineHeight,
  baseline = 0.8,
  align = 0,
  from = 0,
  stagger = 4,
  durationInFrames = 20,
  easing = Easings.easeOutExpo,
  direction = 'in',
  ...groupProps
}: TextRevealProps): React.ReactElement {
  const theme = React.useContext(ThemeContext);
  style = themedTextStyle(style, {}, theme, '<TextReveal>');
  const flat = flattenTextContent(themeTextContent(children, theme), '<TextReveal> children');
  withTextRuns(style ?? {}, flat, '<TextReveal>');
  const frame = useCurrentFrame();
  const { width } = useVideoConfig();
  const height = lineHeight ?? style?.lineHeight ?? style?.fontSize ?? 48;
  if (!Number.isFinite(height) || height <= 0) {
    throw new Error('<TextReveal> requires a positive `lineHeight`');
  }
  // Each line is its own single-line Text, so the multi-line spacing is ours.
  const { lineHeight: _lineHeight, ...lineStyle } = style ?? {};
  // Each line keeps the runs inside it, rebased to its first code point.
  const lines = useLineRanges(flat, style ?? {}, maxWidth).map(([start, end]) => {
    const line = sliceTextRuns(flat, start, end);
    return { text: line.text, style: withTextRuns(lineStyle, line, '<TextReveal>') };
  });
  // Line widths are not needed, so the mask spans far past any line's width.
  const maskWidth = width * 4;
  return React.createElement(
    Group,
    groupProps,
    lines.map((line, index) => {
      const p = progress(frame, from + index * stagger, durationInFrames, easing);
      const shown = direction === 'in' ? p : 1 - p;
      return React.createElement(
        Group,
        { key: index, y: index * height, clip: { x: -maskWidth / 2, width: maskWidth, height } },
        React.createElement(Text, {
          y: height * baseline + height * (1 - shown),
          anchorX: align,
          anchorY: 'baseline',
          style: line.style,
          children: line.text,
        }),
      );
    }),
  );
}

/**
 * The code-point range of each line: split at `\n`, and also wrapped at
 * `maxWidth` by measuring the whole text as `<Text maxWidth>` lays it out.
 * The `\n` ending a line, and the spaces before a wrap, belong to no line.
 */
function useLineRanges(flat: FlatText, style: TextStyle, maxWidth: number | undefined): [number, number][] {
  if (maxWidth !== undefined && (!Number.isFinite(maxWidth) || maxWidth <= 0)) {
    throw new Error('<TextReveal> requires a positive `maxWidth`');
  }
  const runtime = React.useContext(CompositionRuntimeContext);
  const fonts = useMeasurementFonts();
  // Color does not move a break, so a span whose color animates reuses the
  // measurement, as `useTextMetrics` does.
  const { colorRuns, visibleCharacters, fill, stroke, ...layoutStyle } = withTextRuns(
    withInheritedText(style, runtime ?? {}), { ...flat, colorRuns: [] }, '<TextReveal>',
  );
  const key = maxWidth === undefined
    ? null
    : JSON.stringify({ text: flat.text, style: layoutStyle, maxWidth, fonts });
  const lineStarts = React.useMemo(() => {
    if (key === null) return null;
    const metrics = synchronousMeasurer('<TextReveal maxWidth>')(JSON.parse(key) as MeasureTextRequest);
    if (!metrics.lineStarts) {
      throw new Error('<TextReveal maxWidth> requires a text measurer that reports line starts');
    }
    return metrics.lineStarts;
  }, [key]);

  const characters = Array.from(flat.text);
  const starts = lineStarts ?? [0, ...characters.flatMap((c, i) => c === '\n' ? [i + 1] : [])];
  return starts.map((start, index) => {
    let end = starts[index + 1] ?? characters.length;
    if (characters[end - 1] === '\n') {
      end -= 1;
    } else if (index + 1 < starts.length) {
      while (end > start && /\s/u.test(characters[end - 1])) end -= 1;
    }
    return [start, end];
  });
}

export interface TypewriterOptions {
  /** Frame typing starts. Defaults to 0. */
  from?: number;
  /** Frames per character; below 1 types several characters a frame. Defaults to 1. */
  framesPerChar?: number;
  /**
   * Length of one caret blink cycle (on, then off) while idle, in frames.
   * Defaults to one second. The caret stays on while typing.
   */
  blinkFrames?: number;
  /**
   * Extra frames to wait after a character before typing the next one, by
   * character: `{ '、': 4, '。': 10 }`. Each name is one character (one code
   * point). The caret stays steady during a pause.
   */
  pauses?: Readonly<Record<string, number>>;
}

export interface Typewriter {
  /** The characters typed so far. */
  text: string;
  /** How many characters that is (by code point, so emoji and kana count once). */
  length: number;
  /** True once the whole text is typed. */
  done: boolean;
  /** Whether to draw a caret on this frame. */
  caretVisible: boolean;
  /**
   * The first whole frame each character of the whole text is shown on, one
   * entry per code point, on the same clock as `useCurrentFrame()` (so
   * `from` is included). Characters typed on the same frame share it.
   */
  frames: number[];
}

type TypingOptions = Pick<TypewriterOptions, 'from' | 'framesPerChar' | 'pauses'>;

/**
 * Checks the typing options and returns, for each character, the pause
 * frames of the characters before it.
 */
function pausesBefore(characters: readonly string[], options: TypingOptions, caller: string): number[] {
  const { from = 0, framesPerChar = 1, pauses = {} } = options;
  if (!Number.isFinite(from)) {
    throw new Error(`${caller} requires a finite from`);
  }
  if (!Number.isFinite(framesPerChar) || framesPerChar <= 0) {
    throw new Error(`${caller} requires a positive framesPerChar`);
  }
  for (const [character, frames] of Object.entries(pauses)) {
    if (Array.from(character).length !== 1) {
      throw new Error(`${caller} requires each name in pauses to be one character, got "${character}"`);
    }
    if (!Number.isFinite(frames) || frames < 0) {
      throw new Error(`${caller} requires pauses of at least 0 frames, got ${frames} for "${character}"`);
    }
  }
  let total = 0;
  return characters.map((character) => {
    const before = total;
    if (Object.prototype.hasOwnProperty.call(pauses, character)) total += pauses[character];
    return before;
  });
}

/**
 * Whether character `index` is shown on `frame`. The one test everything
 * about typing is derived from, so the typed count and the reported frames
 * cannot disagree by a rounding error.
 */
function isTyped(index: number, frame: number, from: number, framesPerChar: number, paused: number): boolean {
  return Math.floor((frame - from - paused) / framesPerChar) >= index;
}

/** How many of `characters` are typed on `frame` (which may be fractional). */
export function typedCount(characters: readonly string[], frame: number, options: TypingOptions = {}): number {
  const { from = 0, framesPerChar = 1 } = options;
  const paused = pausesBefore(characters, options, 'useTypewriter()');
  return countTyped(characters.length, frame, from, framesPerChar, paused);
}

function countTyped(length: number, frame: number, from: number, framesPerChar: number, paused: readonly number[]): number {
  let typed = 0;
  while (typed < length && isTyped(typed, frame, from, framesPerChar, paused[typed])) typed += 1;
  return typed;
}

function typingFrames(length: number, from: number, framesPerChar: number, paused: readonly number[]): number[] {
  return Array.from({ length }, (_, index) => {
    let frame = Math.ceil(from + paused[index] + index * framesPerChar);
    while (isTyped(index, frame - 1, from, framesPerChar, paused[index])) frame -= 1;
    while (!isTyped(index, frame, from, framesPerChar, paused[index])) frame += 1;
    return frame;
  });
}

/**
 * The first whole frame each character of `text` is typed on, as
 * `useTypewriter(text, options).frames` reports it: one entry per code
 * point. A plain function, for where hooks cannot run and for placing
 * something on each character, such as a key click:
 *
 * ```tsx
 * const clicks = typewriterFrames('npm run build', { from: 10, framesPerChar: 2 });
 * clicks.map((at) => <Sequence key={at} from={at}><Audio src="./key.wav" /></Sequence>);
 * ```
 */
export function typewriterFrames(text: string, options: TypingOptions = {}): number[] {
  const { from = 0, framesPerChar = 1 } = options;
  const characters = Array.from(text);
  const paused = pausesBefore(characters, options, 'typewriterFrames()');
  return typingFrames(characters.length, from, framesPerChar, paused);
}

/**
 * Types `text` out over time, one character every `framesPerChar` frames,
 * with a caret that holds steady while typing and blinks while idle.
 *
 * ```tsx
 * const { text, caretVisible } = useTypewriter('npm run build', { from: 10 });
 * ```
 *
 * For typed text with a caret, use `<TypewriterText>` from
 * `@celesta.studio/components`: it times the typing with this hook and places the
 * caret on the measured layout, so any font, `\n`, and wrapping work.
 *
 * `content` may hold `<Span>`: pass `length` to `style.visibleCharacters` of
 * a `<Text>` with the same content to reveal it without changing its layout.
 */
export function useTypewriter(content: ReactNode, options: TypewriterOptions = {}): Typewriter {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const { from = 0, framesPerChar = 1, blinkFrames = fps } = options;
  if (!Number.isFinite(blinkFrames) || blinkFrames <= 0) {
    throw new Error('useTypewriter() requires a positive blinkFrames');
  }
  // Span colors are checked as they flatten, so names must be looked up first.
  const theme = React.useContext(ThemeContext);
  const characters = Array.from(flattenTextContent(themeTextContent(content, theme), 'useTypewriter() text').text);
  const paused = pausesBefore(characters, options, 'useTypewriter()');
  const typed = countTyped(characters.length, frame, from, framesPerChar, paused);
  const typing = frame >= from && typed < characters.length;
  const phase = (((frame - from) % blinkFrames) + blinkFrames) % blinkFrames;
  return {
    text: characters.slice(0, typed).join(''),
    length: typed,
    done: typed === characters.length,
    caretVisible: typing || phase < blinkFrames / 2,
    frames: typingFrames(characters.length, from, framesPerChar, paused),
  };
}

export interface CountUpOptions {
  /** Starting value. Defaults to 0. */
  from?: number;
  /** Frame counting starts. Defaults to 0. */
  delay?: number;
  /** Frames to reach the target. Defaults to 30. */
  durationInFrames?: number;
  /** Defaults to `Easings.easeOutExpo`: fast at first, settling on the value. */
  easing?: (t: number) => number;
  /** Decimal places to round to. Defaults to 0. */
  decimals?: number;
}

/**
 * A number that counts up (or down) to `to`, rounded to `decimals`. Format it
 * yourself, for example with `toLocaleString('en-US')` for thousands
 * separators.
 */
export function useCountUp(to: number, options: CountUpOptions = {}): number {
  const frame = useCurrentFrame();
  const { from = 0, delay = 0, durationInFrames = 30, easing = Easings.easeOutExpo, decimals = 0 } = options;
  if (!Number.isInteger(decimals) || decimals < 0) {
    throw new Error('useCountUp() requires a non-negative integer decimals');
  }
  const value = from + (to - from) * progress(frame, delay, durationInFrames, easing);
  const factor = 10 ** decimals;
  return Math.round(value * factor) / factor;
}
