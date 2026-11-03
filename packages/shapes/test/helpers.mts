// Types and helpers the tests share.

import assert from 'node:assert/strict';

import type { Layer, LayerContent, Scene } from '@celesta.studio/react';
import type { PathCommand } from '../src/index.js';

/** The content of a layer of type `T`. */
export type Content<T extends LayerContent['type']> = Extract<LayerContent, { type: T }>;

/** `layer`'s content, checked to be of `type`. */
export function contentOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): Content<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer.content as Content<T>;
}

/** A path command of type `T`. */
export type Command<T extends PathCommand['type']> = Extract<PathCommand, { type: T }>;

/** `command`, checked to be of `type`. */
export function commandOf<T extends PathCommand['type']>(command: PathCommand | undefined, type: T): Command<T> {
  assert.ok(command, `a ${type} command`);
  assert.equal(command.type, type);
  return command as Command<T>;
}

/** A path command with an end point: any but `close`. */
export type OnCurveCommand = Exclude<PathCommand, { type: 'close' }>;

/**
 * A line `celesta-react-render` answers a frame request with. When evaluating
 * the frame failed, it holds only `error`; the tests read the field they
 * expect.
 */
export interface FrameResponse {
  scene: Scene;
  error?: string;
}

/**
 * One of the CLI's output lines, as the answer to a frame request. The CLI
 * starts with a handshake line, which the tests skip unread.
 */
export const parseLine = (line: string): FrameResponse => JSON.parse(line);
