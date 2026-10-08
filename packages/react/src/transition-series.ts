import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, Sequence } from './components';
import type { AnimatedNumber, CommonProps, GroupProps } from './components';
import { useCurrentFrame, useVideoConfig } from './hooks';
import { frameKeyframes } from './keyframes';

export type TransitionSeriesType = 'cut' | 'crossfade' | 'slide' | 'wipe';
export type TransitionSeriesEdge = 'left' | 'right' | 'top' | 'bottom';

export interface TransitionSeriesSceneItem {
  /** Length in frames, including the frames shared with its neighbours. */
  durationInFrames: number;
  type?: undefined;
}

export interface TransitionSeriesTransitionItem {
  type: TransitionSeriesType;
  /**
   * Frames the scenes on either side share. A `'cut'` takes 0 (the default
   * for a cut); every other type needs a positive integer.
   */
  durationInFrames?: number;
}

export type TransitionSeriesItem = TransitionSeriesSceneItem | TransitionSeriesTransitionItem;

export interface TransitionSeriesTiming {
  /** Where each scene starts and how long it lasts, in the series' frames. */
  sequences: { from: number; durationInFrames: number }[];
  /** Where each transition item starts and how long the two scenes overlap. */
  transitions: { from: number; durationInFrames: number }[];
  /** The frame the last scene ends on: the series' total length. */
  durationInFrames: number;
}

const TYPES: readonly TransitionSeriesType[] = ['cut', 'crossfade', 'slide', 'wipe'];
const EDGES: readonly TransitionSeriesEdge[] = ['left', 'right', 'top', 'bottom'];

function isTransition(item: TransitionSeriesItem): item is TransitionSeriesTransitionItem {
  return item.type !== undefined;
}

function transitionFrames(item: TransitionSeriesTransitionItem, index: number): number {
  if (!TYPES.includes(item.type)) {
    throw new Error(
      `transition series item ${index} has unknown type ${JSON.stringify(item.type)}; expected ${TYPES.join(', ')}`,
    );
  }
  const { durationInFrames } = item;
  if (item.type === 'cut') {
    if (durationInFrames !== undefined && durationInFrames !== 0) {
      throw new Error(`transition series item ${index} is a cut, which takes no frames; omit durationInFrames`);
    }
    return 0;
  }
  if (!Number.isInteger(durationInFrames) || (durationInFrames as number) <= 0) {
    throw new Error(`transition series item ${index} (${item.type}) requires a positive integer durationInFrames`);
  }
  return durationInFrames as number;
}

/**
 * Lays scenes out the way `<TransitionSeries>` does, without rendering
 * anything. Scenes play back to back; a transition between two scenes
 * overlaps them by its `durationInFrames`, so the series is that much
 * shorter than the scenes added up. Use it to size the `<Composition>`.
 *
 * Throws when the list is empty, starts or ends with a transition, has two
 * transitions in a row, or a scene is shorter than the transitions into and
 * out of it together (three scenes would then show at once).
 */
