// A title screen: an optional chapter number and kicker, a title fitted to
// the room left, and a subtitle, stacked in one block placed by a few
// alignment patterns inside the current layout area. Each piece enters and
// leaves through <Motion>, one after another or all at once.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Rect, Text, useTextMetrics, useTheme, useVideoConfig } from '@celesta/react';
import type { FontFamily, TextStyle } from '@celesta/react';
import { safeAreaInsets, useLayoutBounds } from '@celesta/layout';
import type { Insets, SafeAreaValue } from '@celesta/layout';
import { useFitText } from '@celesta/text';
import type { FitTextStyle } from '@celesta/text';

import { hexColor } from './color';
import type { MotionEffect } from './motion';
import { defaultStagger, solid, staggeredPiece } from './text-block';
import { validators } from './validate';

export interface TitleCardProps {
  /** The headline. `\n` breaks a line; long titles wrap and shrink to fit. */
  title: string;
  /** A line or paragraph under the title. */
  subtitle?: string;
  /** A small label above the title, such as `"03 — SYNTAX"`. */
  kicker?: string;
  /** A large chapter number above the kicker, such as `"01"`. */
  chapter?: string | number;
  /** Horizontal placement of the block and its lines. Defaults to `'left'`. */
  align?: 'left' | 'center' | 'right';
  /** Vertical placement of the block. Defaults to `'middle'`. */
  verticalAlign?: 'top' | 'middle' | 'bottom';
  /**
   * Drawn behind the text over the whole layout area: a hex color or
   * `<Theme>` color name, or any element such as a `<PatternBackground>`.
   * Omitted, the card is transparent.
   */
  background?: string | ReactNode;
  /**
   * Keeps the text clear of platform UI, as `<SafeArea padding>` does; the
   * background still fills the whole area. For example `"vertical-social"`.
   */
  safeArea?: SafeAreaValue;
  /** Space between the area (inside `safeArea`) and the text. Defaults to 9% of the area's shorter side. */
  padding?: number | Insets;
  /** Widest the text may be. Defaults to the room inside the padding. */
  maxWidth?: number;
  /** Space between the pieces. Defaults to 2.4% of the area's shorter side. */
  gap?: number;

  /** Family name, font stack, or `<Theme>` font name of every piece. */
  fontFamily?: FontFamily;
  /** Hex color or `<Theme>` color name of the title. Defaults to `#ffffff`. */
  color?: string;
  /** Color of the subtitle. Defaults to `color`. */
  subtitleColor?: string;
  /** Color of the chapter number, kicker, and kicker rule. Defaults to `color`. */
  accentColor?: string;
  /** The title's largest font size. Defaults to 9% of the area's shorter side. */
  titleSize?: number;
  /** The smallest size a long title shrinks to. Defaults to 40% of `titleSize`. */
  minTitleSize?: number;
  /** Most lines the title may take while it shrinks. Defaults to no limit. */
  maxTitleLines?: number;
  /** The title's line height as a multiple of its size. Defaults to 1.1. */
  titleLineHeight?: number;
  /** Defaults to 3% of the area's shorter side. */
  subtitleSize?: number;
  /** Defaults to 2% of the area's shorter side. */
  kickerSize?: number;
  /** Defaults to 16% of the area's shorter side. */
  chapterSize?: number;
  /** Draws a short line in `accentColor` before the kicker; a number sets its length. */
  kickerRule?: boolean | number;
  /** Extra title style, over the defaults (weight 700). The card chooses its size and line height. */
  titleStyle?: FitTextStyle;
  /** Extra subtitle style, over the defaults (weight 400, line height 1.4). */
  subtitleStyle?: TextStyle;
  /** Extra kicker style, over the defaults (weight 700, letter spacing 15% of its size). */
  kickerStyle?: TextStyle;
  /** Extra chapter number style, over the defaults (weight 700). */
  chapterStyle?: TextStyle;

  /**
   * How each piece arrives, as `<Motion enter>` takes it; `null` cuts.
   * Defaults to `fade-up` over 3% of the area's shorter side.
   */
  enter?: MotionEffect | null;
  /** How the pieces leave at the end of the enclosing window, as `<Motion exit>`; `null` cuts. Defaults to `'fade'`. */
  exit?: MotionEffect | null;
  /**
   * Frames between one piece's entrance and the next, top to bottom; 0 brings
   * them in together. Defaults to a fifth of a second, or 0 when `enter` is `null`.
   */
  stagger?: number;
}

