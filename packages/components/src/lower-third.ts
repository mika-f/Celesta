// A lower third: a name and an optional role on a band sized to them, with
// an accent bar on the band's outer edge, placed in a corner of the current
// layout area. The band slides in and out through <Motion>; the lines come
// in after it, one after another.

import * as React from 'react';

import { Group, Rect, Text, useTheme, useVideoConfig } from '@celesta/react';
import type { FontFamily, Paint, TextMetrics, TextStyle } from '@celesta/react';
import { safeAreaInsets, useLayoutBounds } from '@celesta/layout';
import type { Insets, SafeAreaValue } from '@celesta/layout';
import { useFitText } from '@celesta/text';
import type { FitTextStyle } from '@celesta/text';

import { hexColor } from './color';
import { Motion } from './motion';
import type { MotionEffect } from './motion';
import { Panel } from './panel';
import { defaultStagger, solid, staggeredPiece } from './text-block';
import { validators } from './validate';

/** Where `<LowerThird>` sits in its area. */
export type LowerThirdPlacement = 'bottom-left' | 'bottom-center' | 'bottom-right' | 'top-left' | 'top-right';

export interface LowerThirdProps {
  /** The main line, such as a name. `\n` breaks a line; long titles wrap and shrink to fit. */
  title: string;
  /** A line under the title, such as a role. */
  subtitle?: string;
  /** Defaults to `'bottom-left'`. Right placements align the lines right; `'bottom-center'` centres them. */
  placement?: LowerThirdPlacement;
  /** Keeps the band clear of platform UI, as `<SafeArea padding>` does. For example `"vertical-social"`. */
  safeArea?: SafeAreaValue;
  /** Space between the area (inside `safeArea`) and the band. Defaults to 6% of the area's shorter side. */
  margin?: number | Insets;
  /**
   * The widest the band may be, padding and accent included. Defaults to 60%
   * of the room inside the margin on a landscape area, all of it otherwise.
   */
  maxWidth?: number;
  /**
   * Space between the band's edges (after the accent) and the text.
   * Defaults to 1.6% of the area's shorter side above and below, 2.6% at the sides.
   */
  padding?: number | Insets;
  /** Space between the title and the subtitle. Defaults to 0.6% of the area's shorter side. */
  gap?: number;
  /** The band: a hex color, `<Theme>` color name, or `Paint`. Defaults to `#000000B3`. */
  background?: string | Paint;
  /** Defaults to 0. */
  cornerRadius?: number;
  /** Hex color or `<Theme>` color name of the accent bar. Defaults to `color`. */
  accent?: string;
  /** Width of the accent bar on the band's outer edge; 0 draws none. Defaults to 0.6% of the area's shorter side. */
  accentWidth?: number;

  /** Family name, font stack, or `<Theme>` font name of both lines. */
  fontFamily?: FontFamily;
  /** Hex color or `<Theme>` color name of the title. Defaults to `#ffffff`. */
  color?: string;
  /** Color of the subtitle. Defaults to `color`. */
  subtitleColor?: string;
  /** The title's largest font size. Defaults to 4.5% of the area's shorter side. */
  titleSize?: number;
  /** The smallest size a long title shrinks to. Defaults to 60% of `titleSize`. */
  minTitleSize?: number;
  /** Most lines the title may take. Defaults to 2. */
  maxTitleLines?: number;
  /** The title's line height as a multiple of its size. Defaults to 1.2. */
  titleLineHeight?: number;
  /** The subtitle's largest font size. Defaults to 2.6% of the area's shorter side. */
  subtitleSize?: number;
  /** The smallest size a long subtitle shrinks to. Defaults to 80% of `subtitleSize`. */
  minSubtitleSize?: number;
  /** Most lines the subtitle may take. Defaults to 2. */
  maxSubtitleLines?: number;
  /** The subtitle's line height as a multiple of its size. Defaults to 1.35. */
  subtitleLineHeight?: number;
  /** Extra title style, over the defaults (weight 700). The band chooses its size and line height. */
  titleStyle?: FitTextStyle;
  /** Extra subtitle style, over the defaults (weight 400). */
  subtitleStyle?: FitTextStyle;