export function computeTransitionSeries(items: readonly TransitionSeriesItem[]): TransitionSeriesTiming {
  const sequences: TransitionSeriesTiming['sequences'] = [];
  const transitions: TransitionSeriesTiming['transitions'] = [];
  // Frames the next scene shares with the previous one, and the previous
  // scene's own transition in, to check it can hold both.
  let pending: number | null = null;
  let previousEnter = 0;
  let end = 0;
  items.forEach((item, index) => {
    if (isTransition(item)) {
      const frames = transitionFrames(item, index);
      if (sequences.length === 0) {
        throw new Error(`transition series item ${index} is a transition before any scene; start with a scene`);
      }
      if (pending !== null) {
        throw new Error(`transition series item ${index} follows another transition; put a scene between them`);
      }
      const previous = sequences[sequences.length - 1];
      if (previousEnter + frames > previous.durationInFrames) {
        throw new Error(
          `transition series scene ${sequences.length - 1} lasts ${previous.durationInFrames} frames, ` +
            `too short for its transitions in (${previousEnter}) and out (${frames}); lengthen it or shorten them`,
        );
      }
      transitions.push({ from: end - frames, durationInFrames: frames });
      pending = frames;
      return;
    }
    const { durationInFrames } = item;
    if (!Number.isInteger(durationInFrames) || durationInFrames <= 0) {
      throw new Error(`transition series item ${index} requires a positive integer durationInFrames`);
    }
    const enter = pending ?? 0;
    if (enter > durationInFrames) {
      throw new Error(
        `transition series scene ${sequences.length} lasts ${durationInFrames} frames, ` +
          `shorter than its transition in (${enter}); lengthen it or shorten the transition`,
      );
    }
    const from = end - enter;
    sequences.push({ from, durationInFrames });
    end = from + durationInFrames;
    previousEnter = enter;
    pending = null;
  });
  if (sequences.length === 0) {
    throw new Error('a transition series requires at least one scene');
  }
  if (pending !== null) {
    throw new Error('a transition series cannot end with a transition; add a scene after it');
  }
  return { sequences, transitions, durationInFrames: end };
}

export interface TransitionSeriesSequenceProps extends CommonProps {
  /** Length in frames, including the frames shared with its neighbours. */
  durationInFrames: number;
  children?: ReactNode;
}

export interface TransitionSeriesTransitionProps {
  /**
   * `'cut'` (no overlap), `'crossfade'` (the next scene fades in over this
   * one), `'slide'` (the next scene pushes this one out), or `'wipe'` (the
   * next scene is revealed behind a moving straight edge).
   */
  type: TransitionSeriesType;
  /** Frames the two scenes overlap. Positive; omit it for a cut. */
  durationInFrames?: number;
  /** Edge the next scene enters from, for `'slide'` and `'wipe'`. Defaults to `'left'`. */
  from?: TransitionSeriesEdge;
  /** Shapes the transition's 0–1 progress, for example `Easings.easeInOutCubic`. Defaults to linear. */
  easing?: (progress: number) => number;
}

/** One scene of a `<TransitionSeries>`. Only valid as a direct child of it. */
function TransitionSeriesSequence(_props: TransitionSeriesSequenceProps): never {
  throw new Error('<TransitionSeries.Sequence> must be a direct child of <TransitionSeries>');
}

/** A transition between the scenes on either side. Only valid as a direct child of `<TransitionSeries>`. */
function TransitionSeriesTransition(_props: TransitionSeriesTransitionProps): never {
  throw new Error('<TransitionSeries.Transition> must be a direct child of <TransitionSeries>');
}

export interface TransitionSeriesSceneTransition {
  type: TransitionSeriesType;
  /** Frames this scene shares with its neighbour on that side; 0 for a cut. */
  durationInFrames: number;
}

export interface TransitionSeriesScene {
  /** Position of this scene among the series' scenes, from 0. */
  index: number;
  /** The transition from the previous scene, or null for the first scene or a cut without a `<TransitionSeries.Transition>`. */
  enter: TransitionSeriesSceneTransition | null;
  /** The transition to the next scene, or null for the last scene or a cut without a `<TransitionSeries.Transition>`. */
  exit: TransitionSeriesSceneTransition | null;
}

const SceneContext = React.createContext<TransitionSeriesScene | null>(null);

/**
 * The enclosing `<TransitionSeries.Sequence>`'s place in its series and the
 * transitions on either side, for fading the scene's own audio or holding
 * back an animation until it is fully on screen. `useCurrentFrame()` already
 * counts from the scene's own start, the first frame of its transition in.
 */
export function useTransitionSeriesScene(): TransitionSeriesScene {
  const scene = React.useContext(SceneContext);
  if (!scene) {
    throw new Error('useTransitionSeriesScene() must be called inside a <TransitionSeries.Sequence>');
  }
  return scene;
}

/**
 * A volume for an `<Audio>` placed directly in a `<TransitionSeries.Sequence>`
 * (not in a further `<Sequence>` inside it): `volume` throughout, ramping
 * linearly up from 0 over the transition in and back to 0 over the
 * transition out, so two scenes' sounds cross-fade where their pictures do.
 * Without a transition on either side it is the plain `volume`.
 */
