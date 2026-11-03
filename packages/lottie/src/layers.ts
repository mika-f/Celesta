// The layers of a Lottie composition at one frame, bottom first, timed and
// parented as lottie-web does: `ip`/`op` in the composition's frames, every
// property at the layer's own frame (the composition's minus `st`), and a
// precomposition's layers at that frame over `sr`, or at its time remap.

import * as React from 'react';

import { Group, Image, Mask, Rect } from '@celesta.studio/react';
import type { BlendMode, PathCommand } from '@celesta.studio/react';
import { Path } from '@celesta.studio/shapes';

import {
  bezierCommands,
  groupTransform,
  identity,
  multiply,
  transformCommands,
  transformMatrix,
  transformOpacity,
} from './geometry';
import type { Matrix } from './geometry';
import { bezierAtFrame, scalarAt } from './property';
import { buildShapes } from './shapes';
import type { LottieAsset, LottieLayer, LottieMask } from './types';

const h = React.createElement;

const blendModes: (BlendMode | undefined)[] = [
  'normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten', 'colorDodge', 'colorBurn',
  'hardLight', 'softLight', 'difference', 'exclusion', 'hue', 'saturation', 'color', 'luminosity', 'add',
];

export interface RenderContext {
  /** Frames per second of the animation, for time remapping. */
  fps: number;
  assets: ReadonlyMap<string, LottieAsset>;
  /** Directory image assets' file names are relative to, or `undefined` for their own `u`. */
  assetsPath: string | undefined;
}

interface Composition {
  layers: readonly LottieLayer[];
  width: number;
  height: number;
}

function inRange(layer: LottieLayer, frame: number): boolean {
  return frame >= layer.ip && frame < layer.op;
}

/** A layer's transform with its parents', at the composition's `frame`. */
function layerMatrix(layer: LottieLayer, layers: readonly LottieLayer[], frame: number): Matrix {
  let matrix = transformMatrix(layer.ks, frame - (layer.st ?? 0));
  let current = layer;
  // A parent chain longer than the layer list loops.
  for (let depth = 0; current.parent !== undefined && depth < layers.length; depth += 1) {
    const parent = layers.find(candidate => candidate.ind === current.parent);
    if (parent === undefined) {
      break;
    }
    matrix = multiply(transformMatrix(parent.ks, frame - (parent.st ?? 0)), matrix);
    current = parent;
  }
  return matrix;
}

function invert(m: Matrix): Matrix | null {
  const det = m[0] * m[3] - m[1] * m[2];
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) {
    return null;
  }
  const [a, b, c, d, e, f] = m;
  return [d / det, -b / det, -c / det, a / det, (c * f - d * e) / det, (b * e - a * f) / det];
}

/**
 * Where the layer can show, in its own coordinates: its composition's box,
 * which clips it, mapped back through the layer's transform.
 */
function wholeLayer(composition: Composition, matrix: Matrix): PathCommand[] {
  const inverse = invert(matrix) ?? identity;
  const { width, height } = composition;
  return transformCommands([
    { type: 'moveTo', x: 0, y: 0 },
    { type: 'lineTo', x: width, y: 0 },
    { type: 'lineTo', x: width, y: height },
    { type: 'lineTo', x: 0, y: height },
    { type: 'close' },
  ], inverse);
}

/**
 * The drawing a layer's masks leave visible, applied in turn: add joins a
 * mask's area, subtract cuts it out, and intersect keeps only the overlap.
 * A first mask that subtracts or intersects starts from the whole layer.
 */
function maskDrawing(masks: readonly LottieMask[], frame: number, whole: PathCommand[]): React.ReactNode | undefined {
  const active = masks.filter(mask => mask.mode !== 'n');
  if (active.length === 0) {
    return undefined;
  }
  const full = (key: string) => h(Path, { key, commands: whole, fill: '#FFFFFF' });
  let current: React.ReactNode = active[0].mode === 's' || active[0].mode === 'i' ? full('whole') : null;
  active.forEach((mask, index) => {
    const opacity = Math.min(1, Math.max(0, scalarAt(mask.o, frame, 100) / 100));
    const path = h(Path, { commands: bezierCommands(bezierAtFrame(mask.pt, frame)), fill: '#FFFFFF' });
    const area = mask.inv === true
      ? h(Group, { key: `m${index}`, opacity, mask: h(Mask, { invert: true }, path) }, full('whole'))
      : h(Group, { key: `m${index}`, opacity }, path);
    if (mask.mode === 's') {
      current = h(Group, { key: `m${index}`, mask: h(Mask, { invert: true }, area) }, current);
    } else if (mask.mode === 'i') {
      current = h(Group, { key: `m${index}`, mask: h(Mask, null, area) }, current);
    } else {
      current = h(Group, { key: `m${index}` }, current, area);
    }
  });
  return h(Mask, null, current);
}

