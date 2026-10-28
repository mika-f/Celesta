// Timed captions from data alone: each cue is shown on its frames
// `[from, from + durationInFrames)` on a Panel band, wrapped and limited to a
// number of lines by @celesta/text, with emphasis by grapheme range, and
// entering and leaving through Motion. A render callback can replace the
// band and text without changing the cue's clock. With `highlight` or
// `paging`, a cue is split into words, shown a page at a time, and the word
// being spoken is lit.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Rect, Sequence, Span, Subtitles, Text, useCurrentFrame, useTheme, useVideoConfig } from '@celesta/react';
import type { FontFamily, GlyphMetrics, Paint, TextMetrics, TextStyle } from '@celesta/react';
import { safeAreaInsets, useLayoutBounds } from '@celesta/layout';
import type { Insets, SafeAreaValue } from '@celesta/layout';
import {
  emphasisContent, graphemes, resolveEmphasis, resolveTimedRanges, sliceEmphasis, timedRangeAt, useFitText, useTextMeasurer,
} from '@celesta/text';
import type { EmphasisStyle, FitTextStyle, ResolvedEmphasis, TextEmphasis, TimedRange } from '@celesta/text';

import { paginate, timedWords } from './caption-words';
import type { CaptionPage, CaptionPaging, CaptionWord, CaptionWordUnit, TimedWord } from './caption-words';
import { hexColor } from './color';
import { Motion } from './motion';
import type { MotionEffect } from './motion';
import { Panel } from './panel';
import type { PanelProps } from './panel';
import { validators } from './validate';

/** One caption: the `SubtitleCue` shape `parseSubtitles()` returns, plus emphasis. */
export interface CaptionCue {
  /** Names the cue in errors and in the render callback. */
  id?: string;
  /** First frame the cue is shown on, a whole number, counted like the track's own frames. */
  from: number;
  /** Frames it is shown, a whole number of at least 1. */
  durationInFrames: number;
  /** Lines are separated by `\n`; longer lines wrap. An empty text shows no band. */
  text: string;
  /** Parts of `text` to emphasize, in grapheme clusters; see `emphasisContent()`. */
  emphasis?: readonly TextEmphasis[];
  /**
   * When each word is spoken, for `highlight` and `paging`, in order. Without
   * them, the cue's frames are shared out among its words by length.
   */
  words?: readonly CaptionWord[];
}

/** How `CaptionTrack` lights the word being spoken. */
export interface CaptionHighlight {
  /** Hex color or `<Theme>` color name of the word being spoken. Defaults to `#ffd166`. */
  color?: string;
  /** Color of the words of the page already spoken. Defaults to the track's `color`. */
  spokenColor?: string;
  /** Color of the words of the page not spoken yet. Defaults to the track's `color`. */
  upcomingColor?: string;
  /** Grows the word being spoken about its centre, over the other words. Defaults to 1. */
  scale?: number;
  /** A band behind the word being spoken: a hex color, `<Theme>` color name, or `Paint`. Defaults to none. */
  background?: string | Paint | null;
  /** Space between the word and its band. Defaults to 0.08 em above and below, 0.12 em at the sides. */
  padding?: number | Insets;
  /** Rounds the word's band. Defaults to 0.15 em. */
  cornerRadius?: number;
  /**
   * Frames the band takes to slide from the word before, and the word to
   * grow to `scale`; 0 cuts. Defaults to a tenth of a second.
   */
  transitionInFrames?: number;
}

/** A word of the page `CaptionTrack.render` draws. */
export interface CaptionWordState {
  /** The word's grapheme clusters in the render props' `text`, `[start, end)`. */
  start: number;
  end: number;
  text: string;
  /** The frames it is spoken on, `[from, until)`, counted like the render props' `frame`. */
  from: number;
  until: number;
  state: 'spoken' | 'current' | 'upcoming';
}

