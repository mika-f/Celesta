// Where a layer's JSX was written: the host node's fiber, walked up through
// the components that rendered it. Only elements the dev runtime recorded a
// position for take part, so a production-built tree yields no chain.

import { CompositionRuntimeContext, runtimeFrame } from './hooks';
import { sourceOfProps } from './jsx-dev-runtime';
import type { HostNode } from './reconciler';

export interface LayerSourceEntry {
  name: string;
  file: string;
  line: number;
  column: number;
  /**
   * A Celesta layer component (`<Rect>`, `<Group>`, ...), a Celesta component
   * that draws its caller's JSX (`<Sequence>`, `<Loop>`, ...), or anything else.
   */
  kind: 'primitive' | 'wrapper' | 'component';
  /** The layer a primitive entry produced, when it can be told. */
  layerId?: string;
}

/** The transform props a primitive element received, finite numbers only. */
export interface ReceivedProps {
  x?: number;
  y?: number;
  rotation?: number;
  scale?: number;
  scaleX?: number;
  scaleY?: number;
}

export interface LayerSourceReport {
  chain: LayerSourceEntry[];
  // The fields below are present only when the chain is not empty.
  /**
   * The frame `useCurrentFrame()` reports in the component that most likely
   * wrote the innermost primitive's JSX (its owner); not the frame at the
   * element, which a `<Sequence>` between the two shifts. A guess, for
   * display: `candidateFrames` holds every frame it could be.
   */
  localFrame?: number;
  /**
   * Every distinct frame `useCurrentFrame()` reports under the frame
   * providers on the innermost primitive's way to the root, nearest first.
   * Whichever component read the `frame` an `interpolate(frame, …)` for the
   * element uses sits above it (data only flows down), so that frame is one
   * of these; the editor tells them apart by the value the element received.
   */
  candidateFrames?: number[];
  props?: ReceivedProps;
  /** How many layers this element's JSX produced in the frame. */
  renderCount?: number;
  /** The timing wrappers around the innermost primitive, nearest first. */
  windows?: TimingWindow[];
  /** The composition frames the element is on screen in: every window and the composition, intersected. */
  span?: { from: number; to: number };
}

export interface SourcePos {
  file: string;
  line: number;
  column: number;
}

/** The frames a timing wrapper around the element shows its children in. */
export interface TimingWindow {
  kind: 'Sequence' | 'FreezeFrame' | 'Loop';
  /** In composition frames. */
  from: number;
  /** Exclusive. */
  to: number;
  /** Where the wrapper was written; a `<Scenes>` scene, say, has none. */
  source?: SourcePos;
  /**
   * Inside a `<Loop>` or `<FreezeFrame>` (or that wrapper itself), whose
   * children see other frames than the composition's: `from`/`to` are then
   * the window around the remapping, not one of their own.
   */
  remapped?: true;
}

type WindowKind = TimingWindow['kind'] | 'LoopCycle';

const primitives = new WeakSet<object>();
const transparent = new WeakSet<object>();
const windowKinds = new WeakMap<object, WindowKind>();

/**
 * Marks a component as a Celesta layer primitive, so a source chain can tell
 * the author's own `Rect` from `<Rect>`. Unwraps `forwardRef` and `memo`.
 */
export function markPrimitive(component: unknown): void {
  const target = unwrap(component);
  if (target !== null) primitives.add(target);
}

/**
 * Marks a Celesta component that draws JSX written by its caller
 * (`<Sequence>`, `<FreezeFrame>`, `<MotionBlur>`'s moments, ...): it never
 * owns that JSX, so the frame an `interpolate(frame, …)` reads is looked up
 * above it. A source chain also reports it as a `'wrapper'` entry.
 */
export function markTransparent(component: unknown): void {
  const target = unwrap(component);
  if (target !== null) transparent.add(target);
}

/**
 * Marks a Celesta component that limits the frames its children are shown
 * in, so a layer source report lists it in `windows`. `LoopCycle` is
 * `<Loop>`'s inner cycle, which the report folds into the `<Loop>`.
 */
export function markWindow(component: unknown, kind: WindowKind): void {
  const target = unwrap(component);
  if (target !== null) windowKinds.set(target, kind);
}

function windowKind(fiber: Fiber): WindowKind | undefined {
  const target = unwrap(fiber.elementType ?? fiber.type);
  return target === null ? undefined : windowKinds.get(target);
}

function unwrap(component: unknown): object | null {
  if (typeof component === 'function') return component;
  if (typeof component !== 'object' || component === null) return null;
  const wrapper = component as { type?: unknown; render?: unknown };
  return unwrap(wrapper.render ?? wrapper.type) ?? component;
}

function isPrimitive(type: unknown): boolean {
  const target = unwrap(type);
  return target !== null && primitives.has(target);
}

