// Types and helpers the tests share.

import assert from 'node:assert/strict';

import type { Layer, LayerContent } from '@celesta.studio/react';

/** The content of a layer of type `T`. */
export type Content<T extends LayerContent['type']> = Extract<LayerContent, { type: T }>;

/** `layer`'s content, checked to be of `type`. */
export function contentOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): Content<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer.content as Content<T>;
}

/** The layers of a group layer. */
export const childrenOf = (layer: Layer | undefined): Layer[] => contentOf(layer, 'group').layers;
