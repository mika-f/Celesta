// `<MotionBlur>` and `<Trail>`: draw their children again at other moments
// near the frame. Each moment is a 'time-sample' host element whose children
// see that moment through the runtime context, so components inside that
// derive their animation from `useCurrentFrame()`/`useCurrentTime()` move
// between the copies. render.ts walks each sample at its moment.

import * as React from 'react';
import type { ReactNode } from 'react';

import type { CommonProps } from './components';
import {
  CompositionRuntimeContext,
  type CompositionRuntimeContextValue,
  FreezeFrameContext,
  runtimeFrame,
  useCurrentFrame,
} from './hooks';
import { markTransparent } from './source-chain';
import { secondsToTime } from './time';

/** Children drawn at each moment: elements, or a function of that moment's frame. */
export type MomentChildren = ReactNode | ((frame: number) => ReactNode);

export interface MotionBlurProps extends CommonProps {
  /**
   * How long the shutter stays open, in degrees of a frame: 360 is one
   * whole frame. The open interval is centred on the frame. From 0 to 720;
   * defaults to 180.
   */
  shutterAngle?: number;
  /**
   * How many moments within the shutter are drawn and averaged, a whole
   * number from 1 to 64. Each costs a full draw of the children. Defaults
   * to 8.
   */
  samples?: number;
  /**
   * Elements, or a function of the frame drawn: `{(frame) => …}` passes each
   * moment's frame to props computed here, which would otherwise be the
   * same in every moment.
   */
  children?: MomentChildren;
}

export interface TrailProps extends CommonProps {
  /** How many earlier copies are drawn beneath the children, a whole number from 0 to 64. Defaults to 4. */
  copies?: number;
  /** How many frames each copy lags behind the one above it. Positive; defaults to 1. */
  lagInFrames?: number;
  /**
   * The opacity of the newest copy, from 0 to 1; each older one fades by
   * the same step, down to `trailOpacity / copies` for the oldest.
   * Defaults to 0.5.
   */
  trailOpacity?: number;
  /** As for `<MotionBlur>`. */
  children?: MomentChildren;
}

function requireRuntime(name: string): CompositionRuntimeContextValue {
  const context = React.useContext(CompositionRuntimeContext);
  if (!context) {
    throw new Error(`<${name}> must be called from within a Celesta <Composition>`);
  }
  return context;
}

function requireNumber(value: number, name: string, min: number, max: number, integer = false): number {
  if (!Number.isFinite(value) || value < min || value > max || (integer && !Number.isInteger(value))) {
    const kind = integer ? 'a whole number' : 'a finite number';
    throw new Error(`${name} must be ${kind} from ${min} to ${max}`);
  }
  return value;
}

function FrameChildren({ render }: { render: (frame: number) => ReactNode }): React.ReactElement {
  return React.createElement(React.Fragment, null, render(useCurrentFrame()));
}

function momentChildren(children: MomentChildren): ReactNode {
  return typeof children === 'function' ? React.createElement(FrameChildren, { render: children }) : children;
}

/** One object per moment, stable across frames, for `FreezeFrameContext`. */
function useScopes(count: number): object[] {
  const scopes = React.useRef<object[]>([]).current;
  while (scopes.length < count) scopes.push({});
  return scopes;
}

/**
 * `children` drawn at the composition frame `frame`, whose runtime context
 * reports that frame exactly: `frameOffset` carries its distance from a
 * whole frame, which a microsecond `Time` cannot hold exactly.
 */
function moment(
  context: CompositionRuntimeContextValue,
  frame: number,
  key: string | number,
  scope: object | null,
  hostProps: Record<string, unknown>,
  children: MomentChildren,
): ReactNode {
  const offset = context.frameOffset ?? 0;
  const base = runtimeFrame(context.time, context.fps, offset);
  const time = secondsToTime(frame / context.fps);
  const sample = React.createElement('time-sample', { ...hostProps, time }, momentChildren(children));
  return React.createElement(
    CompositionRuntimeContext.Provider,
    { key, value: { ...context, time, frameOffset: offset + (frame - base) } },
    scope ? React.createElement(FreezeFrameContext.Provider, { value: scope }, sample) : sample,
  );
}