  /**
   * How the band arrives, as `<Motion enter>` takes it; `null` cuts. Defaults
   * to fading in while moving toward the area's middle (`fade-right` on the
   * left, `fade-left` on the right, `fade-up` in the centre).
   */
  enter?: MotionEffect | null;
  /** How the band leaves at the end of the enclosing window, as `<Motion exit>`; `null` cuts. Defaults to `'fade'`. */
  exit?: MotionEffect | null;
  /** How each line arrives inside the band; `null` shows them with it. Defaults to a short `fade-up`. */
  textEnter?: MotionEffect | null;
  /**
   * Frames between the band's entrance and the title's, and the title's and
   * the subtitle's. Defaults to a fifth of a second, or 0 when `textEnter` is `null`.
   */
  stagger?: number;
}

interface Line {
  key: string;
  text: string;
  style: TextStyle;
  metrics: TextMetrics;
}

const PLACEMENTS: readonly LowerThirdPlacement[] = ['bottom-left', 'bottom-center', 'bottom-right', 'top-left', 'top-right'];

const { nonNegative, positive } = validators('LowerThird');

/**
 * A name and an optional role on a band in a corner of the current layout
 * area (the canvas, a `<SafeArea>`, or a `<Fit>`), inside `safeArea` and
 * `margin`. The band is a `<Panel>` as wide as the longer line, up to
 * `maxWidth`; each line wraps there and shrinks with `useFitText` until it
 * fits its line limit. The band enters through `<Motion>` and the lines
 * follow it, `stagger` frames apart; everything leaves with the band at the
 * end of the enclosing sequence, so wrap it in a `<Sequence>` for the
 * stretch it should show.
 *
 * ```tsx
 * <Sequence from={30} durationInFrames={120}>
 *   <LowerThird title="夏猫" subtitle="ソフトウェアエンジニア" accent="#60A0FF" />
 * </Sequence>
 * ```
 */