interface Piece {
  key: string;
  height: number;
  node: (offset: number) => ReactNode;
}

const { nonNegative, positive } = validators('TitleCard');

/**
 * A title screen inside the current layout area (the canvas, a
 * `<SafeArea>`, or a `<Fit>`): a chapter number, kicker, title, and
 * subtitle stacked in one block, placed by `align` and `verticalAlign`.
 * The title keeps its `\n` breaks, wraps at the column, and shrinks until
 * the block fits. Each piece enters through `<Motion>`, `stagger` frames
 * after the one above, and all leave together at the end of the enclosing
 * sequence.
 *
 * ```tsx
 * <TitleCard chapter="01" kicker="GETTING STARTED" title={'Code is\nthe cut'}
 *   subtitle="Celesta 入門" background="ink" color="paper" accentColor="accent" />
 * ```
 */
export function TitleCard(props: TitleCardProps): ReturnType<typeof React.createElement> {
  const bounds = useLayoutBounds();
  const canvas = useVideoConfig();
  const theme = useTheme();
  const side = Math.min(bounds.width, bounds.height);
  const {
    title, subtitle, kicker, chapter, align = 'left', verticalAlign = 'middle', background, safeArea,
    padding = Math.round(side * 0.09), maxWidth, gap = Math.round(side * 0.024),
    fontFamily, color = '#ffffff', subtitleColor = color, accentColor = color,
    titleSize = Math.round(side * 0.09), maxTitleLines, titleLineHeight = 1.1,
    subtitleSize = Math.round(side * 0.03), kickerSize = Math.round(side * 0.02), chapterSize = Math.round(side * 0.16),
    kickerRule = false, titleStyle, subtitleStyle, kickerStyle, chapterStyle,
    enter = { type: 'fade-up', distance: Math.round(side * 0.03) }, exit = 'fade',
  } = props;
  const minTitleSize = Math.min(props.minTitleSize ?? Math.round(titleSize * 0.4), titleSize);
  const stagger = props.stagger ?? defaultStagger(enter, canvas.fps);
  for (const [name, value] of [['titleSize', titleSize], ['minTitleSize', minTitleSize], ['subtitleSize', subtitleSize],
    ['kickerSize', kickerSize], ['chapterSize', chapterSize]] as const) {
    positive(name, value);
  }
  nonNegative('gap', gap);
  nonNegative('stagger', stagger);

  const ink = hexColor(color, theme, 'TitleCard', 'color');
  const soft = hexColor(subtitleColor, theme, 'TitleCard', 'subtitleColor');
  const accent = hexColor(accentColor, theme, 'TitleCard', 'accentColor');

  // The text's box: the area, inside the safe area, inside the padding.
  const outer = safeArea === undefined
    ? { top: 0, right: 0, bottom: 0, left: 0 }
    : safeAreaInsets(safeArea, canvas.width, canvas.height);
  const inner = safeAreaInsets(padding, canvas.width, canvas.height);
  const left = outer.left + inner.left;
  const top = outer.top + inner.top;
  const room = {
    width: Math.max(1, bounds.width - left - outer.right - inner.right),
    height: Math.max(1, bounds.height - top - outer.bottom - inner.bottom),
  };
  const column = Math.max(1, Math.min(room.width, maxWidth ?? room.width));
  const columnLeft = left + (align === 'left' ? 0 : align === 'center' ? (room.width - column) / 2 : room.width - column);
  // Where each piece's Motion sits, so `scale` grows about the aligned edge or centre.
  const anchor = align === 'left' ? 0 : align === 'center' ? column / 2 : column;

  const family = fontFamily === undefined ? {} : { fontFamily };
  const chapterText = chapter === undefined ? '' : String(chapter);
  const chapterLook: TextStyle = { ...family, fontSize: chapterSize, fontWeight: 700, fill: solid(accent), ...chapterStyle };
  const kickerLook: TextStyle = {
    ...family, fontSize: kickerSize, fontWeight: 700, letterSpacing: Math.round(kickerSize * 0.15), fill: solid(accent), ...kickerStyle,
  };
  const subtitleLook: TextStyle = {
    ...family, fontSize: subtitleSize, fontWeight: 400, lineHeight: Math.round(subtitleSize * 1.4), align, fill: solid(soft), ...subtitleStyle,
  };
  const rule = kickerRule === false ? 0 : kickerRule === true ? Math.round(kickerSize * 2) : kickerRule;
  const ruleGap = rule > 0 ? Math.round(kickerSize * 0.8) : 0;

  // Every hook runs on every frame, an absent piece measured as ''.
  const chapterMetrics = useTextMetrics(chapterText, chapterLook, { maxWidth: column });
  const kickerMetrics = useTextMetrics(kicker ?? '', kickerLook);
  const subtitleMetrics = useTextMetrics(subtitle ?? '', subtitleLook, { maxWidth: column });
  const present = [chapterText !== '', kicker !== undefined && kicker !== '', subtitle !== undefined && subtitle !== ''];
  const others = (present[0] ? chapterMetrics.height : 0) + (present[1] ? kickerMetrics.height : 0)
    + (present[2] ? subtitleMetrics.height : 0);
  const gaps = gap * present.filter(Boolean).length;
  const fit = useFitText(title, {
    width: column,
    height: Math.max(1, room.height - others - gaps),
    minFontSize: minTitleSize,
    maxFontSize: titleSize,
    maxLines: maxTitleLines,
    lineHeight: titleLineHeight,
    style: { ...family, fontWeight: 700, align, fill: solid(ink), ...titleStyle },
  });

  // Multi-line text wraps at the column with `align`; `offset` moves it from
  // the piece's anchor to the column's left edge.
  const block = (key: string, text: string, style: TextStyle, metrics: { ascent: number }) => (offset: number) =>
    React.createElement(Text, { key, x: offset, y: metrics.ascent, anchorY: 'baseline', maxWidth: column, style, children: text });

  const pieces: Piece[] = [];
  if (present[0]) pieces.push({ key: 'chapter', height: chapterMetrics.height, node: block('chapter', chapterText, chapterLook, chapterMetrics) });
  if (present[1]) {
    const line = rule + ruleGap + kickerMetrics.width;
    pieces.push({
      key: 'kicker',
      height: kickerMetrics.height,
      // One line, placed by its measured width so it needs no `align`.
      node: (offset) => {
        const start = offset + (align === 'left' ? 0 : align === 'center' ? (column - line) / 2 : column - line);
        const thickness = Math.max(1, Math.round(kickerSize * 0.2));
        return [
          rule > 0 ? React.createElement(Rect, {
            key: 'rule', x: start, y: (kickerMetrics.height - thickness) / 2, width: rule, height: thickness, fill: accent,
          }) : null,
          React.createElement(Text, {
            key: 'kicker', x: start + rule + ruleGap, y: kickerMetrics.ascent, anchorY: 'baseline', style: kickerLook, children: kicker,
          }),
        ];
      },
    });
  }
  pieces.push({ key: 'title', height: fit.metrics.height, node: block('title', title, fit.style, fit.metrics) });
  if (present[2]) pieces.push({ key: 'subtitle', height: subtitleMetrics.height, node: block('subtitle', subtitle!, subtitleLook, subtitleMetrics) });

  const height = pieces.reduce((sum, piece) => sum + piece.height, 0) + gap * (pieces.length - 1);
  let y = top + (verticalAlign === 'top' ? 0 : verticalAlign === 'middle' ? (room.height - height) / 2 : room.height - height);
  const placed = pieces.map((piece, index) => {
    const at = { key: piece.key, x: columnLeft + anchor, y, originY: piece.height / 2 };
    y += piece.height + gap;
    return staggeredPiece(index, at, { enter, exit, stagger }, piece.node(-anchor));
  });

  const backdrop = background === undefined || background === null
    ? null
    : typeof background === 'string'
      ? React.createElement(Rect, {
        key: 'background', width: bounds.width, height: bounds.height, fill: hexColor(background, theme, 'TitleCard', 'background'),
      })
      : React.createElement(React.Fragment, { key: 'background' }, background);
  return React.createElement(Group, null, backdrop, ...placed);
}