function isTransparent(type: unknown): boolean {
  const target = unwrap(type);
  return target !== null && transparent.has(target);
}

interface Fiber {
  return: Fiber | null;
  alternate?: Fiber | null;
  child?: Fiber | null;
  sibling?: Fiber | null;
  stateNode?: unknown;
  type: unknown;
  elementType: unknown;
  memoizedProps: unknown;
  pendingProps: unknown;
}

function typeName(type: unknown): string {
  if (typeof type === 'string') return type;
  if (typeof type === 'function') {
    const named = type as { displayName?: string; name: string };
    return named.displayName || named.name || 'Anonymous';
  }
  if (typeof type === 'object' && type !== null) {
    // `memo` and `forwardRef` wrap the component the author wrote.
    const wrapper = type as { displayName?: string; type?: unknown; render?: unknown };
    return wrapper.displayName || (wrapper.type ? typeName(wrapper.type) : wrapper.render ? typeName(wrapper.render) : 'Anonymous');
  }
  return 'Anonymous';
}

/** Steps from a fiber to the one above it. */
type Up = (fiber: Fiber) => Fiber | null;

/**
 * Which of a fiber and its alternate is in the committed tree. A host node
 * keeps the fiber of the last render that changed its props (the reconciler
 * skips `commitUpdate` otherwise), and a subtree React bailed out of keeps
 * `return` pointers into the previous render, so both a node's fiber and the
 * fibers above it can hold an earlier frame. The committed tree is the one
 * reachable through `child` and `sibling` from the root's `current`.
 */
function committedFibers(start: Fiber | null): (fiber: Fiber) => Fiber {
  let top = start;
  while (top?.return) top = top.return;
  const root = (top?.stateNode as { current?: Fiber } | null | undefined)?.current;
  if (!root) return (fiber) => fiber;
  const committed = new Set<Fiber>();
  const stack: Fiber[] = [root];
  while (stack.length > 0) {
    const fiber = stack.pop() as Fiber;
    committed.add(fiber);
    if (fiber.sibling) stack.push(fiber.sibling);
    if (fiber.child) stack.push(fiber.child);
  }
  return (fiber) => (committed.has(fiber) || !fiber.alternate ? fiber : fiber.alternate);
}

/**
 * The elements that produced `node`, nearest first: the host element itself,
 * then each component element above it, each at the position it was written.
 * `resolveFile` turns the recorded file name into the path to report.
 */
export function sourceChain(
  node: HostNode,
  resolveFile: (fileName: string) => string,
  layerIdOf?: (node: HostNode) => string | undefined,
): LayerSourceEntry[] {
  return sourceFibers(node, resolveFile, layerIdOf).map(({ entry }) => entry);
}

function sourceFibers(
  node: HostNode,
  resolveFile: (fileName: string) => string,
  layerIdOf?: (node: HostNode) => string | undefined,
  committed: (fiber: Fiber) => Fiber = (fiber) => fiber,
): { entry: LayerSourceEntry; fiber: Fiber }[] {
  const chain: { entry: LayerSourceEntry; fiber: Fiber }[] = [];
  const start = node.fiber as Fiber | null | undefined;
  for (let fiber = start ? committed(start) : null; fiber; fiber = fiber.return ? committed(fiber.return) : null) {
    const source = sourceOfProps(fiber.memoizedProps ?? fiber.pendingProps);
    if (!source) continue;
    const file = resolveFile(source.fileName);
    const last = chain[chain.length - 1]?.entry;
    if (last && last.file === file && last.line === source.lineNumber && last.column === source.columnNumber) continue;
    const type = fiber.elementType ?? fiber.type;
    const entry: LayerSourceEntry = {
      name: typeName(type),
      file,
      line: source.lineNumber,
      column: source.columnNumber,
      kind: isPrimitive(type) ? 'primitive' : isTransparent(type) ? 'wrapper' : 'component',
    };
    if (entry.kind === 'primitive') {
      const host = firstHostNode(fiber);
      const id = host ? layerIdOf?.(host) : undefined;
      if (id !== undefined) entry.layerId = id;
    }
    chain.push({ entry, fiber });
  }
  return chain;
}

/** The first host node at or below `fiber`, depth first. */
function firstHostNode(fiber: Fiber): HostNode | null {
  if (typeof fiber.type === 'string') return (fiber.stateNode as HostNode | undefined) ?? null;
  for (let child = fiber.child ?? null; child; child = child.sibling ?? null) {
    const found = firstHostNode(child);
    if (found) return found;
  }
  return null;
}

const TRANSFORM_PROPS = ['x', 'y', 'rotation', 'scale', 'scaleX', 'scaleY'] as const;

/**
 * `sourceChain` plus what the editor needs to write a transform back into
 * the source: the frame local to the layer's element, the transform props
 * its innermost primitive received, how many layers that element produced,
 * and the timing windows it is shown in. `layerNodes` is the frame's layer
 * id -> node map.
 */
