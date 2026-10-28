// A box of a given size — a background, a border, rounded corners, a shadow,
// and padding — that its children lay out inside, optionally clipped to it.

import * as React from 'react';

import { LayoutBounds } from '@celesta/layout';
import type { Insets } from '@celesta/layout';
import { Group, Rect, useTheme } from '@celesta/react';
import type { GroupProps, LayerShadow, Paint } from '@celesta/react';

import { hexColor } from './color';
import { validators } from './validate';

export interface PanelProps extends Omit<GroupProps, 'clip' | 'shadow'> {
  /** Width of the panel, padding included. */
  width: number;
  /** Height of the panel, padding included. */
  height: number;
  /**
   * Space between the panel's edges and its content: the same on every
   * edge, or per edge (missing edges are 0). Defaults to 0.
   */
  padding?: number | Insets;
  /** A hex color, a `<Theme>` color name, or a gradient `Paint` for the background. Omit for none. */
  fill?: string | Paint;
  /** A hex color, a `<Theme>` color name, or a `Paint` for the border, drawn inside the edge. Omit for none. */
  stroke?: string | Paint;
  /** Width of the border. Defaults to 2. */
  strokeWidth?: number;
  /** Rounds the background, the border, and the clip alike. Defaults to 0. */
  cornerRadius?: number;
  /** A shadow cast by the background, not by the content. Needs a `fill`. */
  shadow?: LayerShadow;
  /** Draws the content only inside the panel's rounded rectangle. Defaults to false. */
  clip?: boolean;
  /**
   * The point of the panel at `x`/`y`, and the point it rotates and scales
   * about, as fractions of its width and height: 0 (the default) the
   * top-left corner, 0.5 the centre.
   */
  anchorX?: number;
  anchorY?: number;
}

const { fail, finite, nonNegative, insets } = validators('Panel');

/**
 * A `width`×`height` box that draws, bottom to top, its `shadow`, its `fill`,
 * its content, and its `stroke` (inside the edge, over the content, so
 * content reaching the edge never hides the border). The content's origin is
 * the top-left of the area inside `padding`, and `<Center>`, `<Fit>`,
 * `<SafeArea>`, and `useLayoutBounds()` inside lay out in that area. With
 * `clip`, the content is cut to the panel's rounded rectangle, padding
 * included, as CSS `overflow: hidden` does; the shadow is never clipped.
 *
 * Sizes, padding, `cornerRadius`, and `strokeWidth` must be finite and not
 * negative. Padding wider than the panel leaves a content area of 0, still
 * starting at the left and top padding.
 *
 * ```tsx
 * <Panel x={120} y={600} width={720} height={320} padding={32}
 *   fill="#182030" cornerRadius={20} clip>
 *   <Text style={textStyle}>説明文</Text>
 * </Panel>
 * ```
 */
export function Panel(props: PanelProps): ReturnType<typeof React.createElement> {
  const theme = useTheme();
  const {
    width, height, padding = 0, fill, stroke, strokeWidth = 2, cornerRadius = 0, shadow, clip = false,
    anchorX = 0, anchorY = 0, children, ...group
  } = props;
  nonNegative('width', width);
  nonNegative('height', height);
  nonNegative('cornerRadius', cornerRadius);
  nonNegative('strokeWidth', strokeWidth);
  const { top, right, bottom, left } = insets(padding);
  if (shadow !== undefined && fill === undefined) {
    fail('shadow is cast by the background, so it needs a fill');
  }

  const paint = (value: string | Paint | undefined, prop: string) =>
    typeof value === 'string' ? hexColor(value, theme, 'Panel', prop) : value;
  const background = paint(fill, 'fill');
  const border = paint(stroke, 'stroke');
  const cast = shadow === undefined ? undefined : { ...shadow, color: hexColor(shadow.color, theme, 'Panel', 'shadow color') };

  // The panel's top-left corner, from the group's origin at the anchor point.
  const originX = 0 - finite('anchorX', anchorX) * width;
  const originY = 0 - finite('anchorY', anchorY) * height;
  const contentWidth = Math.max(0, width - left - right);
  const contentHeight = Math.max(0, height - top - bottom);

  return React.createElement(Group, group,
    background === undefined ? null : React.createElement(Rect, {
      key: 'background', x: originX, y: originY, width, height, fill: background, cornerRadius, shadow: cast,
    }),
    React.createElement(Group, {
      key: 'content',
      x: originX + left,
      y: originY + top,
      clip: clip ? { x: 0 - left, y: 0 - top, width, height, cornerRadius } : undefined,
    }, React.createElement(LayoutBounds, { width: contentWidth, height: contentHeight }, children)),
    border === undefined || strokeWidth === 0 ? null : React.createElement(Rect, {
      key: 'border', x: originX, y: originY, width, height, stroke: border, strokeWidth, cornerRadius,
    }),
  );
}