export function useTransitionVolume(volume = 1): AnimatedNumber {
  const { enter, exit } = useTransitionSeriesScene();
  const { fps, durationInFrames } = useVideoConfig();
  const fadeIn = enter?.durationInFrames ?? 0;
  const fadeOut = exit?.durationInFrames ?? 0;
  if (fadeIn === 0 && fadeOut === 0) {
    return volume;
  }
  const keys = [];
  if (fadeIn > 0) keys.push({ frame: 0, value: 0 }, { frame: fadeIn, value: volume });
  if (fadeOut > 0) keys.push({ frame: durationInFrames - fadeOut, value: volume }, { frame: durationInFrames, value: 0 });
  return frameKeyframes(keys, { fps });
}

interface SceneEdgeTransition extends TransitionSeriesSceneTransition {
  from: TransitionSeriesEdge;
  easing: (progress: number) => number;
}

/**
 * Group props for one side of a transition at `frame` frames into it. The
 * progress runs from 1/(n+1) on its first frame to n/(n+1) on its last, so
 * every overlapped frame shows both scenes and the frames either side show
 * one scene whole: counting those two frames (progress 0 and 1), the n + 1
 * steps between consecutive frames are all equal. The entering scene is
 * drawn above the leaving one.
 */
function present(
  transition: SceneEdgeTransition,
  frame: number,
  side: 'enter' | 'exit',
  width: number,
  height: number,
): GroupProps {
  const progress = transition.easing((frame + 1) / (transition.durationInFrames + 1));
  const sign = transition.from === 'left' || transition.from === 'top' ? -1 : 1;
  const horizontal = transition.from === 'left' || transition.from === 'right';
  switch (transition.type) {
    case 'crossfade':
      return side === 'enter' ? { opacity: Math.min(1, Math.max(0, progress)) } : {};
    case 'slide': {
      // At progress 0 the entering scene would sit one frame width (or
      // height) off the edge it comes from; the leaving one moves the same
      // distance the other way.
      const offset = side === 'enter' ? sign * (1 - progress) : -sign * progress;
      return horizontal ? { x: offset * width } : { y: offset * height };
    }
    case 'wipe': {
      if (side === 'exit') return {};
      // The strip from the far edge ends exactly on it: size - size * shown.
      const shown = Math.min(1, Math.max(0, progress));
      if (horizontal) {
        const strip = width * shown;
        return { clip: { x: sign < 0 ? 0 : width - strip, y: 0, width: strip, height } };
      }
      const strip = height * shown;
      return { clip: { x: 0, y: sign < 0 ? 0 : height - strip, width, height: strip } };
    }
    default:
      return {};
  }
}

function SceneFrame({
  enter,
  exit,
  durationInFrames,
  children,
}: {
  enter: SceneEdgeTransition | null;
  exit: SceneEdgeTransition | null;
  durationInFrames: number;
  children?: ReactNode;
}): ReturnType<typeof React.createElement> {
  const frame = useCurrentFrame();
  const { width, height } = useVideoConfig();
  const exitStart = durationInFrames - (exit?.durationInFrames ?? 0);
  // computeTransitionSeries keeps a scene's transitions in and out apart, so
  // at most one of them applies on any frame. The group is kept even when
  // idle so layer ids inside do not change as transitions start and end.
  const props =
    enter && frame < enter.durationInFrames
      ? present(enter, frame, 'enter', width, height)
      : exit && exit.durationInFrames > 0 && frame >= exitStart
        ? present(exit, frame - exitStart, 'exit', width, height)
        : {};
  return React.createElement(Group, props, children);
}

export interface TransitionSeriesProps {
  children?: ReactNode;
}

