// Every frame must be a pure function of its time. While a frame is
// evaluated (`evaluateDeterministically`), `Math.random` draws from a
// generator seeded by the frame's time, and `Date.now()`, `performance.now()`
// and `new Date()` read a clock frozen at the frame's time, so the same frame
// gives the same values whatever order frames render in, in preview, export
// and inspection alike. Outside a frame (module scope, `prepare()`, between
// requests) they are the real ones.
//
// When the host says which code is the entry's (`setNondeterminismSource`),
// a call from it is also recorded, once per API, for the host to report
// (`takeNondeterministicCalls`): the output is deterministic either way, but
// a composition that leans on them renders differently from what its author
// expects (a "random" value that changes every frame, a clock that is the
// composition's time and not today's).
//
// Every render pass of a frame restarts the sequence, so a frame that needed
// a second pass (a font or ref discovered) draws what one pass draws. Code
// that keeps a `Math.random()` value across passes (`useMemo`) still makes
// the values after it depend on how many passes ran, as it makes them depend
// on which frames rendered before; the warning is for that. A subtree drawn
// at another time (`FrameClockContext`) has its own clock and sequence.

import * as React from 'react';

import type { Time } from './scene';

/** The APIs Celesta replaces while a frame is evaluated, as they are reported. */
export type NondeterministicApi = 'Math.random()' | 'Date.now()' | 'performance.now()' | 'new Date()';

/**
 * Whether a call site is the entry's own code: given one stack frame line
 * (`    at fn (file:///…/entry.mjs:12:5)`), true when it is.
 */
export type NondeterminismSource = (frame: string) => boolean;

/**
 * Milliseconds since the Unix epoch `Date.now()` reads at time 0: the clock
 * starts at 1970-01-01T00:00:00Z, so `Date.now()` is the frame's time in
 * milliseconds.
 */
export const FROZEN_EPOCH_MS = 0;

interface FrameState {
  /** The frame's time in milliseconds. */
  ms: number;
  /** The generator's state; see `nextRandom`. */
  seed: number;
  /** The generators of subtrees drawn at another time, by their clock. */
  subtrees: Map<number, { seed: number }>;
}

/**
 * The composition time, in milliseconds, of the subtree being rendered when
 * it is not the frame's: a `<FreezeFrame>`'s frame, a `<MotionBlur>` or
 * `<Trail>` sample's. The clock and `Math.random` inside follow it, so a
 * frozen subtree keeps its values.
 */
export const FrameClockContext = React.createContext<number | null>(null);

/**
 * The value `FrameClockContext` has for the component being rendered, read
 * without subscribing it: React keeps a provider's value on the context
 * while it renders the provider's subtree (`_currentValue` for a primary
 * renderer, as Celesta's is), and the default outside. This runs on every
 * `Math.random()` call, where `useContext` would add a dependency each time.
 */
function subtreeMs(): number | null {
  const value = (FrameClockContext as unknown as { _currentValue?: unknown })._currentValue;
  return typeof value === 'number' ? value : null;
}

/**
 * The clock, in milliseconds, of what is being rendered: a subtree's, or the
 * frame's; `null` outside a frame. For a `<MotionBlur>` sample to offset.
 */
export function frameClockMs(): number | null {
  return subtreeMs() ?? (frames.length > 0 ? current().ms : null);
}

const RealMath = { random: Math.random };
const RealDate = Date;
const realPerformanceNow = Object.getOwnPropertyDescriptor(globalThis.performance ?? {}, 'now');

/** The frames being evaluated, innermost last; empty outside a frame. */
const frames: FrameState[] = [];
let source: NondeterminismSource | null = null;
/** APIs reported so far, and those not yet taken. */
const reported = new Set<NondeterministicApi>();
let pending: NondeterministicApi[] = [];

/**
 * Says which stack frames are the entry's, so that its calls to the replaced
 * APIs are recorded; `null` records none. The CLI passes its bundle's URL.
 */
export function setNondeterminismSource(next: NondeterminismSource | null): void {
  source = next;
}

/** The APIs the entry called while a frame was evaluated, each once, since the last call. */
export function takeNondeterministicCalls(): NondeterministicApi[] {
  const taken = pending;
  pending = [];
  return taken;
}

/** Forgets what was reported, for tests. */
export function resetNondeterministicCalls(): void {
  reported.clear();
  pending = [];
}

/** The seed `Math.random` restarts from at `time`. */
export function frameSeed(time: Time): number {
  return seedAt(timeMs(time));
}

