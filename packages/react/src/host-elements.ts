// Host element types defined outside the core components. A package such as
// `@celesta/character` renders its own lowercase host elements and registers
// here how render.ts turns them into layers and audio, so the walker needs no
// knowledge of them. Registration happens when the package's module loads,
// before any element of that type can be rendered.

import type { HostNode } from './reconciler';
import type { Layer, LayerContent, ResolvedAsset } from './scene';

/** What a host element sees of the walk while it is visited before layers are built. */
export interface HostVisit {
  /**
   * State shared by every host element for one rendered frame, keyed by
   * whatever the element chooses. `prepare` writes here and `content` reads
   * it; audio-only sweeps skip `prepare`, so `audio` must not rely on it.
   */
  readonly frameState: Map<unknown, unknown>;
}

/** What a host element sees of the walk at its own position in the tree. */
export interface HostWalk extends HostVisit {
  /** The layer id this element is given; prefix child layer ids with it. */
  readonly id: string;
  /** The frame on the root composition's clock, the same inside any `<Sequence>`. */
  readonly frame: number;
  readonly fps: number;
  /** True inside a `<FreezeFrame>`. */
  readonly frozen: boolean;
  /** An element's `src`-like prop, a path, asset reference, or ref, as a scene asset. */
  resolveAsset(src: unknown): ResolvedAsset;
  /** The layers `node` renders as here, with `path` as its id unless it has an authored one. */
  layers(node: HostNode, path: string): Layer[];
  /** Plays `props` (`src`, `startFrom`, `playbackRate`, `volume`, `muted`) as `<Audio>` would here. */
  collectAudio(props: Record<string, unknown>): void;
}

export interface HostElement {
  /**
   * Runs for every element of this type the frame shows, in tree order and
   * before any layer is built, so an element can affect one rendered earlier.
   * Visual frames only: audio-only sweeps skip it.
   */
  prepare?(node: HostNode, visit: HostVisit): void;
  /** Collects the element's audio; runs for audio-only sweeps too, without `prepare`. */
  audio?(node: HostNode, walk: HostWalk): void;
  /**
   * The element's layer content; its transform, opacity, blend mode, and
   * effects come from the common props. Without `content` it draws nothing.
   */
  content?(node: HostNode, walk: HostWalk): LayerContent;
}

const elements = new Map<string, HostElement>();

/** Teaches the scene walker a host element type. */
export function registerHostElement(type: string, element: HostElement): void {
  elements.set(type, element);
}

export function hostElement(type: string): HostElement | undefined {
  return elements.get(type);
}

export function hasHostElementPreparation(): boolean {
  for (const element of elements.values()) {
    if (element.prepare) return true;
  }
  return false;
}