/** What `CaptionTrack.render` draws from. */
export interface CaptionRenderProps {
  cue: CaptionCue;
  /** The cue's index in `cues`. */
  index: number;
  /** The text shown: the cue's, or with `highlight` or `paging`, its current page's. */
  text: string;
  /** `text` with its emphasis and highlight as `<Span>`s: draw it with `<Text style={style}>`. */
  content: ReactNode;
  /** The emphasis of `text`, checked and in order, with styles looked up. */
  emphasis: ResolvedEmphasis[];
  /** The words of `text`; empty without `highlight` or `paging`. */
  words: CaptionWordState[];
  /** The page shown, counted from 0, and how many pages the cue has: 0 and 1 without paging. */
  page: number;
  pages: number;
  /** Frames since the cue appeared; the same as `useCurrentFrame()` inside the callback. */
  frame: number;
  /** Frames the cue is shown, after an `overlap: 'latest'` cut. */
  durationInFrames: number;
  /** The text style, at the size the text was fitted to. */
  style: TextStyle;
  /** `content` measured with `style`, wrapped at `maxWidth`. */
  metrics: TextMetrics;
  maxWidth: number;
  /** Whether the text fits `maxLines` at `style.fontSize`. */
  fits: boolean;
  /** Where the default band would be, in the track's coordinates (the layout area's). */
  box: { x: number; y: number; width: number; height: number };
}

export interface CaptionTrackProps {
  /** The captions. Frames count from the enclosing `<Sequence>`'s start. */
  cues: readonly CaptionCue[];
  /** Emphasis styles by name, for emphasis whose `style` is a string. */
  emphasisStyles?: Readonly<Record<string, EmphasisStyle>>;
  /**
   * What cues sharing frames mean: `'error'` (the default) throws, naming
   * both; `'latest'` lets the cue starting later cut the other short.
   */
  overlap?: 'error' | 'latest';
  /** Where the band sits in the layout area. Defaults to `'bottom'`. */
  position?: 'top' | 'middle' | 'bottom';
  /** Horizontal placement of the band and its lines. Defaults to `'center'`. */
  align?: 'left' | 'center' | 'right';
  /** Keeps the band clear of platform UI, as `<SafeArea padding>` does, e.g. `"vertical-social"`. */
  safeArea?: SafeAreaValue;
  /** Space between the area (inside `safeArea`) and the band. Defaults to 6% of the area's shorter side. */
  margin?: number | Insets;
  /** Widest the text may be before it wraps. Defaults to 80% of the room inside the margin. */
  maxWidth?: number;
  /** Most lines a caption, or with paging a page, takes. Defaults to 2. */
  maxLines?: number;
  /**
   * Lights the word being spoken, by the cue's `words` or its frames shared
   * out by length. The band and `scale` need per-glyph metrics, which the
   * browser renderer does not give; there it changes only colors.
   */
  highlight?: CaptionHighlight;
  /**
   * Splits each cue into pages that are shown in turn, each from its first
   * word: a page ends at `maxLines`, and at the rules here. `highlight`
   * pages by the defaults on its own.
   */
  paging?: CaptionPaging;
  /**
   * The words of a cue: `'word'` (the default) splits at spaces; `'phrase'`
   * also splits Japanese into phrases (文節), as `lineBreak: 'phrase'`
   * wraps, and joins given `words` that do not start one to the word
   * before.
   */
  wordUnit?: CaptionWordUnit;
  /** Font size. Defaults to 4.4% of the area's shorter side (48 px at 1080p). */
  fontSize?: number;
  /** The smallest size a caption shrinks to to fit `maxLines`. Defaults to `fontSize`: no shrinking. */
  minFontSize?: number;
  /** Line height as a multiple of the font size. Defaults to 1.4. */
  lineHeight?: number;
  /**
   * A caption that does not fit `maxLines` even at `minFontSize`: `'clip'`
   * (the default) shows its first `maxLines` lines; `'error'` fails the render.
   */
  overflow?: 'clip' | 'error';
  /** Family name, font stack, or `<Theme>` font name. */
  fontFamily?: FontFamily;
  /** Hex color or `<Theme>` color name of the text. Defaults to `#ffffff`. */
  color?: string;
  /**
   * Extra text style, over the defaults (weight 500, `lineBreak: 'phrase'`
   * so Japanese wraps between phrases); the track sets the size, line
   * height, and alignment.
   */
  style?: FitTextStyle;
  /** The band: a hex color, `<Theme>` color name, or `Paint`; `null` for none. Defaults to `#000000b3`. */
  background?: string | Paint | null;
  /** Space between the band's edges and the text. Defaults to 0.3 em above and below, 0.6 em at the sides. */
  padding?: number | Insets;
  /** Rounds the band. Defaults to 0.2 em. */
  cornerRadius?: number;
  /** More `<Panel>` props for the band, such as `stroke` or `shadow`. */
  panel?: Omit<PanelProps, 'width' | 'height' | 'padding' | 'fill' | 'cornerRadius' | 'children'>;
  /**
   * How each caption arrives, as `<Motion enter>` takes it; `null` cuts.
   * Defaults to a fade over a tenth of a second. A cue that starts where
   * the one before it ends cuts in, so back-to-back captions swap without
   * a blink.
   */
  enter?: MotionEffect | null;
  /** How each caption leaves; `null` cuts. Defaults to a fade over a tenth of a second, skipped before a cue that follows at once. */
  exit?: MotionEffect | null;
  /**
   * Draws a caption in place of the band and text; called as a component,
   * so it may use hooks, inside the cue's own `<Sequence>` and its Motion.
   */
  render?: (props: CaptionRenderProps) => ReactNode;
  /**
   * Declares the cues with `<Subtitles>` so an export's `--subtitles` file
   * holds them. Defaults to true.
   */
  subtitles?: boolean;
  /** The language `<Subtitles lang>` declares the cues in. */
  lang?: string;
}