function timeMs(time: Time): number {
  return (time.value * 1000) / time.timescale;
}

/** A hash (FNV-1a) of the time in milliseconds, which `1/30` and `2/60` share. */
function seedAt(ms: number): number {
  const key = String(ms);
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i += 1) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * Runs `evaluate` with `Math.random`, `Date.now()`, `performance.now()` and
 * `new Date()` derived from `time`. `Math.random` starts its sequence afresh
 * on every call, also nested ones, and a nested call leaves the outer
 * sequence where it was, so a frame's values do not depend on how many
 * render passes it took.
 */
export function evaluateDeterministically<T>(time: Time, evaluate: () => T): T {
  const ms = timeMs(time);
  const state: FrameState = { ms, seed: seedAt(ms), subtrees: new Map() };
  if (frames.length === 0) install();
  frames.push(state);
  try {
    return evaluate();
  } finally {
    frames.pop();
    if (frames.length === 0) uninstall();
  }
}

function current(): FrameState {
  return frames[frames.length - 1];
}

/**
 * mulberry32: the next number in `[0, 1)` from the generator of what is being
 * rendered, the frame's or a subtree's at another time.
 */
function nextRandom(): number {
  const frame = current();
  const ms = subtreeMs();
  let state: { seed: number } = frame;
  if (ms !== null && ms !== frame.ms) {
    let subtree = frame.subtrees.get(ms);
    if (!subtree) {
      subtree = { seed: seedAt(ms) };
      frame.subtrees.set(ms, subtree);
    }
    state = subtree;
  }
  state.seed = (state.seed + 0x6d2b79f5) >>> 0;
  let t = state.seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

function clockMs(): number {
  return subtreeMs() ?? current().ms;
}

function frozenNow(): number {
  return FROZEN_EPOCH_MS + Math.floor(clockMs());
}

/**
 * Records `api` when the code that called `replacement` is the entry's.
 * Only the first call of each API is looked at closely: capturing a stack
 * costs a few microseconds, and a particle field calls `Math.random()`
 * thousands of times a frame.
 */
function record(api: NondeterministicApi, replacement: Function): void {
  if (source === null || reported.has(api)) return;
  const capture = (Error as { captureStackTrace?: (target: object, constructor?: Function) => void }).captureStackTrace;
  if (!capture) return;
  const holder: { stack?: string } = {};
  const limit = Error.stackTraceLimit;
  // A few frames, past a built-in such as `Array.prototype.map(Math.random)`.
  Error.stackTraceLimit = 4;
  try {
    capture(holder, replacement);
  } finally {
    Error.stackTraceLimit = limit;
  }
  // The first frame with a file position: built-ins have none.
  const caller = (holder.stack ?? '')
    .split('\n')
    .slice(1)
    .find((line) => /:\d+:\d+\)?$/.test(line));
  if (caller === undefined || !source(caller)) return;
  reported.add(api);
  pending.push(api);
}

function random(): number {
  record('Math.random()', random);
  return nextRandom();
}

function dateNow(): number {
  record('Date.now()', dateNow);
  return frozenNow();
}

function performanceNow(): number {
  record('performance.now()', performanceNow);
  return clockMs();
}

/**
 * `Date` with `new Date()` and `Date()` frozen. Instances are real `Date`s:
 * the prototype is shared, so `instanceof Date` holds either way.
 */
const FrozenDate = function Date(this: unknown, ...args: unknown[]) {
  if (new.target === undefined) {
    record('new Date()', FrozenDate);
    return new RealDate(frozenNow()).toString();
  }
  if (args.length === 0) {
    record('new Date()', FrozenDate);
    return Reflect.construct(RealDate, [frozenNow()], new.target);
  }
  return Reflect.construct(RealDate, args, new.target);
} as unknown as DateConstructor;
Object.setPrototypeOf(FrozenDate, RealDate);
(FrozenDate as { prototype: Date }).prototype = RealDate.prototype;
Object.defineProperty(FrozenDate, 'now', { value: dateNow, writable: true, configurable: true });

function install(): void {
  Math.random = random;
  globalThis.Date = FrozenDate;
  if (globalThis.performance) {
    Object.defineProperty(globalThis.performance, 'now', { value: performanceNow, writable: true, configurable: true });
  }
}

function uninstall(): void {
  Math.random = RealMath.random;
  globalThis.Date = RealDate;
  if (globalThis.performance) {
    if (realPerformanceNow) Object.defineProperty(globalThis.performance, 'now', realPerformanceNow);
    else delete (globalThis.performance as { now?: unknown }).now;
  }
}
