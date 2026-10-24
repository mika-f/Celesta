// Named scenes (chapters) laid out back to back. `defineScenes` places them
// once; `<Scenes>` plays them and `useScene` / `sceneAt` say which one is
// current, from inside a scene or from a HUD next to it.

import * as React from 'react';
import type { ComponentType } from 'react';

import { Sequence } from './components';
import { CompositionRuntimeContext, RootRuntimeContext, runtimeFrame, useCurrentFrame } from './hooks';
import { markTransparent } from './source-chain';

export interface SceneEntry {
  /** Unique key of the scene, for code and for the preview's chapter list. */
  id: string;
  /** Label to show (a HUD, a chapter list). Defaults to `id`. */
  name?: string;
  /** Length in frames. 0 keeps the scene in the list but never plays it. */
  durationInFrames: number;
  /**
   * Frames to shift this scene from the end of the previous one, like
   * `<Series.Sequence offset>`: negative overlaps, positive leaves a gap.
   */
  offset?: number;
  /** What `<Scenes>` draws for it. Inside, frames restart at 0. */
  Scene?: ComponentType;
}

/** A scene with its place on the timeline. */
export type PlacedScene<T extends SceneEntry = SceneEntry> = T & {
  index: number;
  name: string;
  /** First frame, in the frames of the timeline `<Scenes>` is placed on. */
  from: number;
};

export interface SceneList<T extends SceneEntry = SceneEntry> {
  scenes: readonly PlacedScene<T>[];
  /** The frame the last scene ends on: the total length. */
  durationInFrames: number;
}

export interface SceneState<T extends SceneEntry = SceneEntry> {
  scene: PlacedScene<T>;
  index: number;
  id: string;
  name: string;
  from: number;
  /** The scene's own length. */
  durationInFrames: number;
  /** Frames since the scene started (0 on its first frame). */
  frame: number;
  /** `frame / durationInFrames`, held at 1 in a gap after the scene. */
  progress: number;
  previous: PlacedScene<T> | undefined;
  next: PlacedScene<T> | undefined;
  /** Every scene, in order, including ones of length 0. */
  scenes: readonly PlacedScene<T>[];
  /** Length of the whole list. */
  totalDurationInFrames: number;
}

/**
 * Places named scenes back to back, the way `<Series>` places its items.
 * Pass the result to `<Scenes>` to play it, to `useScene` / `sceneAt` to know
 * where you are, and its `durationInFrames` to the `<Composition>`.
 */
export function defineScenes<T extends SceneEntry>(entries: readonly T[]): SceneList<T> {
  const ids = new Set<string>();
  let at = 0;
  let end = 0;
  const scenes = entries.map((entry, index) => {
    const { id, durationInFrames, offset = 0 } = entry;
    if (typeof id !== 'string' || id === '') {
      throw new Error(`scene ${index} requires a non-empty string id`);
    }
    if (ids.has(id)) {
      throw new Error(`scene id "${id}" is used more than once`);
    }
    ids.add(id);
    if (!Number.isInteger(durationInFrames) || durationInFrames < 0) {
      throw new Error(`scene "${id}" requires a non-negative integer durationInFrames`);
    }
    if (!Number.isInteger(offset)) {
      throw new Error(`scene "${id}" requires an integer offset`);
    }
    at += offset;
    if (at < 0) {
      throw new Error(`scene "${id}" would start before frame 0; reduce its negative offset`);
    }
    const from = at;
    at += durationInFrames;
    end = Math.max(end, at);
    return { ...entry, index, name: entry.name ?? id, from };
  });
  return { scenes, durationInFrames: end };
}

/**
 * The scene current at `frame` (in the frames of the timeline the list is
 * played on): the latest-starting scene that has started, the later one when
 * two overlap. In a gap the previous scene stays current. `null` before the
 * first scene and from the end of the list on. Scenes of length 0 are never
 * current.
 */
export function sceneAt<T extends SceneEntry>(list: SceneList<T>, frame: number): SceneState<T> | null {
  if (frame >= list.durationInFrames) {
    return null;
  }
  let current: PlacedScene<T> | undefined;
  for (const scene of list.scenes) {
    if (scene.durationInFrames > 0 && scene.from <= frame && (!current || scene.from >= current.from)) {
      current = scene;
    }
  }
  if (!current) {
    return null;
  }
  const local = frame - current.from;
  return {
    scene: current,
    index: current.index,
    id: current.id,
    name: current.name,
    from: current.from,
    durationInFrames: current.durationInFrames,
    frame: local,
    progress: Math.min(1, local / current.durationInFrames),
    previous: list.scenes[current.index - 1],
    next: list.scenes[current.index + 1],
    scenes: list.scenes,
    totalDurationInFrames: list.durationInFrames,
  };
}