const { fail, positive } = validators('CaptionTrack');

function cueLabel(cues: readonly CaptionCue[], index: number): string {
  const id = cues[index]?.id;
  return id === undefined ? `cue ${index}` : `cue ${index} ("${id}")`;
}

function edges(padding: number | Insets): Required<Insets> {
  if (typeof padding === 'number') return { top: padding, right: padding, bottom: padding, left: padding };
  return { top: padding.top ?? 0, right: padding.right ?? 0, bottom: padding.bottom ?? 0, left: padding.left ?? 0 };
}

/**
 * Timed captions drawn from data alone, with no `<Character>`, voice, or
 * audio: the cue on the current frame, on a band at the bottom of the
 * layout area. Cues are shown on `[from, from + durationInFrames)`; frames
 * no cue covers show nothing, and cues outside the enclosing `<Sequence>`
 * never show.
 *
 * ```tsx
 * <CaptionTrack cues={[
 *   { id: 'intro', from: 0, durationInFrames: 60, text: 'こんにちは' },
 *   { id: 'next', from: 60, durationInFrames: 90, text: '動画を作ります',
 *     emphasis: [{ start: 0, end: 2, style: { color: '#ffd166' } }] },
 * ]} />
 * ```
 */
export function CaptionTrack(props: CaptionTrackProps): ReturnType<typeof React.createElement> {
  const { cues, overlap = 'error', subtitles = true, lang } = props;
  const frame = useCurrentFrame();
  const ranges = React.useMemo(
    () => resolveTimedRanges(cues, { what: '<CaptionTrack>', label: (index) => cueLabel(cues, index), overlap }),
    [cues, overlap],
  );
  cues.forEach((cue, index) => {
    if (typeof cue.text !== 'string') fail(`${cueLabel(cues, index)} needs a string \`text\``);
  });
  const range = timedRangeAt(ranges, frame);
  const declared = subtitles
    ? React.createElement(Subtitles, {
      key: 'subtitles',
      cues: ranges.map((entry) => ({ from: entry.from, durationInFrames: entry.end - entry.from, text: cues[entry.index].text })),
      ...(lang !== undefined ? { lang } : {}),
    })
    : null;
  if (!range) return React.createElement(Group, null, declared);
  const position = ranges.indexOf(range);
  const before = ranges[position - 1];
  const after = ranges[position + 1];
  return React.createElement(
    Group,
    null,
    declared,
    React.createElement(
      Sequence,
      { key: range.index, from: range.from, durationInFrames: range.end - range.from },
      React.createElement(Caption, {
        track: props,
        range,
        joinsBefore: before !== undefined && before.end === range.from,
        joinsAfter: after !== undefined && after.from === range.end,
      }),
    ),
  );
}

