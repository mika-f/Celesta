// The directory of the composition entry file currently being rendered.
// `cli.ts` sets `MIKAN_REACT_ENTRY_DIR` before running the entry's
// `prepare()`, so helpers that read local files during preparation
// (`loadLipSync`, `loadPsdPreset`) resolve relative paths the same way the
// Rust renderer resolves a relative `<Audio>`/`<Image>` `src` — against the
// entry's own directory.

import * as path from 'node:path';

export function entryDir(): string {
  return process.env.MIKAN_REACT_ENTRY_DIR ?? process.cwd();
}

/** Resolves `src` against the entry directory unless it is already absolute. */
export function entryRelativePath(src: string): string {
  return path.isAbsolute(src) ? src : path.resolve(entryDir(), src);
}
