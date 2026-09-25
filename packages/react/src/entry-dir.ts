// The directory of the composition entry file currently being rendered.
// `cli.ts` sets `CELESTA_REACT_ENTRY_DIR` before running the entry's
// `prepare()`, so helpers that read local files during preparation
// (`loadLipSync`, `loadPsdPreset`) resolve relative paths the same way the
// Rust renderer resolves a relative `<Audio>`/`<Image>` `src` — against the
// entry's own directory.

import * as path from 'node:path';

export function entryDir(): string {
  return process.env.CELESTA_REACT_ENTRY_DIR ?? process.cwd();
}

/** Whether `src` is an `http`/`https` URL, which the Rust side downloads and caches. */
export function isRemoteUrl(src: string): boolean {
  return /^https?:\/\/./i.test(src);
}

/** Resolves `src` against the entry directory unless it is already absolute. */
export function entryRelativePath(src: string): string {
  return path.isAbsolute(src) ? src : path.resolve(entryDir(), src);
}