function Caption({ track, range, joinsBefore, joinsAfter }: {
  track: CaptionTrackProps;
  range: TimedRange;
  joinsBefore: boolean;
  joinsAfter: boolean;
}): ReactNode {
  const bounds = useLayoutBounds();
  const canvas = useVideoConfig();
  const frame = useCurrentFrame();
  const theme = useTheme();
  const measure = useTextMeasurer();
  const side = Math.min(bounds.width, bounds.height);
  const {
    cues, emphasisStyles, position = 'bottom', align = 'center', safeArea, margin = Math.round(side * 0.06),
    maxLines = 2, fontFamily, color = '#ffffff', style, background = '#000000b3', panel, overflow = 'clip', render,
    highlight, wordUnit = 'word',
  } = track;
  const fontSize = positive('fontSize', track.fontSize ?? Math.round(side * 0.044));
  const minFontSize = Math.min(positive('minFontSize', track.minFontSize ?? fontSize), fontSize);
  const lineHeight = positive('lineHeight', track.lineHeight ?? 1.4);
  if (!Number.isInteger(maxLines) || maxLines < 1) fail(`maxLines must be a whole number of at least 1, not ${maxLines}`);
  const padding = edges(track.padding ?? { top: fontSize * 0.3, bottom: fontSize * 0.3, left: fontSize * 0.6, right: fontSize * 0.6 });
  const cornerRadius = track.cornerRadius ?? Math.round(fontSize * 0.2);
  const paged = highlight !== undefined || track.paging !== undefined;
  const paging = track.paging ?? {};
  const maxWords = paging.maxWords ?? Infinity;
  if (maxWords !== Infinity && (!Number.isInteger(maxWords) || maxWords < 1)) fail(`paging.maxWords must be a whole number of at least 1, not ${maxWords}`);
  if (paging.pause !== undefined && (!Number.isInteger(paging.pause) || paging.pause < 1)) {
    fail(`paging.pause must be a whole number of frames of at least 1, not ${paging.pause}`);
  }
  if (wordUnit !== 'word' && wordUnit !== 'phrase') fail(`wordUnit must be 'word' or 'phrase', not ${JSON.stringify(wordUnit)}`);

  const cue = cues[range.index];
  const what = `<CaptionTrack> ${cueLabel(cues, range.index)}`;
  const emphasis = resolveEmphasis(cue.text, cue.emphasis, { styles: emphasisStyles, theme, what });

  // The band's room: the area, inside the safe area, inside the margin.
  const outer = safeArea === undefined
    ? { top: 0, right: 0, bottom: 0, left: 0 }
    : safeAreaInsets(safeArea, canvas.width, canvas.height);
  const inner = safeAreaInsets(margin, canvas.width, canvas.height);
  const left = outer.left + inner.left;
  const top = outer.top + inner.top;
  const room = {
    width: Math.max(1, bounds.width - left - outer.right - inner.right),
    height: Math.max(1, bounds.height - top - outer.bottom - inner.bottom),
  };
  const maxWidth = Math.max(1, Math.min(
    track.maxWidth ?? Math.round(room.width * 0.8),
    room.width - padding.left - padding.right,
  ));
  const textStyle: FitTextStyle = {
    ...(fontFamily !== undefined ? { fontFamily } : {}),
    fontWeight: 500,
    lineBreak: 'phrase',
    fill: { type: 'solid', color: hexColor(color, theme, 'CaptionTrack', 'color') },
    ...style,
    align,
  };

  // The cue's words and pages, measured at `fontSize`; kept while the cue
  // and the layout stay the same.
  const clusters = graphemes(cue.text);
  const layoutKey = paged ? JSON.stringify({
    text: cue.text, words: cue.words, from: cue.from, end: range.end, emphasis, wordUnit, maxWords, maxLines,
    sentences: paging.sentences, pause: paging.pause, textStyle, fontSize, lineHeight, maxWidth,
  }) : '';
  const layout = React.useMemo((): { words: TimedWord[]; pages: CaptionPage[] } | undefined => {
    if (!paged) return undefined;
    const words = timedWords(cue.text, { from: cue.from, end: range.end, words: cue.words }, wordUnit, what);
    const pages = paginate(
      clusters,
      words,
      { maxWords, sentences: paging.sentences ?? true, pause: paging.pause },
      (start, end) => measure(
        emphasisContent(clusters.slice(start, end).join(''), sliceEmphasis(emphasis, start, end)),
        { ...textStyle, fontSize, lineHeight: lineHeight * fontSize },
        maxWidth,
      ).lines <= maxLines,
      cue.from,
    );
    return { words, pages };
  }, [layoutKey]);
  const now = range.from + frame;
  const pages = layout?.pages ?? [];
  let pageIndex = 0;
  while (pageIndex + 1 < pages.length && pages[pageIndex + 1].from <= now) pageIndex += 1;
  const page = pages[pageIndex];
  const text = page ? clusters.slice(page.start, page.end).join('') : cue.text;
  const shownEmphasis = page ? sliceEmphasis(emphasis, page.start, page.end) : emphasis;
  const words: CaptionWordState[] = page && layout
    ? layout.words.slice(page.first, page.last + 1).map((word) => ({
      start: word.start - page.start,
      end: word.end - page.start,
      text: clusters.slice(word.start, word.end).join(''),
      from: word.from - range.from,
      until: word.until - range.from,
      state: word.until <= now ? 'spoken' : word.from <= now ? 'current' : 'upcoming',
    }))
    : [];
  const content = emphasisContent(text, shownEmphasis);

  const fit = useFitText(content, {
    width: maxWidth,
    height: Math.max(1, room.height - padding.top - padding.bottom),
    minFontSize,
    maxFontSize: fontSize,
    maxLines,
    lineHeight,
    style: textStyle,
  });
  if (!fit.fits && overflow === 'error' && text !== '') {
    fail(`${cueLabel(cues, range.index)} "${text}" does not fit ${maxLines} line${maxLines === 1 ? '' : 's'} of ${maxWidth} px even at ${minFontSize} px`);
  }
  const shownHeight = fit.fits ? fit.metrics.height : Math.min(fit.metrics.height, maxLines * fit.metrics.lineHeight);
  const box = {
    width: fit.metrics.width + padding.left + padding.right,
    height: shownHeight + padding.top + padding.bottom,
    x: 0,
    y: 0,
  };
  box.x = left + (align === 'left' ? 0 : align === 'center' ? (room.width - box.width) / 2 : room.width - box.width);
  box.y = top + (position === 'top' ? 0 : position === 'middle' ? (room.height - box.height) / 2 : room.height - box.height);
  // Lines wrap at maxWidth with `align`; shift the text so its widest line
  // sits inside the band's padding.
  const offset = align === 'left' ? 0 : align === 'center' ? (maxWidth - fit.metrics.width) / 2 : maxWidth - fit.metrics.width;

  // The word being spoken: its colors, and where its glyphs are drawn.
  const lit = highlight === undefined ? undefined : litWord({
    highlight, words, text, emphasis: shownEmphasis, metrics: fit.metrics, style: fit.style, offset, maxWidth,
    shownLines: Math.round(shownHeight / fit.metrics.lineHeight), frame, fps: canvas.fps, theme,
  });

  let drawn: ReactNode;
  if (render) {
    drawn = React.createElement(CaptionRenderer, {
      render,
      info: {
        cue, index: range.index, text, content: lit?.content ?? content, emphasis: shownEmphasis, words,
        page: pageIndex, pages: Math.max(1, pages.length), frame, durationInFrames: range.end - range.from,
        style: fit.style, metrics: fit.metrics, maxWidth, fits: fit.fits, box,
      },
    });
  } else if (text === '') {
    drawn = null;
  } else {
    const shown = React.createElement(Text, {
      key: 'text', x: -offset, y: fit.metrics.ascent, anchorY: 'baseline', maxWidth, style: fit.style,
      children: lit?.drawnContent ?? content,
    });
    const layers = lit ? [shown, ...lit.over] : [shown];
    const clipped = shownHeight < fit.metrics.height;
    const lines = clipped
      ? React.createElement(Group, { clip: { x: -offset, y: 0, width: maxWidth, height: shownHeight } }, ...layers)
      : lit ? React.createElement(Group, null, ...layers) : shown;
    drawn = background === null
      ? React.createElement(Group, { x: box.x + padding.left, y: box.y + padding.top }, lines)
      : React.createElement(Panel, {
        ...panel,
        x: box.x, y: box.y, width: box.width, height: box.height, padding, cornerRadius, fill: background,
      }, lines);
  }
  const enter = track.enter === undefined ? { type: 'fade' as const, durationInFrames: Math.max(1, Math.round(canvas.fps / 10)) } : track.enter;
  const exit = track.exit === undefined ? { type: 'fade' as const, durationInFrames: Math.max(1, Math.round(canvas.fps / 10)) } : track.exit;
  const effects = {
    enter: enter === null || joinsBefore ? undefined : enter,
    exit: exit === null || joinsAfter ? undefined : exit,
  };
  if (effects.enter === undefined && effects.exit === undefined) return drawn;
  // Scale effects grow about the band's centre.
  return React.createElement(Motion, {
    ...effects,
    originX: box.x + box.width / 2,
    originY: box.y + box.height / 2,
  }, drawn);
}

