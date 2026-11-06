// Writing a file so that readers see the old contents or the new, never part
// of the new: write a temporary file beside it, then rename it into place.

import * as fs from 'node:fs';

/** How `writeFileAtomic` treats a rename that fails. */
export interface WriteFileAtomicOptions {
  /**
   * `'fail'` (the default) throws the rename's error. `'same-size'` returns
   * instead when the target then exists with as many bytes as `data`: for a
   * file named by its content's hash, another process published the same
   * bytes first (on Windows, a rename onto a file another process has open
   * fails).
   */
  ifExists?: 'fail' | 'same-size';
}

function temporaryPath(target: string): string {
  return `${target}.${process.pid}.${Math.random().toString(36).slice(2)}.tmp`;
}

function publishedBySomeoneElse(target: string, data: Uint8Array | string, options: WriteFileAtomicOptions): boolean {
  if (options.ifExists !== 'same-size') return false;
  const size = typeof data === 'string' ? Buffer.byteLength(data) : data.byteLength;
  return fs.statSync(target, { throwIfNoEntry: false })?.size === size;
}

/**
 * @internal Writes `data` to `target` through a temporary file in the same
 * folder, renamed into place; the temporary file is removed on any error.
 * Not for compositions.
 */
export function writeFileAtomicSync(target: string, data: Uint8Array | string, options: WriteFileAtomicOptions = {}): void {
  const temporary = temporaryPath(target);
  try {
    fs.writeFileSync(temporary, data);
    fs.renameSync(temporary, target);
  } catch (error) {
    fs.rmSync(temporary, { force: true });
    if (!publishedBySomeoneElse(target, data, options)) throw error;
  }
}

/** @internal `writeFileAtomicSync`, without blocking. Not for compositions. */
export async function writeFileAtomic(target: string, data: Uint8Array | string, options: WriteFileAtomicOptions = {}): Promise<void> {
  const temporary = temporaryPath(target);
  try {
    await fs.promises.writeFile(temporary, data);
    await fs.promises.rename(temporary, target);
  } catch (error) {
    await fs.promises.rm(temporary, { force: true });
    if (!publishedBySomeoneElse(target, data, options)) throw error;
  }
}
