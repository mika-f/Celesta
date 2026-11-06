// Shared exact-time helpers. `Time` is the generated `{ value, timescale }`
// rational from celesta_composition; seconds are converted at a microsecond
// timescale — comfortably finer than any frame rate or audio clock — so
// trimming and sequence offsets are not visibly quantized.

import type { Time } from './scene';

export const SECONDS_TIMESCALE = 1_000_000;

export function secondsToTime(seconds: number): Time {
  return { value: Math.round(seconds * SECONDS_TIMESCALE), timescale: SECONDS_TIMESCALE };
}

export function secondsFromTime(time: Time): number {
  return time.value / time.timescale;
}

/**
 * How far apart two times may be and still be the same window edge. Each
 * `<Sequence>` rounds the time it shifts to a microsecond, so frame 10 at
 * 30 fps is 0.333333 s inside one, and nested ones add up the error.
 */
const EDGE_TOLERANCE_SECONDS = 10 / SECONDS_TIMESCALE;

/** Whether `seconds` is before the window edge `edgeSeconds`; see `EDGE_TOLERANCE_SECONDS`. */
export function isBefore(seconds: number, edgeSeconds: number): boolean {
  return seconds < edgeSeconds - EDGE_TOLERANCE_SECONDS;
}

/**
 * Whether a window ending at `endSec` (composition seconds) ends together
 * with the composition. Half a frame of slack absorbs rounding in summed
 * sequence offsets.
 */
export function endsWithComposition(endSec: number, compositionEndSec: number, fps: number): boolean {
  return Math.abs(endSec - compositionEndSec) < 0.5 / fps;
}

/** The window a `<Sequence>` is placed in. */
export interface SequenceParent {
  fps: number;
  /** Where the enclosing window ends, in its local seconds. */
  endSec: number;
  /** The composition time, in seconds, of the enclosing window's local 0. */
  originSec: number;
  /** Where the composition ends, in seconds. */
  compositionEndSec: number;
  /**
   * Whether the frame is rendered past the composition's end with the
   * enclosing window held open.
   */
  openEnd: boolean;
}

/** A `<Sequence>`'s window, in its parent's local clock. */
export interface SequenceWindow {
  /** `from`, in frames. */
  from: number;
  /** `durationInFrames`; undefined when the window runs to the parent's end. */
  durationInFrames: number | undefined;
  /** Where the window starts, in the parent's local seconds. */
  startSec: number;
  /** Where the window ends, in the parent's local seconds. */
  endSec: number;
  /** Where the window ends, in composition seconds. */
  compositionEndSec: number;
  /**
   * Whether the window stays open past `endSec`: past the composition's end
   * (see `SequenceParent.openEnd`), a window that ends with the composition
   * stays open, and one that ends later keeps its own end.
   */
  openEnd: boolean;
}

/**
 * The window of a `<Sequence>` with these `from` and `durationInFrames`
 * props. `<Sequence>` and render.ts's walker both place sequences with it,
 * so the children React mounts are the ones the walker collects.
 *
 * `from` must be a finite number when set; it defaults to 0.
 * `durationInFrames` must be at least 0 when set: `0` is an empty window,
 * and `undefined` or `Infinity` runs to the end of the parent.
 */
export function sequenceWindow(
  props: { from?: unknown; durationInFrames?: unknown },
  parent: SequenceParent,
): SequenceWindow {
  const { from = 0, durationInFrames } = props;
  if (typeof from !== 'number' || !Number.isFinite(from)) {
    throw new Error(`<Sequence> \`from\` must be a finite number of frames, not ${describe(from)}`);
  }
  if (
    durationInFrames !== undefined &&
    (typeof durationInFrames !== 'number' || Number.isNaN(durationInFrames) || durationInFrames < 0)
  ) {
    throw new Error(
      `<Sequence> \`durationInFrames\` must be a number of frames of at least 0, or Infinity, not ${describe(durationInFrames)}`,
    );
  }
  const fps = parent.fps;
  const bounded = durationInFrames !== undefined && durationInFrames !== Infinity;
  const endSec = bounded ? (from + durationInFrames) / fps : parent.endSec;
  const compositionEndSec = parent.originSec + endSec;
  return {
    from,
    durationInFrames: bounded ? durationInFrames : undefined,
    startSec: from / fps,
    endSec,
    compositionEndSec,
    openEnd: parent.openEnd && endsWithComposition(compositionEndSec, parent.compositionEndSec, fps),
  };
}

/** `value` as an error message shows it: strings quoted, numbers as written. */
function describe(value: unknown): string {
  return typeof value === 'string' ? JSON.stringify(value) : String(value);
}

/** Converts a `HH:MM:SS(.mmm)`, `MM:SS(.mmm)`, or `SS(.mmm)` timecode to a frame. */
export function timecodeToFrame(timecode: string, fps: number): number {
  const trimmed = timecode.trim();
  if (!/^\d+(?::\d+){0,2}(?:\.\d{1,3})?$/.test(trimmed)) {
    throw new Error(`Invalid timecode: ${timecode}`);
  }
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('timecodeToFrame() requires a positive fps');
  }

  const seconds = trimmed
    .split(':')
    .reduce((total, component) => total * 60 + Number(component), 0);
  return Math.round(seconds * fps);
}

export interface TimecodeOptions {
  /**
   * `false` leaves the hours out: `MM:SS:FF`, the minutes counting on past
   * 59 instead of wrapping. Defaults to `true`.
   */
  hours?: boolean;
}

/**
 * Formats a frame as an `HH:MM:SS:FF` timecode, where `FF` counts frames
 * within the second, or `MM:SS:FF` with `{ hours: false }`. Negative frames
 * are formatted as frame 0.
 */
export function frameToTimecode(frame: number, fps: number, options: TimecodeOptions = {}): string {
  if (!Number.isInteger(fps) || fps <= 0) {
    throw new Error('frameToTimecode() requires a positive integer fps');
  }
  const whole = Math.max(0, Math.floor(frame));
  const seconds = Math.floor(whole / fps);
  const pad = (n: number) => String(n).padStart(2, '0');
  const tail = `${pad(seconds % 60)}:${pad(whole % fps)}`;
  if (options.hours === false) {
    return `${pad(Math.floor(seconds / 60))}:${tail}`;
  }
  return `${pad(Math.floor(seconds / 3600))}:${pad(Math.floor(seconds / 60) % 60)}:${tail}`;
}