function CaptionRenderer({ render, info }: { render: (props: CaptionRenderProps) => ReactNode; info: CaptionRenderProps }): ReactNode {
  return render(info);
}

interface LitWord {
  /** The page's text with emphasis and the words' colors. */
  content: ReactNode;
  /** The page's text as drawn beneath the band: the word being spoken in its own color, or left clear when it is drawn grown. */
  drawnContent: ReactNode;
  /** Drawn over the page's text: the word grown to `scale`, and the band with the text it covers. */
  over: ReactNode[];
}

/** One line's part of a word, from the measured glyphs, in the text's coordinates before `offset`. */
interface WordLine {
  line: number;
  x0: number;
  x1: number;
  text: string;
}

const CLEAR = '#00000000';

function litWord(options: {
  highlight: CaptionHighlight;
  words: readonly CaptionWordState[];
  text: string;
  emphasis: readonly ResolvedEmphasis[];
  metrics: TextMetrics;
  style: TextStyle;
  offset: number;
  maxWidth: number;
  shownLines: number;
  frame: number;
  fps: number;
  theme: ReturnType<typeof useTheme>;
}): LitWord {
  const { highlight, words, text, emphasis, metrics, style, offset, maxWidth, shownLines, frame, fps, theme } = options;
  const color = hexColor(highlight.color ?? '#ffd166', theme, 'CaptionTrack', 'highlight.color');
  const spoken = highlight.spokenColor === undefined ? undefined : hexColor(highlight.spokenColor, theme, 'CaptionTrack', 'highlight.spokenColor');
  const upcoming = highlight.upcomingColor === undefined ? undefined : hexColor(highlight.upcomingColor, theme, 'CaptionTrack', 'highlight.upcomingColor');
  const scale = positive('highlight.scale', highlight.scale ?? 1);
  const transition = highlight.transitionInFrames ?? Math.max(1, Math.round(fps / 10));
  if (!Number.isInteger(transition) || transition < 0) {
    fail(`highlight.transitionInFrames must be a whole number of at least 0, not ${transition}`);
  }
  const fontSize = style.fontSize ?? metrics.ascent;
  const lineHeight = metrics.lineHeight;
  const padding = edges(highlight.padding ?? { top: fontSize * 0.08, bottom: fontSize * 0.08, left: fontSize * 0.12, right: fontSize * 0.12 });
  const cornerRadius = highlight.cornerRadius ?? Math.round(fontSize * 0.15);

  const clusters = graphemes(text);
  const current = words.findIndex((word) => word.state === 'current');
  const word = words[current];
  const lines = word ? wordLines(metrics, text, clusters, word, shownLines) : [];
  const progress = word === undefined || transition === 0 ? 1 : easeOut(Math.min(1, (frame - word.from) / transition));
  const grown = 1 + (scale - 1) * progress;
  const separate = scale !== 1 && lines.length > 0;
  const banded = highlight.background != null && lines.length > 0;
  const colors = { current: color, spoken, upcoming };
  const content = coloredContent(clusters, emphasis, words, colors);
  // With a band, `color` is the color of text on it: off the band, as the
  // band slides in, the word being spoken keeps the colors it has.
  const offBand = separate ? { ...colors, current: CLEAR } : banded ? { ...colors, current: undefined } : colors;

  const grownWord = (fill: TextStyle['fill']) => lines.map((line, index) => React.createElement(
    Group,
    { key: `word-${index}`, x: (line.x0 + line.x1) / 2 - offset, y: line.line * lineHeight + lineHeight / 2, scale: grown },
    React.createElement(Text, {
      x: -(line.x1 - line.x0) / 2, y: metrics.ascent - lineHeight / 2, anchorY: 'baseline',
      style: { ...style, align: 'left', fill }, children: line.text,
    }),
  ));
  const over: ReactNode[] = separate ? grownWord(banded ? style.fill : { type: 'solid', color }) : [];
  if (banded) {
    // The band behind one line of a word, grown by `by` about its centre.
    const band = (line: WordLine, by: number) => {
      const width = (line.x1 - line.x0 + padding.left + padding.right) * by;
      const height = (fontSize + padding.top + padding.bottom) * by;
      return {
        x: (line.x0 + line.x1 + padding.right - padding.left) / 2 - offset - width / 2,
        y: line.line * lineHeight + (lineHeight + padding.bottom - padding.top) / 2 - height / 2,
        width,
        height,
      };
    };
    const before = words[current - 1];
    const previous = before ? wordLines(metrics, text, clusters, before, shownLines).pop() : undefined;
    // On the band, the text it covers is drawn again in `color`, with the
    // same weights so it lies on the same glyphs, and so as it slides,
    // whatever it is over reads on it.
    const weights = emphasis.map((entry) => ({
      ...entry, style: entry.style.fontWeight === undefined ? {} : { fontWeight: entry.style.fontWeight },
    }));
    const onBand = coloredContent(clusters, weights, separate ? words : [], { current: CLEAR });
    lines.forEach((line, index) => {
      let rect = band(line, grown);
      let radius = cornerRadius * grown;
      // The band slides along the line from where the word before had it.
      if (index === 0 && previous && previous.line === line.line && progress < 1) {
        const from = band(previous, scale);
        const to = band(line, scale);
        rect = { ...to, x: from.x + (to.x - from.x) * progress, width: from.width + (to.width - from.width) * progress };
        radius = cornerRadius * scale;
      }
      over.push(React.createElement(
        Group,
        { key: `band-${index}`, clip: { ...rect, cornerRadius: radius } },
        React.createElement(Rect, { ...rect, fill: highlight.background!, cornerRadius: radius }),
        React.createElement(Text, {
          x: -offset, y: metrics.ascent, anchorY: 'baseline', maxWidth,
          style: { ...style, fill: { type: 'solid', color } }, children: onBand,
        }),
        ...(separate ? grownWord({ type: 'solid', color }) : []),
      ));
    });
  }
  return { content, drawnContent: coloredContent(clusters, emphasis, words, offBand), over };
}

