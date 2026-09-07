import { entryRelativePath } from './entry-dir';
import type { Rational } from './scene';

export interface MediaVideoInfo {
  codec?: string;
  width: number;
  height: number;
  frameRate?: Rational;
  durationSeconds?: number;
}

export interface MediaAudioInfo {
  codec?: string;
  sampleRate?: number;
  channels?: number;
  durationSeconds?: number;
}

export interface MediaInfo {
  src: string;
  durationSeconds?: number;
  video?: MediaVideoInfo;
  audio: MediaAudioInfo[];
}

export type ProbedMediaInfo = Omit<MediaInfo, 'src'>;

let probe: ((path: string) => Promise<ProbedMediaInfo>) | undefined;
const cached = new Map<string, Promise<ProbedMediaInfo>>();

/** @internal Installed by the Frameweave CLI before an entry's `prepare()` runs. */
export function setMediaProbe(next: (path: string) => Promise<ProbedMediaInfo>): void {
  probe = next;
  cached.clear();
}

/**
 * Probes and caches local media metadata during an entry's async `prepare()`.
 * Relative paths resolve from the entry file, matching `<Video>` and `<Audio>`.
 */
export async function preloadMedia(src: string): Promise<MediaInfo> {
  if (!probe) {
    throw new Error('preloadMedia() requires a Frameweave editor or exporter runtime');
  }
  const path = entryRelativePath(src);
  let pending = cached.get(path);
  if (!pending) {
    pending = probe(path);
    cached.set(path, pending);
  }
  return { src, ...(await pending) };
}

/** Converts a probed duration to a composition frame count. */
export function mediaDurationInFrames(media: MediaInfo, fps: number): number | undefined {
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('mediaDurationInFrames() requires a positive finite fps');
  }
  return media.durationSeconds === undefined ? undefined : Math.ceil(media.durationSeconds * fps);
}
