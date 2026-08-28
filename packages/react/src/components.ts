import * as React from 'react';
import type { ReactNode } from 'react';

import { CompositionRuntimeContext } from './hooks';
import type { Animatable, TextStyle } from './scene';
import { secondsFromTime, secondsToTime } from './time';

// Unlike the pre-reconciler tree walker, these are real function components:
// each returns a host element (a lowercase intrinsic type the reconciler in
// reconciler.ts understands), so ordinary React composition — conditionals,
// `.map()`, context, hooks — works through them exactly as it would for any
// other component.

export interface CommonProps {
  id?: string;
  x?: number;
  y?: number;
  scale?: number;
  scaleX?: number;
  scaleY?: number;
  rotation?: number;
  anchorX?: number;
  anchorY?: number;
  opacity?: number;
}

export interface CompositionProps {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  children?: ReactNode;
}

export interface GroupProps extends CommonProps {
  children?: ReactNode;
}

export interface ImageProps extends CommonProps {
  src: string;
}

export interface TextProps extends CommonProps {
  children: ReactNode;
  style?: TextStyle;
  maxWidth?: number;
}

export interface RectProps extends CommonProps {
  width: number;
  height: number;
  /** Hex fill color (`"#RRGGBB"` or `"#RRGGBBAA"`). Omit for no fill. */
  fill?: string;
  /** Hex stroke color; has no visible effect without `strokeWidth`. */
  stroke?: string;
  strokeWidth?: number;
  cornerRadius?: number;
}

export interface VideoProps extends CommonProps {
  src: string;
  /**
   * Seconds into the source file playback starts from, at the sequence's own
   * frame 0 — the counterpart of a project timeline clip's trim-in point.
   * Defaults to 0.
   */
  startFrom?: number;
  /**
   * Speed the source plays back at relative to the enclosing sequence's own
   * clock (2 plays twice as fast, 0.5 half as fast). Defaults to 1. A React
   * `<Video>` has no per-frame automation the way a project timeline clip's
   * `Animatable<f64>` playback rate does — it is a single static value for
   * the whole clip.
   */
  playbackRate?: number;
}

export function Composition(props: CompositionProps): ReturnType<typeof React.createElement> {
  return React.createElement('composition', props);
}

export function Group(props: GroupProps): ReturnType<typeof React.createElement> {
  return React.createElement('group', props);
}

export function Image(props: ImageProps): ReturnType<typeof React.createElement> {
  return React.createElement('image', props);
}

export function Text(props: TextProps): ReturnType<typeof React.createElement> {
  return React.createElement('text', props);
}

/** A flat-shaded rectangle, optionally rounded and/or stroked. */
export function Rect(props: RectProps): ReturnType<typeof React.createElement> {
  return React.createElement('rect', props);
}

export function Video(props: VideoProps): ReturnType<typeof React.createElement> {
  return React.createElement('video', props);
}

export interface SequenceProps extends CommonProps {
  /**
   * Frame this sequence starts at, in the enclosing timeline's own frame
   * numbering (the root composition's clock, or its parent `<Sequence>`'s
   * shifted clock when nested). Defaults to 0.
   */
  from?: number;
  /**
   * Length in frames. Children are only visible — and only contribute audio —
   * inside `[from, from + durationInFrames)`; defaults to running until the
   * end of the enclosing window.
   */
  durationInFrames?: number;
  children?: ReactNode;
}

/**
 * Places its children at an offset of the composition's timeline. Inside a
 * `<Sequence>`, `useCurrentFrame()`/`useCurrentTime()` report the shifted
 * local clock (`frame - from`), and `<Video>`/`<Audio>` play synced to that
 * same local clock, so media placed inside a sequence starts when the
 * sequence does. Children outside the window render no layers and collect no
 * audio; the range itself is evaluated by the renderer per requested frame,
 * so sequences work under conditionals, `.map()`, and hooks like any other
 * component.
 */
export function Sequence(props: SequenceProps): ReturnType<typeof React.createElement> {
  // The host element ('sequence') carries only static props; shifting the
  // clock for descendant components must happen here, through context, while
  // they render. Whether the sequence is active at a given composition time
  // is decided later, by render.ts's walker, which sees every 'sequence'
  // node regardless of the current time.
  const context = React.useContext(CompositionRuntimeContext);
  if (!context) {
    throw new Error('<Sequence> must be called from within a Mikan <Composition>');
  }
  const fps = context.fps;
  const localSeconds = secondsFromTime(context.time);
  const from = typeof props.from === 'number' && Number.isFinite(props.from) ? props.from : 0;
  const shiftedTime = secondsToTime(localSeconds - from / fps);
  const duration =
    typeof props.durationInFrames === 'number'
      ? Math.max(0, props.durationInFrames)
      : Math.max(0, context.durationInFrames - from);
  return React.createElement(
    CompositionRuntimeContext.Provider,
    {
      value: {
        ...context,
        time: shiftedTime,
        durationInFrames: duration,
      },
    },
    React.createElement('sequence', props),
  );
}

export type AnimatedNumber = Animatable<number>;

export interface AudioProps extends CommonProps {
  src: string;
  /**
   * Seconds into the source file playback starts from, at the sequence's own
   * frame 0. Defaults to 0.
   */
  startFrom?: number;
  /**
   * Speed the source plays back at relative to the enclosing sequence's own
   * clock. Defaults to 1. Either a plain number or a generated
   * `KeyframeAnimation` over seconds-into-the-sequence times, matching the
   * project format's `Animatable<f64>` playback rate.
   */
  playbackRate?: AnimatedNumber;
  /**
   * Linear volume multiplier, 0 silent, 1 unchanged. Defaults to 1. Either a
   * plain number or a generated `KeyframeAnimation` over
   * seconds-into-the-sequence times, matching the project format's
   * `Animatable<f64>` volume automation.
   */
  volume?: AnimatedNumber;
  /** Defaults to false. */
  muted?: boolean;
}

/**
 * Declares one audio clip playing synced to the enclosing timeline's own
 * clock — bare, it spans the whole composition; inside one or more
 * `<Sequence>`s it is bounded by them and starts when the innermost sequence
 * does. Audio declarations are gathered from the rendered frame tree (the
 * same walk that produces layers), so an `<Audio>` behind an ordinary React
 * conditional or hook contributes sound on exactly the frames where it
 * renders, with its full audible range derived from any enclosing sequences.
 */
export function Audio(props: AudioProps): ReturnType<typeof React.createElement> {
  return React.createElement('audio', props);
}
