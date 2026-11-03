import * as React from 'react';

import { Group, useVideoConfig } from '@celesta.studio/react';
import type { GroupProps } from '@celesta.studio/react';

/** The area nested layout components lay out in, in pixels. */
export interface LayoutSize {
  width: number;
  height: number;
}

const LayoutBoundsContext = React.createContext<LayoutSize | null>(null);

/** The enclosing `<SafeArea>`'s, `<Fit>`'s, or `<LayoutBounds>`'s area, or else the canvas. */
export function useLayoutBounds(): LayoutSize {
  const inherited = React.useContext(LayoutBoundsContext);
  const { width, height } = useVideoConfig();
  return inherited ?? { width, height };
}

export interface LayoutBoundsProps extends LayoutSize {
  children?: React.ReactNode;
}

/**
 * Makes `width`×`height` the area nested `<Center>`, `<Fit>`, `<SafeArea>`,
 * and `useLayoutBounds()` lay out in, from the current origin. It draws
 * nothing and does not move the children: a component with a size of its
 * own, such as a panel, wraps its content in it.
 */
export function LayoutBounds({ width, height, children }: LayoutBoundsProps): ReturnType<typeof React.createElement> {
  const value = React.useMemo(() => ({
    width: finiteNonNegative(width, 'LayoutBounds width'),
    height: finiteNonNegative(height, 'LayoutBounds height'),
  }), [width, height]);
  return React.createElement(LayoutBoundsContext.Provider, { value }, children);
}

export interface CenterProps extends GroupProps {}

/** Places its local origin at the center of the current canvas or safe area. */
export function Center({ children, x = 0, y = 0, ...props }: CenterProps): ReturnType<typeof Group> {
  const { width, height } = useLayoutBounds();
  return React.createElement(Group, { ...props, x: width / 2 + x, y: height / 2 + y }, children);
}

export interface Insets {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}

/**
 * A named safe area. `'vertical-social'` keeps clear of the UI that YouTube
 * Shorts, TikTok and Instagram Reels draw over a vertical video: the top 10%
 * (search and tabs), the bottom 20% (caption, channel name, progress bar),
 * the right 18.5% (like, comment and share buttons) and the left 1/15. On a
 * 1080×1920 canvas that is top 192, right 200, bottom 384 and left 72 px.
 * These are rough guides, not the platforms' own figures, and they do not
 * follow changes to the platforms' UI.
 */
export type SafeAreaPreset = 'vertical-social';

/** A safe area as a uniform inset, per-edge insets (missing edges are 0), or a preset. */
export type SafeAreaValue = number | Insets | SafeAreaPreset;

// Fractions of the canvas height (top, bottom) and width (left, right).
const SAFE_AREA_PRESETS: Record<SafeAreaPreset, Required<Insets>> = {
  'vertical-social': { top: 0.1, right: 0.185, bottom: 0.2, left: 1 / 15 },
};

function finiteNonNegative(value: number, name: string): number {
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`${name} must be a finite non-negative number`);
  }
  return value;
}

/**
 * Resolves a safe area to pixel insets for a `width`×`height` canvas. A
 * preset scales with the canvas and rounds to whole pixels.
 */
export function safeAreaInsets(area: SafeAreaValue, width: number, height: number): Required<Insets> {
  if (typeof area === 'string') {
    const fractions = Object.prototype.hasOwnProperty.call(SAFE_AREA_PRESETS, area)
      ? SAFE_AREA_PRESETS[area]
      : undefined;
    if (!fractions) {
      const names = Object.keys(SAFE_AREA_PRESETS).map((name) => `'${name}'`).join(', ');
      throw new Error(`unknown safe area preset '${area}' (expected ${names})`);
    }
    return {
      top: Math.round(fractions.top * height),
      right: Math.round(fractions.right * width),
      bottom: Math.round(fractions.bottom * height),
      left: Math.round(fractions.left * width),
    };
  }
  const values = typeof area === 'number' ? { top: area, right: area, bottom: area, left: area } : area;
  return {
    top: finiteNonNegative(values.top ?? 0, 'safe area top inset'),
    right: finiteNonNegative(values.right ?? 0, 'safe area right inset'),
    bottom: finiteNonNegative(values.bottom ?? 0, 'safe area bottom inset'),
    left: finiteNonNegative(values.left ?? 0, 'safe area left inset'),
  };
}

