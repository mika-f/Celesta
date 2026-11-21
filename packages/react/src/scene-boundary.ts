// Per-scene error isolation. `<Scenes>`, `<Series>`, and `<TransitionSeries>`
// wrap each scene in a `<SceneBoundary>`, a React error boundary: in the
// editor preview a scene that throws while it renders draws an error card in
// its place and the other scenes still play; everywhere else (exports,
// inspect.mjs, the MCP tools) the error fails the frame, naming the scene.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Rect, Text } from './components';
import { CompositionRuntimeContext } from './hooks';
import { markQuietBoundary } from './reconciler';
import { markTransparent } from './source-chain';

/** A scene that threw while it rendered, as a preview frame reports it. */
export interface SceneErrorReport {
  /** The scene's id: a `defineScenes` id, or the `id` of a series item. */
  id: string;
  /** The scene's label. */
  name: string;
  /** The error's message. */
  message: string;
  /** The error's stack, when it has one. */
  stack?: string;
  /** The components between the scene and the throw, innermost first, as React reports them. */
  componentStack?: string;
}

/**
 * @internal Collects the scene errors of one render pass. render.ts provides
 * a new one for every pass, which also tells a `<SceneBoundary>` that a new
 * frame began: what it caught before is forgotten, since frames can be
 * requested in any order.
 */
export interface SceneErrorSink {
  /**
   * Whether a scene that throws draws its card: in the preview's frames
   * (`useIsPreview()`), and in the passes that mount the entry, whose
   * scene is discarded. Otherwise the frame fails.
   */
  catches: boolean;
  report(error: SceneErrorReport): void;
}

/** @internal */
export const SceneErrorSinkContext = React.createContext<SceneErrorSink | null>(null);

/**
 * The error a frame fails with outside the preview when a scene threw: its
 * message names the scene, and `cause` is what the scene threw.
 */
export class SceneRenderError extends Error {
  readonly sceneId: string;
  readonly sceneName: string;
  /** What the scene threw. */
  readonly cause: unknown;

  constructor(sceneId: string, sceneName: string, cause: unknown) {
    const label = sceneName === sceneId ? `"${sceneId}"` : `"${sceneId}" (${sceneName})`;
    super(`scene ${label} threw while rendering: ${messageOf(cause)}`);
    this.name = 'SceneRenderError';
    this.cause = cause;
    this.sceneId = sceneId;
    this.sceneName = sceneName;
    const stack = stackOf(cause);
    if (stack !== undefined) this.stack = `${this.name}: ${this.message}\n${stackFrames(stack)}`;
  }
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function stackOf(error: unknown): string | undefined {
  return error instanceof Error && typeof error.stack === 'string' ? error.stack : undefined;
}

/** A stack's frames: the lines after its message. */
function stackFrames(stack: string): string {
  return stack.split('\n').filter((line) => /^\s+at /.test(line)).join('\n');
}

/** A stack up to the frames of React's own work loop, which are the same for every error. */
function withoutReact(stack: string): string {
  const lines = stack.split('\n');
  const end = lines.findIndex((line) => /react_stack_bottom_frame|[\\/]react-reconciler[\\/]/.test(line));
  return (end === -1 ? lines : lines.slice(0, end)).join('\n');
}

/** The components of a React component stack inside the scene, innermost first. */
function sceneComponents(componentStack: string): string {
  const lines = componentStack.replace(/^\n/, '').split('\n');
  const end = lines.findIndex((line) => /^\s*at Boundary\b/.test(line));
  return (end === -1 ? lines : lines.slice(0, end)).join('\n');
}

export interface SceneBoundaryProps {
  /** The scene's id, reported with its error. */
  id: string;
  /** The scene's label on the error card and in Problems. Defaults to `id`. */
  name?: string;
  children?: ReactNode;
}

interface BoundaryProps {
  sceneId: string;
  name: string;
  sink: SceneErrorSink | null;
  children?: ReactNode;
}

interface BoundaryState {
  /** What the children threw in this pass; `caught` tells a thrown `undefined` apart. */
  error: unknown;
  caught: boolean;
  /** The sink of the pass `error` was caught in. */
  sink: SceneErrorSink | null;
}

class Boundary extends React.Component<BoundaryProps, BoundaryState> {
  constructor(props: BoundaryProps) {
    super(props);
    this.state = { error: undefined, caught: false, sink: props.sink };
  }

  static getDerivedStateFromError(error: unknown): Partial<BoundaryState> {
    return { error, caught: true };
  }

  // A new pass renders the children again: an error is never carried into
  // another frame.
  static getDerivedStateFromProps(props: BoundaryProps, state: BoundaryState): Partial<BoundaryState> | null {
    return props.sink === state.sink ? null : { error: undefined, caught: false, sink: props.sink };
  }

