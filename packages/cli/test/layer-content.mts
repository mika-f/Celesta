import assert from 'node:assert/strict';

import type { Layer, LayerContent } from '@celesta.studio/react';

export type ContentOf<T extends LayerContent['type']> = Extract<LayerContent, { type: T }>;

/** The layer's content, asserted to be a `type` one. */
export function contentOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): ContentOf<T> {
  assert.equal(layer?.content.type, type);
  return layer!.content as ContentOf<T>;
}