export function LowerThird(props: LowerThirdProps): ReturnType<typeof React.createElement> {
  const bounds = useLayoutBounds();
  const canvas = useVideoConfig();
  const theme = useTheme();
  const side = Math.min(bounds.width, bounds.height);
  const {
    title, subtitle, placement = 'bottom-left', safeArea, margin = Math.round(side * 0.06),
    gap = Math.round(side * 0.006), background = '#000000B3', cornerRadius = 0,
    fontFamily, color = '#ffffff', subtitleColor = color, accent = color, accentWidth = Math.round(side * 0.006),
    titleSize = Math.round(side * 0.045), maxTitleLines = 2, titleLineHeight = 1.2,
    subtitleSize = Math.round(side * 0.026), maxSubtitleLines = 2, subtitleLineHeight = 1.35,
    titleStyle, subtitleStyle, exit = 'fade', textEnter = { type: 'fade-up', distance: Math.round(side * 0.012) },
  } = props;
  const padding = props.padding ?? {
    top: Math.round(side * 0.016), bottom: Math.round(side * 0.016),
    left: Math.round(side * 0.026), right: Math.round(side * 0.026),
  };
  const minTitleSize = Math.min(props.minTitleSize ?? Math.round(titleSize * 0.6), titleSize);
  const minSubtitleSize = Math.min(props.minSubtitleSize ?? Math.round(subtitleSize * 0.8), subtitleSize);
  for (const [name, value] of [['titleSize', titleSize], ['minTitleSize', minTitleSize], ['subtitleSize', subtitleSize],
    ['minSubtitleSize', minSubtitleSize], ['titleLineHeight', titleLineHeight], ['subtitleLineHeight', subtitleLineHeight]] as const) {
    positive(name, value);
  }
  nonNegative('gap', gap);
  nonNegative('accentWidth', accentWidth);
  const stagger = nonNegative('stagger', props.stagger ?? defaultStagger(textEnter, canvas.fps));
  if (!PLACEMENTS.includes(placement)) {
    throw new Error(`<LowerThird> unknown placement ${JSON.stringify(placement)}; expected ${PLACEMENTS.join(', ')}`);
  }
  const [vertical, horizontal] = placement.split('-') as ['top' | 'bottom', 'left' | 'center' | 'right'];
  const enter = props.enter === undefined
    ? { type: horizontal === 'left' ? 'fade-right' : horizontal === 'right' ? 'fade-left' : 'fade-up', distance: Math.round(side * 0.03) } as const
    : props.enter;

  const ink = hexColor(color, theme, 'LowerThird', 'color');
  const soft = hexColor(subtitleColor, theme, 'LowerThird', 'subtitleColor');
  const bar = hexColor(accent, theme, 'LowerThird', 'accent');

  // The room for the band: the area, inside the safe area, inside the margin.
  const outer = safeArea === undefined
    ? { top: 0, right: 0, bottom: 0, left: 0 }
    : safeAreaInsets(safeArea, canvas.width, canvas.height);
  const edge = safeAreaInsets(margin, canvas.width, canvas.height);
  const pad = safeAreaInsets(padding, canvas.width, canvas.height);
  const room = {
    left: outer.left + edge.left,
    top: outer.top + edge.top,
    right: bounds.width - outer.right - edge.right,
    bottom: bounds.height - outer.bottom - edge.bottom,
  };
  const roomWidth = Math.max(1, room.right - room.left);
  const widest = Math.min(roomWidth, maxWidth(props.maxWidth, roomWidth, bounds));
  const budget = Math.max(1, widest - accentWidth - pad.left - pad.right);

  const align = horizontal;
  const family = fontFamily === undefined ? {} : { fontFamily };
  const hasSubtitle = subtitle !== undefined && subtitle !== '';
  // Both hooks run on every frame; an absent subtitle is fitted as ''.
  const titleFit = useFitText(title, {
    width: budget,
    height: maxTitleLines * titleSize * titleLineHeight + 1,
    minFontSize: minTitleSize,
    maxFontSize: titleSize,
    maxLines: maxTitleLines,
    lineHeight: titleLineHeight,
    style: { ...family, fontWeight: 700, align, fill: solid(ink), ...titleStyle },
  });
  const subtitleFit = useFitText(subtitle ?? '', {
    width: budget,
    height: maxSubtitleLines * subtitleSize * subtitleLineHeight + 1,
    minFontSize: minSubtitleSize,
    maxFontSize: subtitleSize,
    maxLines: maxSubtitleLines,
    lineHeight: subtitleLineHeight,
    style: { ...family, fontWeight: 400, align, fill: solid(soft), ...subtitleStyle },
  });

  const lines: Line[] = [{ key: 'title', text: title, style: titleFit.style, metrics: titleFit.metrics }];
  if (hasSubtitle) lines.push({ key: 'subtitle', text: subtitle!, style: subtitleFit.style, metrics: subtitleFit.metrics });
  // The lines wrapped at `budget` are no wider than this, so they wrap the
  // same way at it.
  const column = Math.min(budget, Math.ceil(Math.max(...lines.map((line) => line.metrics.width))));
  const textHeight = lines.reduce((sum, line) => sum + line.metrics.height, 0) + gap * (lines.length - 1);
  const width = accentWidth + pad.left + column + pad.right;
  const height = pad.top + textHeight + pad.bottom;
  const accentOnRight = horizontal === 'right';
  const textLeft = (accentOnRight ? 0 : accentWidth) + pad.left;
  // Where each line's Motion sits, so `scale` grows about the aligned edge or centre.
  const anchor = align === 'left' ? 0 : align === 'center' ? column / 2 : column;

  const x = horizontal === 'left' ? room.left : horizontal === 'right' ? room.right - width : (room.left + room.right - width) / 2;
  const y = vertical === 'top' ? room.top : room.bottom - height;

  let lineTop = pad.top;
  const placed = lines.map((line, index) => {
    const at = { key: line.key, x: textLeft + anchor, y: lineTop, originY: line.metrics.height / 2 };
    lineTop += line.metrics.height + gap;
    return staggeredPiece(index + 1, at, { enter: textEnter, exit: null, stagger }, React.createElement(Text, {
      x: -anchor, y: line.metrics.ascent, anchorY: 'baseline', maxWidth: column, style: line.style, children: line.text,
    }));
  });

  const band = React.createElement(Panel, { width, height, fill: background, cornerRadius, clip: true },
    accentWidth > 0 ? React.createElement(Rect, {
      key: 'accent', x: accentOnRight ? width - accentWidth : 0, width: accentWidth, height, fill: bar,
    }) : null,
    ...placed);
  const at = { x, y, originX: width / 2, originY: height / 2 };
  return enter === null && exit === null
    ? React.createElement(Group, at, band)
    : React.createElement(Motion, { ...at, enter: enter ?? undefined, exit: exit ?? undefined }, band);
}

function maxWidth(value: number | undefined, room: number, bounds: { width: number; height: number }): number {
  if (value !== undefined) return positive('maxWidth', value);
  return bounds.width > bounds.height ? room * 0.6 : room;
}
