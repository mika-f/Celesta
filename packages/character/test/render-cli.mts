// The lines the render CLI (packages/cli/bin/celesta-react-render.js) writes
// to stdout, as far as the tests that spawn it read them; `Message` in
// packages/cli/src/cli.ts has the rest. Those tests are integration tests of
// the build: the CLI loads the entry's @celesta/character from dist/.

import assert from 'node:assert/strict';

import type { CompositionConfig, Scene } from '@celesta/react';
import type { AudioClipDescriptor, PrepareDependency } from '@celesta/react/internal';

/** The first line: what the entry declares, and the files its `prepare()` read. */
export interface ReadyMessage {
  config: CompositionConfig;
  dependencies: PrepareDependency[];
}

/** The answer to a frame request. */
export interface FrameMessage {
  scene: Scene;
  audio: AudioClipDescriptor[];
}

export type CliMessage = ReadyMessage | FrameMessage | { probeMedia: { path: string } } | { error: string };

/** Parses one line the CLI wrote. */
export const parseMessage = (line: string): CliMessage => JSON.parse(line);

/** The error `message` reports, if it is one. */
export const errorOf = (message: CliMessage): string | undefined => ('error' in message ? message.error : undefined);

/** `message`, checked to be a rendered frame and not an error. */
export function frameOf(message: CliMessage | undefined): FrameMessage {
  assert.ok(message, 'a message');
  assert.equal(errorOf(message), undefined, 'a frame without an error');
  assert.ok('scene' in message, 'a frame');
  return message;
}