function TransitionSeriesRoot({ children }: TransitionSeriesProps): ReturnType<typeof React.createElement> {
  const elements: React.ReactElement<TransitionSeriesSequenceProps | TransitionSeriesTransitionProps>[] = [];
  React.Children.forEach(children, (child) => {
    if (child === null || child === undefined || typeof child === 'boolean') {
      return;
    }
    if (
      !React.isValidElement(child) ||
      (child.type !== TransitionSeriesSequence && child.type !== TransitionSeriesTransition)
    ) {
      throw new Error('<TransitionSeries> children must be <TransitionSeries.Sequence> or <TransitionSeries.Transition> elements');
    }
    elements.push(child as React.ReactElement<TransitionSeriesSequenceProps | TransitionSeriesTransitionProps>);
  });
  const items = elements.map((element): TransitionSeriesItem => {
    if (element.type === TransitionSeriesTransition) {
      const { type, durationInFrames } = element.props as TransitionSeriesTransitionProps;
      // A missing `type` must not read as a scene.
      return { type: type ?? ('' as TransitionSeriesType), durationInFrames };
    }
    return { durationInFrames: (element.props as TransitionSeriesSequenceProps).durationInFrames };
  });
  const { sequences } = computeTransitionSeries(items);

  // The transition element at `index`, if there is one: computeTransitionSeries
  // has already checked its type and length.
  const edgeAt = (index: number): SceneEdgeTransition | null => {
    const element = elements[index];
    if (element?.type !== TransitionSeriesTransition) {
      return null;
    }
    const { type, durationInFrames, from = 'left', easing = (progress: number) => progress } =
      element.props as TransitionSeriesTransitionProps;
    if (!EDGES.includes(from)) {
      throw new Error(`transition series item ${index} has unknown from ${JSON.stringify(from)}; expected ${EDGES.join(', ')}`);
    }
    return { type, durationInFrames: durationInFrames ?? 0, from, easing };
  };

  const scenes: React.ReactElement[] = [];
  elements.forEach((element, index) => {
    if (element.type === TransitionSeriesTransition) {
      return;
    }
    const sceneIndex = scenes.length;
    const enter = edgeAt(index - 1);
    const exit = edgeAt(index + 1);
    const { durationInFrames, children: inner, ...props } = element.props as TransitionSeriesSequenceProps;
    const scene: TransitionSeriesScene = {
      index: sceneIndex,
      enter: enter && { type: enter.type, durationInFrames: enter.durationInFrames },
      exit: exit && { type: exit.type, durationInFrames: exit.durationInFrames },
    };
    scenes.push(
      React.createElement(
        Sequence,
        { key: element.key ?? sceneIndex, ...props, ...sequences[sceneIndex] },
        React.createElement(
          SceneContext.Provider,
          { value: scene },
          React.createElement(SceneFrame, { enter, exit, durationInFrames }, inner),
        ),
      ),
    );
  });
  return React.createElement(React.Fragment, null, scenes);
}

/**
 * Plays scenes one after another with transitions between them, laid out
 * from the scenes' lengths. A `<TransitionSeries.Transition>` between two
 * `<TransitionSeries.Sequence>`s overlaps them by its `durationInFrames`:
 * the next scene starts that many frames before this one ends, and the
 * series gets that much shorter (see `computeTransitionSeries`). Scenes with
 * nothing between them cut.
 *
 * Each scene is an ordinary `<Sequence>`: its frames and media start at 0 on
 * the first frame of its transition in, and it is unmounted after its
 * transition out. Sounds of overlapping scenes play together at their own
 * volume; `useTransitionVolume()` fades a scene's `<Audio>` with its picture.
 *
 * ```tsx
 * <TransitionSeries>
 *   <TransitionSeries.Sequence durationInFrames={90}><Intro /></TransitionSeries.Sequence>
 *   <TransitionSeries.Transition type="crossfade" durationInFrames={15} />
 *   <TransitionSeries.Sequence durationInFrames={240}><Body /></TransitionSeries.Sequence>
 * </TransitionSeries>
 * ```
 */
export const TransitionSeries = Object.assign(TransitionSeriesRoot, {
  Sequence: TransitionSeriesSequence,
  Transition: TransitionSeriesTransition,
});