/** A layer's own drawing (no transform), or null for a null layer. */
function layerContent(
  layer: LottieLayer,
  localFrame: number,
  context: RenderContext,
): React.ReactNode {
  switch (layer.ty) {
    case 0: {
      const asset = layer.refId === undefined ? undefined : context.assets.get(layer.refId);
      if (asset?.layers === undefined) {
        return null;
      }
      let childFrame = localFrame / (layer.sr ?? 1);
      if (layer.tm !== undefined) {
        childFrame = scalarAt(layer.tm, localFrame, 0) * context.fps;
        // lottie-web shows the frame before the end at the very end.
        if (childFrame === layer.op) {
          childFrame = layer.op - 1;
        }
      }
      const width = layer.w ?? asset.w ?? 0;
      const height = layer.h ?? asset.h ?? 0;
      return h(
        Group,
        { clip: { width, height } },
        ...compositionLayers({ layers: asset.layers, width, height }, childFrame, context),
      );
    }
    case 1:
      return h(Rect, { width: layer.sw ?? 0, height: layer.sh ?? 0, fill: layer.sc ?? '#000000' });
    case 2: {
      const asset = layer.refId === undefined ? undefined : context.assets.get(layer.refId);
      if (asset?.p === undefined || asset.e === 1 || asset.p.startsWith('data:')) {
        return null;
      }
      let src = asset.p;
      if (!/^https?:\/\//i.test(src)) {
        const directory = context.assetsPath ?? asset.u ?? '';
        src = directory === '' || directory.endsWith('/') ? `${directory}${src}` : `${directory}/${src}`;
      }
      return h(Image, { src, width: asset.w, height: asset.h });
    }
    case 4:
      return h(Group, null, ...[...buildShapes(layer.shapes, localFrame, 'l').elements].reverse());
    default:
      return null;
  }
}

/** A layer at the composition's `frame`, transformed and masked, or null when it draws nothing. */
function layerElement(
  layer: LottieLayer,
  composition: Composition,
  frame: number,
  context: RenderContext,
  key: string,
): React.ReactElement | null {
  const localFrame = frame - (layer.st ?? 0);
  const matrix = layerMatrix(layer, composition.layers, frame);
  const transform = groupTransform(matrix);
  const opacity = transformOpacity(layer.ks, localFrame);
  if (transform === null || opacity <= 0) {
    return null;
  }
  let content = layerContent(layer, localFrame, context);
  if (content === null) {
    return null;
  }
  const masks = layer.masksProperties ?? [];
  if (masks.length > 0) {
    const mask = maskDrawing(masks, localFrame, wholeLayer(composition, matrix));
    if (mask !== undefined) {
      content = h(Group, { mask }, content);
    }
  }
  const blendMode = blendModes[layer.bm ?? 0] ?? 'normal';
  return h(Group, { key, ...transform, opacity, ...(blendMode === 'normal' ? {} : { blendMode }) }, content);
}

/** The drawings of a composition's layers at `frame`, bottom first. */
export function compositionLayers(composition: Composition, frame: number, context: RenderContext): React.ReactElement[] {
  const { layers } = composition;
  const elements: React.ReactElement[] = [];
  for (let index = layers.length - 1; index >= 0; index -= 1) {
    const layer = layers[index];
    if (layer.hd === true || layer.td === 1 || !inRange(layer, frame)) {
      continue;
    }
    const element = layerElement(layer, composition, frame, context, `${index}`);
    if (element === null) {
      continue;
    }
    if (layer.tt === undefined || layer.tt < 1 || layer.tt > 4) {
      elements.push(element);
      continue;
    }
    const matte = layer.tp !== undefined ? layers.find(candidate => candidate.ind === layer.tp) : layers[index - 1];
    const matteElement = matte !== undefined && inRange(matte, frame)
      ? layerElement(matte, composition, frame, context, 'matte')
      : null;
    const mode = layer.tt >= 3 ? 'luminance' : 'alpha';
    const invert = layer.tt === 2 || layer.tt === 4;
    elements.push(h(Group, { key: `${index}`, mask: h(Mask, { mode, invert }, matteElement) }, element));
  }
  return elements;
}
