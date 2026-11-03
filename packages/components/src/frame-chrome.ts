// The overlay many films draw around the picture: corner brackets, a title,
// the timecode, the current chapter, and a progress bar along the bottom,
// placed inside one margin so they keep their places at any canvas size.

import * as React from 'react';

import { Group, Rect, Span, Text, frameToTimecode, sceneAt, useCurrentFrame, useTheme, useVideoConfig } from '@celesta.studio/react';
import type { BlendMode, FontFamily, SceneEntry, SceneList, SceneState } from '@celesta.studio/react';

import { fade, hexColor } from './color';

/** A corner of the frame, where `<FrameChrome>` places one of its labels. */
export type FrameCorner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';

/** The labels of `<FrameChrome>`, by what they show. */
export type FrameChromeLabel = 'title' | 'timecode' | 'chapter' | 'caption';

export interface FrameChromeProps<T extends SceneEntry = SceneEntry> {
  /** Distance from the canvas edges to the corner brackets, in pixels. Defaults to 56. */
  margin?: number;
  /** Draws an L-shaped bracket in each corner. */
  corners?: boolean;
  /** Length of each bracket arm. Defaults to 24. */
  cornerLength?: number;
  /** Thickness of the bracket arms. Defaults to 2. */
  strokeWidth?: number;
  /** Space between a bracket's arm and the label beside it. Defaults to 14. */
  gap?: number;
  /** A hex color or a `<Theme>` color name, for everything. Defaults to `#ffffff`. */
  color?: string;
  /** Blends the overlay, such as `"difference"` to read over light and dark scenes alike. */
  blendMode?: BlendMode;
  /** Blends the progress bar, such as `"normal"` to keep a `progressColor` as it is. Defaults to `blendMode`. */
  progressBlendMode?: BlendMode;
  /** Opacity of the whole overlay. Defaults to 1. */
  opacity?: number;
  /** Family name, font stack, or `<Theme>` font name of the labels. */
  fontFamily?: FontFamily;
  /** Defaults to 18. */
  fontSize?: number;
  /** Weight of every label but the title. Defaults to 400. */
  fontWeight?: number;
  /** Text of the title label, top left by default. */
  title?: string;
  /** Weight of `title`. Defaults to 700. */
  titleWeight?: number;
  /** Text after the title, at `fontWeight` and `subtitleOpacity`. */
  subtitle?: string;
  /** Defaults to 0.6. */
  subtitleOpacity?: number;
  /** Shows the frame as a timecode, top right by default. */
  timecode?: boolean;
  /** `false` formats the timecode without hours, as `frameToTimecode(…, { hours: false })`. Defaults to `true`. */
  timecodeHours?: boolean;
  /**
   * A `defineScenes` list, read on the caller's clock (place `<FrameChrome>`
   * on the same timeline as the `<Scenes>`). Splits the progress bar into
   * one segment per scene and gives `chapters` its name.
   */
  scenes?: SceneList<T>;
  /** Shows the current scene's name, bottom left by default. Needs `scenes`. */
  chapters?: boolean;
  /** Text of the chapter label. Defaults to the scene's `name`. */
  formatChapter?: (scene: SceneState<T>) => string;
  /** Any text, bottom right by default, such as the canvas size and frame rate. */
  caption?: string;
  /** Draws a bar inside the bottom margin that fills from the first frame to the last. */
  progress?: boolean;
  /** With `scenes`, `false` keeps the bar in one piece. Defaults to `true`. */
  segments?: boolean;
  /** Space between the scene segments. Defaults to 6. */
  segmentGap?: number;
  /** A hex color or a `<Theme>` color name for the filled part. Defaults to `color`. */
  progressColor?: string;
  /** Defaults to 2. */
  progressHeight?: number;
  /** Distance from the bottom edge to the bar's middle. Defaults to half of `margin`. */
  progressInset?: number;
  /** Opacity of the bar's unfilled part, in `color`. Defaults to 0.25; 0 hides it. */
  trackOpacity?: number;
  /**
   * Frames the bar fills over, ending full on frame `durationInFrames - 1`.
   * Defaults to the length of `scenes`, else of the composition.
   */
  durationInFrames?: number;
  /** Moves labels to other corners. Two labels shown in the same corner throw. */
  positions?: Partial<Record<FrameChromeLabel, FrameCorner>>;
}

const DEFAULT_POSITIONS: Record<FrameChromeLabel, FrameCorner> = {
  title: 'top-left',
  timecode: 'top-right',
  chapter: 'bottom-left',
  caption: 'bottom-right',
};

/**
 * How full the bar is at `frame`: 0 on frame 0, 1 on frame `duration - 1`,
 * in frames of the whole length so that scene segments fill as one bar.
 */
function filledFrames(frame: number, duration: number): number {
  if (duration <= 1) return frame >= 0 ? duration : 0;
  return Math.min(duration, Math.max(0, (frame * duration) / (duration - 1)));
}

/**
 * Corner brackets, a title, the timecode, the current chapter, a caption,
 * and a progress bar, laid out inside `margin` from the canvas size. Only
 * what is turned on is drawn. Labels sit beside the brackets, one per
 * corner; `positions` moves them.
 *
 * ```tsx
 * <FrameChrome margin={56} corners timecode progress chapters
 *   title="CELESTA" subtitle="/ FEATURE TOUR" scenes={SCENES}
 *   color="#f4f1ea" blendMode="difference" fontFamily="mono" />
 * ```
 */
