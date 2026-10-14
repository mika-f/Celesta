import type { GroupProps } from '@celesta/react';

/** The edge the entering scene comes from, for the transitions that move or turn one way. */
export type TransitionSeriesEdge = 'left' | 'right' | 'top' | 'bottom';

/** What a transition's `present` function is given, on every frame of the overlap. */
export interface TransitionPresentInput {
  /**
   * How far the transition has got, after the `<TransitionSeries.Transition>`'s
   * `easing`: from 1/(n+1) on the first of its n frames to n/(n+1) on the
   * last, so every overlapped frame shows both scenes. An easing that
   * overshoots can take it below 0 or above 1.
   */
  progress: number;
  /** The frame of the overlap, from 0. */
  frame: number;
  /** Frames the two scenes overlap. */
  durationInFrames: number;
  /** The `<TransitionSeries.Transition>`'s `from`; `'left'` when it has none. */
  from: TransitionSeriesEdge;
  /** The series' width: its `width` prop, or the layout bounds (see `useLayoutBounds()`). */
  width: number;
  /** The series' height, as `width`. */
  height: number;
}

/**
 * How one scene is drawn on a frame of a transition: props for the `Group`
 * that holds it, such as `x`, `scale`, `opacity`, `clip`, `blur`, `shader`,
 * or `mask`. The group sits at the series' origin, in the series' own
 * coordinates, so `{ x: 0, y: 0, width, height }` of the input is the area
 * the series covers.
 */
export type TransitionSceneStyle = Omit<GroupProps, 'children' | 'id' | 'lang'>;

/** Both scenes' styles on a frame. A side left out is drawn as it is. */
export interface TransitionPresentation {
  /** The scene coming in, drawn above the one going out. */
  enter?: TransitionSceneStyle;
  /** The scene going out. */
  exit?: TransitionSceneStyle;
}

/** A transition `<TransitionSeries.Transition type>` takes, as `defineTransition()` makes it. */
export interface TransitionDefinition {
  /** Names the transition in `useTransitionSeriesScene()` and in error messages. */
  readonly name: string;
  /** Both scenes' styles on one frame of the overlap. */
  readonly present: (input: TransitionPresentInput) => TransitionPresentation;
}

/**
 * Defines a transition for `<TransitionSeries.Transition type>`: a name, and
 * a function from the frame's progress, the direction, and the series' size
 * to how each scene is drawn. Every built-in transition except `'cut'` is
 * one of these.
 *
 * ```tsx
 * const drift = defineTransition({
 *   name: 'drift',
 *   present: ({ progress, width }) => ({
 *     enter: { x: (1 - progress) * width * 0.2, opacity: progress },
 *     exit: { blur: progress * 16 },
 *   }),
 * });
 * <TransitionSeries.Transition type={drift} durationInFrames={20} />
 * ```
 */
export function defineTransition(definition: {
  name: string;
  present: (input: TransitionPresentInput) => TransitionPresentation;
}): TransitionDefinition {
  if (definition === null || typeof definition !== 'object') {
    throw new Error('defineTransition expects an object with `name` and `present`');
  }
  const { name, present } = definition;
  if (typeof name !== 'string' || name.length === 0) {
    throw new Error('defineTransition: name must be a non-empty string');
  }
  if (name === 'cut') {
    throw new Error('defineTransition: "cut" is the name of the transition that takes no frames');
  }
  if (typeof present !== 'function') {
    throw new Error(`defineTransition: ${name}'s present must be a function`);
  }
  return Object.freeze({ name, present });
}

/** Whether `value` is a transition definition: an object with a string `name` and a `present` function. */
export function isTransitionDefinition(value: unknown): value is TransitionDefinition {
  return (
    value !== null &&
    typeof value === 'object' &&
    typeof (value as TransitionDefinition).name === 'string' &&
    typeof (value as TransitionDefinition).present === 'function'
  );
}