/** The parts of `word` on each shown line, from the measured glyphs; none without per-glyph metrics. */
function wordLines(
  metrics: TextMetrics,
  text: string,
  clusters: readonly string[],
  word: CaptionWordState,
  shownLines: number,
): WordLine[] {
  const start = Array.from(clusters.slice(0, word.start).join('')).length;
  const end = start + Array.from(clusters.slice(word.start, word.end).join('')).length;
  const byLine = new Map<number, GlyphMetrics[]>();
  for (const glyph of metrics.glyphs) {
    if (glyph.start >= end || glyph.end <= start || glyph.line >= shownLines) continue;
    byLine.set(glyph.line, [...(byLine.get(glyph.line) ?? []), glyph]);
  }
  const points = Array.from(text);
  return [...byLine.entries()].sort(([a], [b]) => a - b).map(([line, glyphs]) => ({
    line,
    x0: Math.min(...glyphs.map((glyph) => glyph.x)),
    x1: Math.max(...glyphs.map((glyph) => glyph.x + glyph.width)),
    text: points.slice(Math.min(...glyphs.map((glyph) => glyph.start)), Math.max(...glyphs.map((glyph) => glyph.end))).join(''),
  }));
}

/**
 * `clusters` as `<Span>`s colored by word: the word being spoken in
 * `current` over any emphasis, the others in their emphasis's color, or
 * else `spoken` or `upcoming`; a color left out leaves the text's own.
 */