export function layerSourceReport(
  node: HostNode,
  layerNodes: ReadonlyMap<string, HostNode>,
  resolveFile: (fileName: string) => string,
): LayerSourceReport {
  const ids = new Map<HostNode, string>();
  for (const [id, hostNode] of layerNodes) if (!ids.has(hostNode)) ids.set(hostNode, id);
  const start = (node.fiber as Fiber | null | undefined) ?? null;
  const committed = committedFibers(start);
  const up: Up = (fiber) => (fiber.return ? committed(fiber.return) : null);
  const found = sourceFibers(node, resolveFile, (hostNode) => ids.get(hostNode), committed);
  const chain = found.map(({ entry }) => entry);
  if (chain.length === 0) return { chain };

  const primitive = found.find(({ entry }) => entry.kind === 'primitive');
  const element = primitive?.fiber ?? (start ? committed(start) : null);

  const props: ReceivedProps = {};
  const received = (primitive?.fiber.memoizedProps ?? primitive?.fiber.pendingProps ?? {}) as Record<string, unknown>;
  for (const key of TRANSFORM_PROPS) {
    const value = received[key];
    if (typeof value === 'number' && Number.isFinite(value)) props[key] = value;
  }

  let renderCount = 0;
  const innermost = chain[0];
  const seen = new Set<HostNode>();
  for (const other of layerNodes.values()) {
    if (seen.has(other)) continue;
    seen.add(other);
    const first = sourceFibers(other, resolveFile, undefined, committed)[0]?.entry;
    if (first && first.file === innermost.file && first.line === innermost.line && first.column === innermost.column) {
      renderCount++;
    }
  }
  return {
    chain,
    localFrame: ownerFrame(element, up),
    candidateFrames: framesAbove(element, up),
    props,
    renderCount,
    ...timingWindows(element, up, resolveFile),
  };
}

/**
 * The element and instance keys of every distinct node in `layerNodes`: the
 * positions of its source chain other than `'wrapper'` entries, innermost
 * first, and the first of them. A node whose chain has no such entry is left
 * out.
 */
export function occurrenceKeys(
  layerNodes: ReadonlyMap<string, HostNode>,
  resolveFile: (fileName: string) => string,
): Array<{ element: SourcePos; chain: SourcePos[] }> {
  const nodes = new Set(layerNodes.values());
  const [first] = nodes;
  if (!first) return [];
  const committed = committedFibers((first.fiber as Fiber | null | undefined) ?? null);
  const keys: Array<{ element: SourcePos; chain: SourcePos[] }> = [];
  for (const node of nodes) {
    const chain = sourceFibers(node, resolveFile, undefined, committed)
      .filter(({ entry }) => entry.kind !== 'wrapper')
      .map(({ entry: { file, line, column } }) => ({ file, line, column }));
    if (chain.length > 0) keys.push({ element: chain[0], chain });
  }
  return keys;
}

type RuntimeValue = {
  time: { value: number; timescale: number };
  fps: number;
  frameOffset?: number;
  durationInFrames: number;
};

/** The value a `CompositionRuntimeContext` provider fiber hands down, if it is one. */
function providedValue(fiber: Fiber): RuntimeValue | undefined {
  // React 19 makes the context its own provider; 18 wraps it.
  const type = fiber.type as { _context?: unknown } | null;
  if (!type || typeof type !== 'object') return undefined;
  if (type !== (CompositionRuntimeContext as unknown) && type._context !== CompositionRuntimeContext) return undefined;
  return (fiber.memoizedProps as { value?: RuntimeValue | null } | null)?.value ?? undefined;
}

/** The frame a `CompositionRuntimeContext` provider fiber hands down, if it is one. */
function providedFrame(fiber: Fiber): number | undefined {
  const value = providedValue(fiber);
  return value ? runtimeFrame(value.time, value.fps, value.frameOffset) : undefined;
}

/**
 * The timing wrappers above `element`, nearest first, and the composition
 * frames all of them show it in. Each window is placed where its wrapper
 * gates its children; inside a `<Loop>` or `<FreezeFrame>` the clock no
 * longer follows the composition's, so a window there takes the one around it.
 */
