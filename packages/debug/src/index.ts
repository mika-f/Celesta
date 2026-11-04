import * as React from 'react';
import type { ReactNode } from 'react';

import { safeAreaInsets } from '@celesta.studio/layout';
import type { SafeAreaValue } from '@celesta.studio/layout';
import { Group, Rect, Text, useCurrentFrame, useDebugGuides, useVideoConfig } from '@celesta.studio/react';
import type { GroupProps } from '@celesta.studio/react';

const DEFAULT_COLOR = '#00e5ffff';
const DEFAULT_SHADE = '#00e5ff26';

export interface DebugOverlayProps {
  color?: string;
  /**
   * The safe area to outline: an inset on every edge, per-edge insets, or a
   * preset such as `'vertical-social'` (see `<SafeArea>`). Defaults to 5% of
   * the shorter side on every edge.
   */
  safeArea?: SafeAreaValue;
  /** Fill for the canvas outside the safe area; `false` draws none. */
  shade?: string | false;
  showCenter?: boolean;
  showFrame?: boolean;
}

/**
 * Draws canvas guides in the editor preview and in PNG exports with
 * `--debug-guides`; emits nothing in a video export.
 */
export function DebugOverlay({
  color = DEFAULT_COLOR,
  safeArea,
  shade = DEFAULT_SHADE,
  showCenter = true,
  showFrame = true,
}: DebugOverlayProps): ReturnType<typeof React.createElement> | null {
  const guides = useDebugGuides();
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  if (!guides) {
    return null;
  }
  let insets;
  try {
    insets = safeAreaInsets(safeArea ?? Math.round(Math.min(width, height) * 0.05), width, height);
  } catch (error) {
    throw new Error(`<DebugOverlay> \`safeArea\`: ${error instanceof Error ? error.message : String(error)}`);
  }
  // Insets wider than the canvas leave an empty safe area at its far edge.
  const left = Math.min(insets.left, width);
  const right = Math.min(insets.right, width - left);
  const top = Math.min(insets.top, height);
  const bottom = Math.min(insets.bottom, height - top);
  const innerWidth = width - left - right;
  const innerHeight = height - top - bottom;
  // Lines one pixel wide at 540 px and thicker on larger canvases, so they
  // survive the downscaling of a contact sheet.
  const unit = Math.max(1, Math.round(Math.min(width, height) / 540));
  const fill = shade === false ? undefined : shade;
  const shades = fill === undefined
    ? []
    : [
        { x: 0, y: 0, width, height: top },
        { x: 0, y: height - bottom, width, height: bottom },
        { x: 0, y: top, width: left, height: innerHeight },
        { x: width - right, y: top, width: right, height: innerHeight },
      ].filter((area) => area.width > 0 && area.height > 0);
  return React.createElement(
    Group,
    null,
    shades.map((area, index) => React.createElement(Rect, { key: `shade${index}`, ...area, fill })),
    React.createElement(Rect, {
      x: left,
      y: top,
      width: innerWidth,
      height: innerHeight,
      stroke: color,
      strokeWidth: unit,
    }),
    showCenter
      ? React.createElement(
          React.Fragment,
          null,
          React.createElement(Rect, { x: width / 2 - 12 * unit, y: height / 2, width: 24 * unit, height: unit, fill: color }),
          React.createElement(Rect, { x: width / 2, y: height / 2 - 12 * unit, width: unit, height: 24 * unit, fill: color }),
        )
      : null,
    showFrame
      ? React.createElement(
          Text,
          {
            x: 8 * unit,
            y: 8 * unit,
            style: { fontFamily: 'sans-serif', fontSize: 14 * unit, fill: { type: 'solid', color } },
            children: `frame ${frame} · ${width}×${height}`,
          },
        )
      : null,
  );
}

export interface DebugBoundsProps extends GroupProps {
  width: number;
  height: number;
  color?: string;
  label?: string;
  showOrigin?: boolean;
  children?: ReactNode;
}

/**
 * Adds bounds and a local-origin cross where debug guides are drawn (the
 * preview, PNG exports with `--debug-guides`) while preserving the same
 * export transform.
 */
export function DebugBounds({
  width,
  height,
  color = DEFAULT_COLOR,
  label,
  showOrigin = true,
  children,
  ...props
}: DebugBoundsProps): ReturnType<typeof Group> {
  const guides = useDebugGuides();
  if (!Number.isFinite(width) || width <= 0 || !Number.isFinite(height) || height <= 0) {
    throw new Error('<DebugBounds> requires positive finite `width` and `height` props');
  }
  return React.createElement(
    Group,
    props,
    children,
    guides
      ? React.createElement(
          React.Fragment,
          null,
          React.createElement(Rect, { width, height, stroke: color, strokeWidth: 1 }),
          showOrigin
            ? React.createElement(
                React.Fragment,
                null,
                React.createElement(Rect, { x: -5, width: 11, height: 1, fill: color }),
                React.createElement(Rect, { y: -5, width: 1, height: 11, fill: color }),
              )
            : null,
          label
            ? React.createElement(
                Text,
                {
                  x: 4,
                  y: 4,
                  style: { fontFamily: 'sans-serif', fontSize: 12, fill: { type: 'solid', color } },
                  children: label,
                },
              )
            : null,
        )
      : null,
  );
}
