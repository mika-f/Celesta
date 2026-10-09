import { probeMedia } from '@celesta/react/internal';
import type { MediaInfo } from '@celesta/react/internal';

export type { MediaAudioInfo, MediaInfo, MediaVideoInfo } from '@celesta/react/internal';

/**
 * Probes and caches media metadata during an entry's async `prepare()`.
 * Relative paths resolve from the entry file, matching `<Video>` and `<Audio>`;
 * an `http`/`https` URL is downloaded into Celesta's cache first.
 */
export function preloadMedia(src: string): Promise<MediaInfo> {
  return probeMedia(src);
}

/** Converts a probed duration to a composition frame count. */
export function mediaDurationInFrames(media: MediaInfo, fps: number): number | undefined {
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error('mediaDurationInFrames() requires a positive finite fps');
  }
  return media.durationSeconds === undefined ? undefined : Math.ceil(media.durationSeconds * fps);
}