/**
 * The frames a `<MotionBlur>` draws at `frame`: the middles of `samples`
 * equal parts of the shutter interval centred on it, kept within the
 * enclosing window's first and last frames (beyond the last only when the
 * window is open past the composition's end).
 */
export function motionBlurFrames(
  frame: number,
  shutterAngle: number,
  samples: number,
  durationInFrames: number,
  openEnd: boolean,
): number[] {
  const open = shutterAngle / 360;
  const last = openEnd ? Infinity : durationInFrames - 1;
  return Array.from({ length: samples }, (_, index) =>
    Math.max(0, Math.min(last, frame - open / 2 + ((index + 0.5) * open) / samples)));
}

/**
 * Blurs what moves inside it: draws its children at `samples` moments while
 * a shutter `shutterAngle` degrees wide is open around the frame, and shows
 * their average. Inside, `useCurrentFrame()` reports each moment's frame,
 * such as 12.25, so components that animate from it move between the
 * moments; props computed outside are the same in every moment, so pass a
 * function `{(frame) => …}` to compute them per moment. The result is the
 * same for any frame drawn in any order. `<Video>` and animated images
 * inside keep the frame's picture in every moment, as do lip sync and
 * blinking. Audio and subtitles inside play once. Every moment is a copy, like a
 * `<FreezeFrame>`'s: a `<CharacterView>` inside never attaches its ref, so a
 * `<Dialogue>` must be inside too.
 */
export function MotionBlur(props: MotionBlurProps): React.ReactElement {
  const context = requireRuntime('MotionBlur');
  const { shutterAngle = 180, samples = 8, children, ...rest } = props;
  requireNumber(shutterAngle, '<MotionBlur> shutterAngle', 0, 720);
  requireNumber(samples, '<MotionBlur> samples', 1, 64, true);
  const scopes = useScopes(samples);
  if (samples === 1 || shutterAngle === 0) {
    // One moment, the frame itself: nothing to average.
    return React.createElement('group', rest, momentChildren(children));
  }
  const frame = runtimeFrame(context.time, context.fps, context.frameOffset);
  const frames = motionBlurFrames(frame, shutterAngle, samples, context.durationInFrames, context.openEnd === true);
  return React.createElement(
    'motion-blur',
    rest,
    frames.map((sampleFrame, index) => moment(context, sampleFrame, index, scopes[index], {}, children)),
  );
}

/**
 * Leaves a trail behind what moves inside it: draws `copies` earlier copies
 * of its children, each `lagInFrames` frames older than the one above it
 * and fainter, beneath the children themselves. Copies from before the
 * enclosing window began are not drawn. Inside a copy, `useCurrentFrame()`
 * reports that copy's frame; as with `<MotionBlur>`, pass a function
 * `{(frame) => …}` for props computed here. `<Video>` and animated images
 * keep the frame's picture in every copy. Audio and subtitles come from the
 * children only. A copy fades as a `<Group>` with that opacity does: overlapping
 * layers within it show through each other.
 */
export function Trail(props: TrailProps): React.ReactElement {
  const context = requireRuntime('Trail');
  const { copies = 4, lagInFrames = 1, trailOpacity = 0.5, children, ...rest } = props;
  requireNumber(copies, '<Trail> copies', 0, 64, true);
  if (!Number.isFinite(lagInFrames) || lagInFrames <= 0) {
    throw new Error('<Trail> lagInFrames must be a positive finite number');
  }
  requireNumber(trailOpacity, '<Trail> trailOpacity', 0, 1);
  const scopes = useScopes(copies);
  const frame = runtimeFrame(context.time, context.fps, context.frameOffset);
  const moments: ReactNode[] = [];
  // Oldest first, so newer copies draw over older ones.
  for (let copy = copies; copy >= 1; copy--) {
    const copyFrame = frame - copy * lagInFrames;
    if (copyFrame < 0) continue;
    const opacity = trailOpacity * (1 - (copy - 1) / copies);
    moments.push(moment(context, copyFrame, copy, scopes[copy - 1], { opacity }, children));
  }
  moments.push(React.createElement('time-sample', { key: 'live', live: true, time: context.time }, momentChildren(children)));
  return React.createElement('trail', rest, moments);
}

for (const wrapper of [MotionBlur, Trail, FrameChildren]) markTransparent(wrapper);