function coloredContent(
  clusters: readonly string[],
  emphasis: readonly ResolvedEmphasis[],
  words: readonly CaptionWordState[],
  colors: { current?: string; spoken?: string; upcoming?: string },
): ReactNode {
  const cuts = new Set([0, clusters.length]);
  for (const range of [...emphasis, ...words]) {
    cuts.add(range.start);
    cuts.add(range.end);
  }
  const sorted = [...cuts].sort((a, b) => a - b);
  const parts: ReactNode[] = [];
  for (let i = 0; i + 1 < sorted.length; i += 1) {
    const [start, end] = [sorted[i], sorted[i + 1]];
    const part = clusters.slice(start, end).join('');
    const emphasized = emphasis.find((entry) => entry.start <= start && start < entry.end)?.style;
    const word = words.find((entry) => entry.start <= start && start < entry.end);
    const fill = word?.state === 'current' && colors.current !== undefined
      ? colors.current
      : emphasized?.color ?? (word?.state === 'spoken' ? colors.spoken : word?.state === 'upcoming' ? colors.upcoming : undefined);
    const fontWeight = emphasized?.fontWeight;
    parts.push(fill === undefined && fontWeight === undefined
      ? part
      : React.createElement(Span, {
        key: i,
        style: { ...(fill !== undefined ? { fill } : {}), ...(fontWeight !== undefined ? { fontWeight } : {}) },
      }, part));
  }
  return parts;
}

function easeOut(t: number): number {
  return 1 - (1 - t) ** 3;
}
