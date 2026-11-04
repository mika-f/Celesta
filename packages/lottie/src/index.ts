import * as React from 'react';

import { Group, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import type { CommonProps } from '@celesta.studio/react';

import { inspectLottie } from './inspect';
import { compositionLayers } from './layers';
import type { RenderContext } from './layers';
import type { LottieAnimation, LottieAsset } from './types';

export { inspectLottie };
export type {
  LottieAnimation,
  LottieAsset,
  LottieBezier,
  LottieEasing,
  LottieKeyframe,
  LottieLayer,
  LottieMask,
  LottieProperty,
  LottieShape,
  LottieTransform,
} from './types';

export interface LottieProps extends CommonProps {
  /**
   * The animation: a Bodymovin/Lottie JSON file's contents, as
   * `import animation from './animation.json'` gives them.
   */
  data: LottieAnimation;
  /**
   * Width to draw the animation's box at, in pixels. Alone, keeps the
   * aspect ratio. Defaults to the file's `w`.
   */
  width?: number;
  /** Height to draw the animation's box at. Alone, keeps the aspect ratio. Defaults to the file's `h`. */
  height?: number;
  /** Speed relative to the enclosing sequence's clock; negative plays backwards. Defaults to 1. */
  playbackRate?: number;
  /** Starts over after the last frame instead of holding it. Defaults to true. */
  loop?: boolean;
  /** First frame of the animation to play, in its own frames. Defaults to the file's `ip`. */
  startFrame?: number;
  /** Frame to stop before, in the animation's own frames. Defaults to the file's `op`. */
  endFrame?: number;
  /**
   * Directory image layers' files are in, relative to the entry like an
   * `<Image>`'s `src`. Defaults to each image asset's own `u`.
   */
  assetsPath?: string;
  /**
   * Receives a message for each feature of the file that is not drawn,
   * once per animation. Defaults to `console.warn`.
   */
  onWarning?: (message: string) => void;
}

export interface LottieTiming {
  /** The animation's frame rate, its `fr`. */
  fps: number;
  /** First frame played. */
  startFrame: number;
  /** Frame playback stops before. */
  endFrame: number;
  playbackRate?: number;
  loop?: boolean;
}

/**
 * The animation's frame (fractional between its frames) `seconds` into
 * playback: `startFrame` at 0, then wrapping back to `startFrame` at
 * `endFrame` with `loop`, or holding the last frame, `endFrame - 1`, without.
 */
export function lottieFrameAt(seconds: number, timing: LottieTiming): number {
  const { fps, startFrame, endFrame, playbackRate = 1, loop = true } = timing;
  const length = endFrame - startFrame;
  if (!(length > 0)) {
    return startFrame;
  }
  const offset = seconds * playbackRate * fps;
  if (loop) {
    return startFrame + ((offset % length) + length) % length;
  }
  return startFrame + Math.min(Math.max(0, length - 1), Math.max(0, offset));
}

interface Prepared {
  assets: ReadonlyMap<string, LottieAsset>;
  /** The handlers warned so far. */
  warned: WeakSet<(message: string) => void>;
  warnings: string[];
}

const prepared = new WeakMap<LottieAnimation, Prepared>();

function prepare(data: LottieAnimation): Prepared {
  let entry = prepared.get(data);
  if (entry === undefined) {
    if (typeof data !== 'object' || data === null || !Array.isArray(data.layers)) {
      throw new Error('<Lottie> `data` must be a Lottie animation: a Bodymovin JSON object with `layers`');
    }
    for (const key of ['fr', 'w', 'h'] as const) {
      if (!(typeof data[key] === 'number' && data[key] > 0)) {
        throw new Error(`<Lottie> \`data.${key}\` must be a positive number, got ${JSON.stringify(data[key])}`);
      }
    }
    entry = {
      assets: new Map((data.assets ?? []).map(asset => [asset.id, asset])),
      warned: new WeakSet(),
      warnings: inspectLottie(data),
    };
    prepared.set(data, entry);
  }
  return entry;
}

// Node and browsers both have it; the package targets neither's types.
declare const console: { warn(message: string): void };

const defaultWarn = (message: string) => console.warn(message);

/**
 * Plays a Lottie animation in step with the composition: each frame draws
 * the animation's frame at that time as paths, groups, and masks, so
 * scrubbing anywhere draws the same picture. `x`/`y` place the box's anchor
 * (its top-left by default), and the box clips the animation, as lottie-web
 * does.
 *
 * Drawn: precomposition, solid, image (files, not embedded), null, and shape
 * layers; parenting, time remap and stretch, masks (add, subtract,
 * intersect, inverted, with opacity), and track mattes (alpha, luma, and
 * inverted); groups, paths, rectangles, ellipses, stars, polygons, fills,
 * strokes (dashed too), linear and radial gradients, and trim paths;
 * layer blend modes. Anything else is reported through `onWarning` and left
 * out: text layers, expressions, effects, 3D, merge paths, repeaters, round
 * corners, and the even-odd fill rule among them.
 */
export function Lottie(props: LottieProps): React.ReactElement {
  const {
    data,
    width,
    height,
    playbackRate = 1,
    loop = true,
    startFrame,
    endFrame,
    assetsPath,
    onWarning = defaultWarn,
    anchorX = 0,
    anchorY = 0,
    ...common
  } = props;
  const { assets, warned, warnings } = prepare(data);
  if (warnings.length > 0 && !warned.has(onWarning)) {
    warned.add(onWarning);
    warnings.forEach(message => onWarning(message));
  }
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const lottieFrame = lottieFrameAt(frame / fps, {
    fps: data.fr,
    startFrame: startFrame ?? data.ip,
    endFrame: endFrame ?? data.op,
    playbackRate,
    loop,
  });

  const scaleX = width !== undefined ? width / data.w : height !== undefined ? height / data.h : 1;
  const scaleY = height !== undefined ? height / data.h : scaleX;
  const context: RenderContext = { fps: data.fr, assets, assetsPath };
  const layers = compositionLayers({ layers: data.layers, width: data.w, height: data.h }, lottieFrame, context);
  return React.createElement(
    Group,
    common,
    React.createElement(
      Group,
      {
        x: -anchorX * data.w * scaleX,
        y: -anchorY * data.h * scaleY,
        scaleX,
        scaleY,
        clip: { width: data.w, height: data.h },
      },
      ...layers,
    ),
  );
}
