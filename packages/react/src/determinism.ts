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
  subtrees: Map<FrameClock, { seed: number }>;
}

/**
 * The composition frame (whole, or fractional for a `<MotionBlur>` sample)
 * a subtree is drawn at, with the composition's fps; its clock reads
 * `frame × 1000 / fps` milliseconds, as the frame itself would at the top.
 */
export interface FrameClock {
  frame: number;
  fps: number;
}

/**
 * The clock of the subtree being rendered when it is not the frame's: a
 * `<FreezeFrame>`'s frame, a `<MotionBlur>` sample's, a `<Trail>` copy's.
 * The clock and `Math.random` inside follow it, so a frozen subtree keeps its
 * values. Each provider renders a new object, and each object has a
 * `Math.random` sequence of its own, seeded from its time: the outer tree
 * and other subtrees at the same time do not draw from it.
 */
export const FrameClockContext = React.createContext<FrameClock | null>(null);

function clockMs(clock: FrameClock): number {
  return (clock.frame * 1000) / clock.fps;
}

/**
 * The value `FrameClockContext` has for the component being rendered, read
 * without subscribing it: React keeps a provider's value on the context
 * while it renders the provider's subtree (`_currentValue` for a primary
 * renderer, as Celesta's is), and the default outside. This runs on every
 * `Math.random()` call, where `useContext` would add a dependency each time.
 */
function subtreeClock(): FrameClock | null {
  return (FrameClockContext as unknown as { _currentValue?: FrameClock | null })._currentValue ?? null;
}

/**
 * The clock of the subtree being rendered, or `null` for the frame's; for a
 * `<MotionBlur>` sample inside one to offset.
 */
export function enclosingFrameClock(): FrameClock | null {
  return subtreeClock();
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
  const clock = subtreeClock();
  let state: { seed: number } = frame;
  if (clock !== null) {
    let subtree = frame.subtrees.get(clock);
    if (!subtree) {
      subtree = { seed: seedAt(clockMs(clock)) };
      frame.subtrees.set(clock, subtree);
    }
    state = subtree;
  }
  state.seed = (state.seed + 0x6d2b79f5) >>> 0;
  let t = state.seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

function nowMs(): number {
  const clock = subtreeClock();
  return clock === null ? current().ms : clockMs(clock);
}

function frozenNow(): number {
  return FROZEN_EPOCH_MS + Math.floor(nowMs());
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
  const holder: { stack?: unknown } = {};
  const ErrorWithHooks = Error as { prepareStackTrace?: (error: Error, sites: unknown[]) => unknown };
  const limit = Error.stackTraceLimit;
  const prepare = ErrorWithHooks.prepareStackTrace;
  let stack: unknown;
  // A few frames, past a built-in such as `Array.prototype.map(Math.random)`.
  Error.stackTraceLimit = 4;
  // V8's own call sites, as `    at fn (file:///…/entry.mjs:12:5)` lines:
  // with source maps on (the CLI turns them on for readable errors), the
  // stack string names the entry's original files, not the bundle.
  ErrorWithHooks.prepareStackTrace = (_, sites) => ['', ...sites.map((site) => `    at ${String(site)}`)].join('\n');
  try {
    capture(holder, replacement);
    stack = holder.stack;
  } finally {
    Error.stackTraceLimit = limit;
    ErrorWithHooks.prepareStackTrace = prepare;
  }
  // The first frame with a file position: built-ins have none.
  const caller = (typeof stack === 'string' ? stack : '')
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
  return nowMs();
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