  componentDidCatch(error: unknown, info: React.ErrorInfo): void {
    // Only the preview catches; elsewhere the card below throws instead.
    const stack = stackOf(error);
    const componentStack = info.componentStack ? sceneComponents(info.componentStack) : '';
    this.props.sink?.report({
      id: this.props.sceneId,
      name: this.props.name,
      message: messageOf(error),
      ...(stack !== undefined ? { stack: withoutReact(stack) } : {}),
      ...(componentStack !== '' ? { componentStack } : {}),
    });
  }

  render(): ReactNode {
    if (!this.state.caught) return this.props.children;
    const { sceneId, name } = this.props;
    if (this.props.sink?.catches !== true) {
      // Never draw the card into an export: fail the frame, naming the scene.
      if (this.state.error instanceof SceneRenderError) throw this.state.error;
      throw new SceneRenderError(sceneId, name, this.state.error);
    }
    return React.createElement(SceneErrorCard, { id: sceneId, name, message: messageOf(this.state.error) });
  }
}
markQuietBoundary(Boundary);

/**
 * Isolates a scene: in the editor preview, an error its children throw while
 * rendering draws an error card (the scene's name and id, and the message)
 * in its place, and is listed in Problems with its stack; the rest of the
 * film still plays. Outside the preview (exports, `inspect.mjs`, the MCP
 * tools) the frame fails with a `SceneRenderError` that names the scene, so
 * no video ever contains the card. Each frame renders the children again:
 * a scene that throws on some frames draws normally on the others.
 *
 * `<Scenes>`, `<Series>`, and `<TransitionSeries>` already wrap each scene in
 * one; wrap a `<Sequence>` of your own to treat it as a scene.
 *
 * Only errors thrown while React renders are caught. One the scene walker
 * finds afterwards, such as an invalid prop value, still fails the frame.
 *
 * ```tsx
 * <Sequence from={90} durationInFrames={120}>
 *   <SceneBoundary id="chart" name="Sales chart"><Chart /></SceneBoundary>
 * </Sequence>
 * ```
 */
export function SceneBoundary({ id, name, children }: SceneBoundaryProps): React.ReactElement {
  if (typeof id !== 'string' || id === '') {
    throw new Error('<SceneBoundary> requires a non-empty string `id`');
  }
  const sink = React.useContext(SceneErrorSinkContext);
  return React.createElement(Boundary, { sceneId: id, name: name ?? id, sink }, children);
}
markTransparent(SceneBoundary);

const CARD_BACKGROUND = '#17191d';
const CARD_ACCENT = '#e5484d';
const CARD_TEXT = '#f2f3f5';
const CARD_MUTED = '#9aa0a8';
/** The longest message the card draws; Problems has the whole error. */
const CARD_MESSAGE_LIMIT = 400;

/** The card a failed scene draws in the preview. */
function SceneErrorCard({ id, name, message }: { id: string; name: string; message: string }): React.ReactElement {
  const runtime = React.useContext(CompositionRuntimeContext);
  const width = runtime?.width ?? 1920;
  const height = runtime?.height ?? 1080;
  const unit = Math.min(width, height) / 1080;
  const margin = Math.round(96 * unit);
  const text = (size: number, color: string, weight = 400) => ({
    fontFamily: 'sans-serif',
    fontSize: Math.round(size * unit),
    fontWeight: weight,
    color,
  });
  const shown = message.length > CARD_MESSAGE_LIMIT ? `${message.slice(0, CARD_MESSAGE_LIMIT - 1)}…` : message;
  return React.createElement(
    Group,
    null,
    React.createElement(Rect, { width, height, fill: CARD_BACKGROUND }),
    React.createElement(Rect, { width: Math.max(4, Math.round(12 * unit)), height, fill: CARD_ACCENT }),
    React.createElement(Text, { x: margin, y: margin, ...text(32, CARD_ACCENT, 700), children: 'Scene failed to render' }),
    React.createElement(Text, {
      x: margin,
      y: margin + Math.round(64 * unit),
      maxWidth: width - 2 * margin,
      ...text(72, CARD_TEXT, 700),
      children: name,
    }),
    React.createElement(Text, { x: margin, y: margin + Math.round(168 * unit), ...text(32, CARD_MUTED), children: `id: ${id}` }),
    React.createElement(Text, {
      x: margin,
      y: margin + Math.round(256 * unit),
      maxWidth: width - 2 * margin,
      ...text(36, CARD_TEXT),
      children: shown,
    }),
    React.createElement(Text, {
      x: margin,
      y: height - margin,
      anchorY: 1,
      ...text(28, CARD_MUTED),
      children: 'The error and its stack are in Problems. Exports fail on this scene.',
    }),
  );
}
