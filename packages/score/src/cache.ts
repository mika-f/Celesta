// The score's file on disk. Node's modules are imported here, when a score is
// prepared, so that a composition that only imports the score still loads
// where there is no file system.

/** Bumped when the file's name or layout changes. */
const CACHE_FORMAT = 1;

export interface WrittenScore {
  key: string;
  path: string;
  /** True when the file was already there. */
  cached: boolean;
}

/**
 * Writes `wav()` to `<cacheDir>/<sha256 of input>.wav` unless a file of
 * `size` bytes is already there, through a temporary file and a rename.
 * `wav` is only called when the file has to be written.
 */
export async function writeScoreFile(input: string, size: number, wav: () => Uint8Array, cacheDir: string): Promise<WrittenScore> {
  const { createHash, randomBytes } = await import('node:crypto');
  const { mkdir, rename, rm, stat, writeFile } = await import('node:fs/promises');
  const path = await import('node:path');

  const key = createHash('sha256').update(`celesta-score/${CACHE_FORMAT}\n${input}`).digest('hex');
  const dir = path.isAbsolute(cacheDir)
    ? cacheDir
    : path.resolve(process.env.CELESTA_REACT_ENTRY_DIR ?? process.cwd(), cacheDir);
  const target = path.join(dir, `${key}.wav`);
  try {
    if ((await stat(target)).size === size) return { key, path: target, cached: true };
  } catch {
    // Not there yet.
  }
  await mkdir(dir, { recursive: true });
  const temporary = path.join(dir, `.${key}.${randomBytes(6).toString('hex')}.tmp`);
  try {
    await writeFile(temporary, wav());
    await rename(temporary, target);
  } catch (error) {
    await rm(temporary, { force: true });
    throw error;
  }
  return { key, path: target, cached: false };
}