export interface SafeAreaProps extends GroupProps {
  /**
   * The inset on every edge, per-edge insets, or a preset. A preset is
   * sized from the composition canvas, so use it on a `<SafeArea>` that
   * covers the whole canvas.
   */
  padding: SafeAreaValue;
}

/** Insets children and makes the reduced dimensions available to nested layout components. */
export function SafeArea({
  padding,
  children,
  x = 0,
  y = 0,
  ...props
}: SafeAreaProps): ReturnType<typeof React.createElement> {
  const bounds = useLayoutBounds();
  const canvas = useVideoConfig();
  const { top, right, bottom, left } = safeAreaInsets(padding, canvas.width, canvas.height);
  const inner = { width: Math.max(0, bounds.width - left - right), height: Math.max(0, bounds.height - top - bottom) };
  return React.createElement(
    LayoutBoundsContext.Provider,
    { value: inner },
    React.createElement(Group, { ...props, x: x + left, y: y + top }, children),
  );
}

export interface StackProps extends GroupProps {
  direction?: 'horizontal' | 'vertical';
  spacing: number;
}

/** Arranges child origins at a fixed spacing without requiring child measurement. */
export function Stack({
  direction = 'vertical',
  spacing,
  children,
  ...props
}: StackProps): ReturnType<typeof Group> {
  if (!Number.isFinite(spacing)) {
    throw new Error('<Stack> requires a finite `spacing` prop');
  }
  const items = React.Children.map(children, (child, index) =>
    React.createElement(Group, {
      x: direction === 'horizontal' ? index * spacing : 0,
      y: direction === 'vertical' ? index * spacing : 0,
      children: child,
    }),
  );
  return React.createElement(Group, props, items);
}

export interface GridProps extends GroupProps {
  columns: number;
  columnWidth: number;
  rowHeight: number;
  columnGap?: number;
  rowGap?: number;
}

/** Places child origins into fixed-size cells, row by row. */
export function Grid({
  columns,
  columnWidth,
  rowHeight,
  columnGap = 0,
  rowGap = 0,
  children,
  ...props
}: GridProps): ReturnType<typeof Group> {
  if (!Number.isInteger(columns) || columns <= 0) {
    throw new Error('<Grid> requires a positive integer `columns` prop');
  }
  finiteNonNegative(columnWidth, 'Grid columnWidth');
  finiteNonNegative(rowHeight, 'Grid rowHeight');
  finiteNonNegative(columnGap, 'Grid columnGap');
  finiteNonNegative(rowGap, 'Grid rowGap');
  const items = React.Children.map(children, (child, index) =>
    React.createElement(Group, {
      x: (index % columns) * (columnWidth + columnGap),
      y: Math.floor(index / columns) * (rowHeight + rowGap),
      children: child,
    }),
  );
  return React.createElement(Group, props, items);
}

export interface FitProps extends GroupProps {
  sourceWidth: number;
  sourceHeight: number;
  mode?: 'contain' | 'cover';
}

/** Scales source-sized children to contain or cover the current layout bounds. */
export function Fit({
  sourceWidth,
  sourceHeight,
  mode = 'contain',
  children,
  x = 0,
  y = 0,
  ...props
}: FitProps): ReturnType<typeof React.createElement> {
  if (!Number.isFinite(sourceWidth) || sourceWidth <= 0 || !Number.isFinite(sourceHeight) || sourceHeight <= 0) {
    throw new Error('<Fit> requires positive finite `sourceWidth` and `sourceHeight` props');
  }
  const bounds = useLayoutBounds();
  const scaleForWidth = bounds.width / sourceWidth;
  const scaleForHeight = bounds.height / sourceHeight;
  const fitScale = mode === 'cover' ? Math.max(scaleForWidth, scaleForHeight) : Math.min(scaleForWidth, scaleForHeight);
  const group = React.createElement(
    Group,
    {
      ...props,
      x: x + (bounds.width - sourceWidth * fitScale) / 2,
      y: y + (bounds.height - sourceHeight * fitScale) / 2,
      scale: (props.scale ?? 1) * fitScale,
      scaleX: props.scaleX === undefined ? undefined : props.scaleX * fitScale,
      scaleY: props.scaleY === undefined ? undefined : props.scaleY * fitScale,
    },
    children,
  );
  return React.createElement(
    LayoutBoundsContext.Provider,
    { value: { width: sourceWidth, height: sourceHeight } },
    group,
  );
}