function timingWindows(
  element: Fiber | null,
  up: Up,
  resolveFile: (fileName: string) => string,
): { windows: TimingWindow[]; span: { from: number; to: number } } {
  // The way from the element to the root, nearest first.
  const path: Fiber[] = [];
  for (let fiber = element ? up(element) : null; fiber; fiber = up(fiber)) path.push(fiber);
  let root: RuntimeValue | undefined;
  for (const fiber of path) root = providedValue(fiber) ?? root;
  const total = root?.durationInFrames ?? 0;

  const windows: TimingWindow[] = [];
  let enclosing = { from: 0, to: total };
  let remapped = false;
  for (let index = path.length - 1; index >= 0; index--) {
    const fiber = path[index];
    const kind = windowKind(fiber);
    if (kind === undefined || kind === 'LoopCycle') continue;
    // `<Loop>` draws its window and each cycle with `<Sequence>`s of its own.
    const parent = path[index + 1];
    const parentKind = parent ? windowKind(parent) : undefined;
    if (kind === 'Sequence' && (parentKind === 'Loop' || parentKind === 'LoopCycle')) continue;

    const props = (fiber.memoizedProps ?? fiber.pendingProps) as { from?: unknown } | null;
    let { from, to } = enclosing;
    if (!remapped && kind !== 'FreezeFrame') {
      // The window `<Sequence>` gates its children by: its own `from`, read
      // as it does, against the window above, and the length it hands down
      // (a `<Loop>`'s is its outer `<Sequence>`'s). Not the frame it hands
      // down, which a `<MotionBlur>` moment shifts by a fraction.
      let value: RuntimeValue | undefined;
      for (let below = index - 1; below >= 0 && !value; below--) value = providedValue(path[below]);
      if (value) {
        const own = props?.from;
        from = enclosing.from + (typeof own === 'number' && Number.isFinite(own) ? own : 0);
        to = from + value.durationInFrames;
      }
    }
    const window: TimingWindow = { kind, from, to };
    const source = sourceOfProps(props);
    if (source) window.source = { file: resolveFile(source.fileName), line: source.lineNumber, column: source.columnNumber };
    if (remapped || kind !== 'Sequence') window.remapped = true;
    windows.push(window);
    if (!remapped) enclosing = { from, to };
    if (kind !== 'Sequence') remapped = true;
  }

  const span = { from: 0, to: total };
  for (const window of windows) {
    span.from = Math.max(span.from, window.from);
    span.to = Math.min(span.to, window.to);
  }
  return { windows: windows.reverse(), span };
}

/** The frame `useCurrentFrame()` reports in a component at `fiber`. */
function frameAt(fiber: Fiber | null, up: Up): number | undefined {
  for (let above = fiber ? up(fiber) : null; above; above = up(above)) {
    const frame = providedFrame(above);
    if (frame !== undefined) return frame;
  }
  return undefined;
}

/** A function component (or `memo`/`forwardRef` of one) the author wrote. */
function isAuthorComponent(fiber: Fiber): boolean {
  const type = fiber.elementType ?? fiber.type;
  const target = unwrap(type);
  if (target === null || typeof target !== 'function') return false;
  return !primitives.has(target) && !transparent.has(target);
}

/**
 * Whether `props` holds the element whose props object is `target`: as a
 * prop, in `children`, or anywhere inside elements and arrays passed down.
 */
function passesDown(props: unknown, target: object): boolean {
  const seen = new Set<object>();
  const stack: unknown[] = [props];
  while (stack.length > 0 && seen.size < 10_000) {
    const value = stack.pop();
    if (typeof value !== 'object' || value === null || seen.has(value)) continue;
    if (value === target) return true;
    seen.add(value);
    if (Array.isArray(value)) {
      stack.push(...value);
    } else if ('$$typeof' in value) {
      stack.push((value as { props?: unknown }).props);
    } else if (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null) {
      stack.push(...Object.values(value));
    }
  }
  return false;
}

/** The distinct frames the providers above `fiber` hand down, nearest first. */
function framesAbove(fiber: Fiber | null, up: Up): number[] {
  const frames: number[] = [];
  for (let above = fiber ? up(fiber) : null; above; above = up(above)) {
    const frame = providedFrame(above);
    if (frame !== undefined && !frames.includes(frame)) frames.push(frame);
  }
  return frames;
}

/**
 * The frame the component that wrote `primitive`'s JSX (its owner) reads.
 * React keeps no owner outside development builds, so the owner is the
 * nearest component above the element that the author wrote, did not mark
 * as drawing its caller's JSX, and did not receive the element through its
 * props. A component may also call a function its caller wrote (a render
 * prop, a function from context, ...), so this is only a guess; the editor
 * checks `candidateFrames` instead of trusting it.
 */
function ownerFrame(primitive: Fiber | null, up: Up): number {
  const atElement = frameAt(primitive, up) ?? 0;
  if (!primitive) return atElement;
  const target = (primitive.memoizedProps ?? primitive.pendingProps) as object | null;
  for (let fiber = up(primitive); fiber; fiber = up(fiber)) {
    if (!isAuthorComponent(fiber)) continue;
    const props = fiber.memoizedProps ?? fiber.pendingProps;
    if (target && passesDown(props, target)) continue;
    // Outside every provider (an entry's root, say), no frame can be read.
    return frameAt(fiber, up) ?? atElement;
  }
  return atElement;
}
