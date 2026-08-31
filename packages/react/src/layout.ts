import * as React from 'react';

import { Group } from './components';
import type { GroupProps } from './components';
import { useVideoConfig } from './hooks';

interface LayoutBounds {
  width: number;
  height: number;
}

const LayoutBoundsContext = React.createContext<LayoutBounds | null>(null);

export function useLayoutBounds(): LayoutBounds {
  const inherited = React.useContext(LayoutBoundsContext);
  const { width, height } = useVideoConfig();
  return inherited ?? { width, height };
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

export interface SafeAreaProps extends GroupProps {
  padding: number | Insets;
}

function finiteNonNegative(value: number, name: string): number {
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`${name} must be a finite non-negative number`);
  }
  return value;
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
  const values = typeof padding === 'number' ? { top: padding, right: padding, bottom: padding, left: padding } : padding;
  const top = finiteNonNegative(values.top ?? 0, 'SafeArea top padding');
  const right = finiteNonNegative(values.right ?? 0, 'SafeArea right padding');
  const bottom = finiteNonNegative(values.bottom ?? 0, 'SafeArea bottom padding');
  const left = finiteNonNegative(values.left ?? 0, 'SafeArea left padding');
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