export function FrameChrome<T extends SceneEntry>(props: FrameChromeProps<T>): ReturnType<typeof React.createElement> {
  const {
    margin = 56, corners = false, cornerLength = 24, strokeWidth = 2, gap = 14,
    color = '#ffffff', blendMode, progressBlendMode = blendMode, opacity,
    fontFamily, fontSize = 18, fontWeight = 400,
    title, titleWeight = 700, subtitle, subtitleOpacity = 0.6,
    timecode = false, timecodeHours = true,
    scenes, chapters = false, formatChapter, caption,
    progress = false, segments = true, segmentGap = 6, progressColor, progressHeight = 2,
    progressInset = margin / 2, trackOpacity = 0.25, durationInFrames,
    positions,
  } = props;
  const frame = useCurrentFrame();
  const config = useVideoConfig();
  const theme = useTheme();
  const { width: W, height: H, fps } = config;
  const ink = hexColor(color, theme, 'FrameChrome', 'color');
  const fill = progressColor === undefined ? ink : hexColor(progressColor, theme, 'FrameChrome', 'progressColor');

  const scene = scenes ? sceneAt(scenes, frame) : null;
  const texts: Partial<Record<FrameChromeLabel, React.ReactNode>> = {};
  if (title !== undefined || subtitle !== undefined) {
    texts.title = [
      title === undefined ? null : React.createElement(Span, { key: 't', fontWeight: titleWeight }, title),
      title !== undefined && subtitle !== undefined ? ' ' : null,
      subtitle === undefined ? null : React.createElement(Span, { key: 's', color: fade(ink, subtitleOpacity) }, subtitle),
    ];
  }
  if (timecode) texts.timecode = frameToTimecode(frame, fps, { hours: timecodeHours });
  if (chapters && scene) texts.chapter = formatChapter ? formatChapter(scene) : scene.name;
  if (caption !== undefined) texts.caption = caption;

  const taken = new Map<FrameCorner, FrameChromeLabel>();
  const labels: React.ReactNode[] = [];
  for (const key of Object.keys(DEFAULT_POSITIONS) as FrameChromeLabel[]) {
    if (texts[key] === undefined) continue;
    const corner = positions?.[key] ?? DEFAULT_POSITIONS[key];
    const other = taken.get(corner);
    if (other) {
      throw new Error(`<FrameChrome> ${other} and ${key} are both placed ${corner}; move one with positions`);
    }
    taken.set(corner, key);
    const right = corner.endsWith('right');
    const bottom = corner.startsWith('bottom');
    const inset = margin + cornerLength + gap;
    const across = margin + cornerLength / 2;
    labels.push(React.createElement(Text, {
      key,
      x: right ? W - inset : inset,
      y: bottom ? H - across : across,
      anchorX: right ? 1 : 0,
      anchorY: 0.5,
      fontFamily, fontSize, fontWeight, color: ink,
      children: texts[key],
    }));
  }

  const brackets: React.ReactNode[] = [];
  if (corners) {
    for (const [cx, cy, sx, sy] of [[margin, margin, 1, 1], [W - margin, margin, -1, 1], [margin, H - margin, 1, -1], [W - margin, H - margin, -1, -1]]) {
      brackets.push(React.createElement(Group, { key: `${sx},${sy}`, x: cx, y: cy },
        React.createElement(Rect, { x: sx < 0 ? -cornerLength : 0, y: sy < 0 ? -strokeWidth : 0, width: cornerLength, height: strokeWidth, fill: ink }),
        React.createElement(Rect, { x: sx < 0 ? -strokeWidth : 0, y: sy < 0 ? -cornerLength : 0, width: strokeWidth, height: cornerLength, fill: ink }),
      ));
    }
  }

  const bars: React.ReactNode[] = [];
  if (progress) {
    const total = durationInFrames ?? scenes?.durationInFrames ?? config.durationInFrames;
    const done = filledFrames(frame, total);
    const y = H - progressInset - progressHeight / 2;
    const width = W - margin * 2;
    // One piece per scene, as wide as the scene is long, or the whole bar.
    const pieces = scenes && segments
      ? scenes.scenes.filter((s) => s.durationInFrames > 0).map((s) => ({ key: s.id, from: s.from, length: s.durationInFrames }))
      : [{ key: 'bar', from: 0, length: total }];
    const span = pieces.reduce((sum, piece) => sum + piece.length, 0);
    const room = width - segmentGap * (pieces.length - 1);
    let x = margin;
    for (const piece of pieces) {
      const w = span > 0 ? (room * piece.length) / span : 0;
      const filled = piece.length > 0 ? Math.min(1, Math.max(0, (done - piece.from) / piece.length)) : 1;
      bars.push(React.createElement(Group, { key: piece.key, x, y },
        trackOpacity > 0 ? React.createElement(Rect, { width: w, height: progressHeight, fill: ink, opacity: trackOpacity }) : null,
        filled > 0 ? React.createElement(Rect, { width: w * filled, height: progressHeight, fill }) : null,
      ));
      x += w + segmentGap;
    }
  }

  // Two groups, so the bar can blend apart from the marks; each carries the
  // opacity itself, since a translucent parent would blend them against it
  // instead of against the picture.
  return React.createElement(React.Fragment, null,
    brackets.length + labels.length > 0 ? React.createElement(Group, { key: 'marks', blendMode, opacity }, brackets, labels) : null,
    bars.length > 0 ? React.createElement(Group, { key: 'progress', blendMode: progressBlendMode, opacity }, bars) : null,
  );
}