interface ScenesScope {
  list: SceneList<any>;
  /** Root frame minus the frame `<Scenes>` was placed at. */
  base: number;
  parent: ScenesScope | null;
}

const ScenesContext = React.createContext<ScenesScope | null>(null);

function useRootFrame(hookName: string): number {
  const root = React.useContext(RootRuntimeContext) ?? React.useContext(CompositionRuntimeContext);
  if (!root) {
    throw new Error(`${hookName} must be called from within a Celesta <Composition>`);
  }
  return runtimeFrame(root.time, root.fps);
}

export interface ScenesProps<T extends SceneEntry = SceneEntry> {
  scenes: SceneList<T>;
}

/**
 * Plays a `defineScenes` list: each scene's `Scene` in a `<Sequence>` at its
 * place. Inside, `useScene()` reports the current scene whatever
 * `<Sequence>`s enclose the call.
 *
 * ```tsx
 * const scenes = defineScenes([
 *   { id: 'intro', name: 'Cold open', durationInFrames: 90, Scene: Intro },
 *   { id: 'body', name: 'Manifesto', durationInFrames: 240, Scene: Body },
 * ]);
 * <Scenes scenes={scenes} />
 * <Hud scenes={scenes} />
 * ```
 */
export function Scenes<T extends SceneEntry>({ scenes }: ScenesProps<T>): ReturnType<typeof React.createElement> {
  const parent = React.useContext(ScenesContext);
  const base = useRootFrame('<Scenes>') - useCurrentFrame();
  const scope = React.useMemo(() => ({ list: scenes, base, parent }), [scenes, base, parent]);
  return React.createElement(
    ScenesContext.Provider,
    { value: scope },
    scenes.scenes.map(({ id, from, durationInFrames, Scene }) =>
      durationInFrames > 0 && Scene
        ? React.createElement(Sequence, { key: id, from, durationInFrames }, React.createElement(Scene))
        : null,
    ),
  );
}

export interface UseSceneOptions {
  /**
   * The clock a list that no `<Scenes>` above plays is read on: the
   * caller's frame (`'caller'`, the default) or the composition's
   * (`'root'`), which `<Sequence>`s do not shift. Inside a `<Scenes>` that
   * plays the list, its clock is used either way.
   */
  clock?: 'caller' | 'root';
}

/**
 * The current scene of a `defineScenes` list, or `null` outside every scene
 * (see `sceneAt`). With no argument, the list of the nearest `<Scenes>`
 * above. Inside a `<Scenes>`, its own list is read on its clock, so nested
 * `<Sequence>`s do not shift it; elsewhere (a HUD beside the `<Scenes>`) the
 * list is read on the caller's clock, so place the caller on the same
 * timeline as the `<Scenes>`. With `{ clock: 'root' }`, it is read on the
 * composition's frame there instead, for a caller inside a `<Sequence>`
 * whose list is laid out on the whole video (a chapter of a
 * `<TransitionSeries>`, say).
 *
 * Inside a `<MotionBlur>` sample or a `<Trail>` copy within a scene, the
 * result is the one of the frame being drawn, not of the sample.
 *
 * ```tsx
 * const scene = useScene(scenes);
 * if (!scene) return null;
 * <Text>{scene.name}</Text>
 * <Rect width={1920 * scene.progress} height={4} />
 *
 * // Inside <Sequence from={90}>: the video's chapter, not the one at frame 0.
 * const chapter = useScene(scenes, { clock: 'root' });
 * ```
 */
export function useScene<T extends SceneEntry>(list?: SceneList<T>, options?: UseSceneOptions): SceneState<T> | null {
  const clock = options?.clock ?? 'caller';
  if (clock !== 'caller' && clock !== 'root') {
    throw new Error("useScene() requires `clock` to be 'caller' or 'root'");
  }
  let scope = React.useContext(ScenesContext);
  const rootFrame = useRootFrame('useScene()');
  const localFrame = useCurrentFrame();
  while (scope && list !== undefined && scope.list !== list) {
    scope = scope.parent;
  }
  if (scope) {
    return sceneAt(scope.list as SceneList<T>, rootFrame - scope.base);
  }
  if (list === undefined) {
    throw new Error('useScene() without a list must be called inside <Scenes>');
  }
  return sceneAt(list, clock === 'root' ? rootFrame : localFrame);
}

// Draws the `Scene` components its caller lists, each in a `<Sequence>`.
markTransparent(Scenes);
