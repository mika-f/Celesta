// The card a rough cut shows where a scene is not built yet: its name, what
// happens in it, where it sits in the list and how long it lasts, over a
// bar that fills as the scene plays. A pure function of the frame, drawn
// with the core's own primitives so `<Scenes>` can draw it. Its text names no
// font family: the renderer's default face draws it without a fallback-font
// warning on every rough export.

import * as React from 'react';

import { Group, Rect, Text } from './components';
import { useCurrentFrame, useVideoConfig } from './hooks';

export interface ScenePlaceholderProps {
  /** The scene's label, drawn large. */
  name: string;
  /** One sentence on what happens in the scene, drawn under the name. */
  desc?: string;
  /** The scene's place in its list, from 0: drawn as "Scene 2 of 5". */
  index?: number;
  /** How many scenes the list has. */
  count?: number;
  /** The scene's length in frames; the bar fills over it. */
  durationInFrames: number;
  /** The tag in the top right corner. Defaults to `'Rough cut'`. */
  label?: string;
}

const GROUND = '#17191d';
const FRAME = '#4a5059';
const TITLE = '#f1f2f4';
const BODY = '#b4bac3';
const MUTED = '#8b929c';
const MARK = '#e0a644';
const TRACK = '#2c3036';

/** Seconds, with up to two decimals and no trailing zeros. */
function seconds(frames: number, fps: number): string {
  return `${Number((frames / fps).toFixed(2))} s`;
}

/**
 * A placeholder card for a scene that is not built yet, sized from the
 * composition: `<Scenes>` draws it for an entry with `status: 'rough'` or
 * without a `Scene`. Place it in a `<Sequence>` of the scene's length to draw
 * it yourself (a chapter laid out with `<TransitionSeries>`, say); the bar
 * reads the caller's frame.
 *
 * ```tsx
 * <Sequence from={90} durationInFrames={120}>
 *   <ScenePlaceholder name="The card" desc="A card slides onto the blueprint." index={1} count={4} durationInFrames={120} />
 * </Sequence>
 * ```
 */
export function ScenePlaceholder({
  name,
  desc,
  index,
  count,
  durationInFrames,
  label = 'Rough cut',
}: ScenePlaceholderProps): React.ReactElement {
  const frame = useCurrentFrame();
  const { width, height, fps } = useVideoConfig();
  const u = Math.min(width, height) / 1080;
  const margin = 64 * u;
  const pad = margin + 48 * u;
  const inner = Math.max(0, width - 2 * pad);
  const last = Math.max(1, durationInFrames - 1);
  const progress = Math.min(1, Math.max(0, frame / last));
  const place = index === undefined ? null : count === undefined ? `Scene ${index + 1}` : `Scene ${index + 1} of ${count}`;
  const small = 30 * u;
  const h = React.createElement;
  return h(
    Group,
    null,
    h(Rect, { width, height, fill: GROUND }),
    h(Rect, {
      x: margin,
      y: margin,
      width: Math.max(0, width - 2 * margin),
      height: Math.max(0, height - 2 * margin),
      stroke: FRAME,
      strokeWidth: 2 * u,
      dash: [16 * u, 12 * u],
    }),
    place === null
      ? null
      : h(Text, { x: pad, y: pad, fontSize: small, color: MUTED, children: place }),
    h(Text, { x: width - pad, y: pad, anchorX: 1, fontSize: small, color: MARK, children: label }),
    h(Text, {
      x: pad,
      y: height / 2 - 16 * u,
      anchorY: 1,
      maxWidth: inner,
      fontSize: 96 * u,
      color: TITLE,
      children: name,
    }),
    desc
      ? h(Text, { x: pad, y: height / 2 + 24 * u, maxWidth: inner, fontSize: 42 * u, color: BODY, children: desc })
      : null,
    h(Text, {
      x: pad,
      y: height - pad - 30 * u,
      anchorY: 1,
      fontSize: small,
      color: MUTED,
      children: `${seconds(durationInFrames, fps)} · ${durationInFrames} frames`,
    }),
    h(Text, {
      x: width - pad,
      y: height - pad - 30 * u,
      anchorX: 1,
      anchorY: 1,
      fontSize: small,
      color: MUTED,
      children: seconds(Math.min(frame, durationInFrames), fps),
    }),
    h(Rect, { x: pad, y: height - pad - 6 * u, width: inner, height: 6 * u, fill: TRACK }),
    progress > 0 ? h(Rect, { x: pad, y: height - pad - 6 * u, width: inner * progress, height: 6 * u, fill: MARK }) : null,
  );
}
