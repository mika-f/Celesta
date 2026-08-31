import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Rect, Text } from './components';
import type { GroupProps } from './components';
import { useCurrentFrame, useIsPreview, useVideoConfig } from './hooks';

const DEFAULT_COLOR = '#00e5ffff';

export interface DebugOverlayProps {
  color?: string;
  safeArea?: number;
  showCenter?: boolean;
  showFrame?: boolean;
}

/** Draws canvas guides in the editor preview and emits nothing during export. */
export function DebugOverlay({
  color = DEFAULT_COLOR,
  safeArea,
  showCenter = true,
  showFrame = true,
}: DebugOverlayProps): ReturnType<typeof React.createElement> | null {
  const preview = useIsPreview();
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  if (!preview) {
    return null;
  }
  const inset = safeArea ?? Math.round(Math.min(width, height) * 0.05);
  if (!Number.isFinite(inset) || inset < 0) {
    throw new Error('<DebugOverlay> requires a finite non-negative `safeArea` prop');
  }
  const guideInset = Math.min(inset, width / 2, height / 2);
  return React.createElement(
    Group,
    null,
    React.createElement(Rect, {
      x: guideInset,
      y: guideInset,
      width: width - guideInset * 2,
      height: height - guideInset * 2,
      stroke: color,
      strokeWidth: 1,
    }),
    showCenter
      ? React.createElement(
          React.Fragment,
          null,
          React.createElement(Rect, { x: width / 2 - 12, y: height / 2, width: 24, height: 1, fill: color }),
          React.createElement(Rect, { x: width / 2, y: height / 2 - 12, width: 1, height: 24, fill: color }),
        )
      : null,
    showFrame
      ? React.createElement(
          Text,
          {
            x: 8,
            y: 8,
            style: { fontFamily: 'sans-serif', fontSize: 14, fill: { type: 'solid', color } },
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

/** Adds bounds and a local-origin cross in preview while preserving the same export transform. */
export function DebugBounds({
  width,
  height,
  color = DEFAULT_COLOR,
  label,
  showOrigin = true,
  children,
  ...props
}: DebugBoundsProps): ReturnType<typeof Group> {
  const preview = useIsPreview();
  if (!Number.isFinite(width) || width <= 0 || !Number.isFinite(height) || height <= 0) {
    throw new Error('<DebugBounds> requires positive finite `width` and `height` props');
  }
  return React.createElement(
    Group,
    props,
    children,
    preview
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
