import { entryRelativePath, isRemoteUrl } from './entry-dir';
import type { Rational } from './scene';

/** A video stream's color tags, by FFmpeg's names for them. */
export interface MediaVideoColor {
  /** `bt709`, `bt2020`, `smpte432`, … */
  primaries?: string;
  /** `bt709`, `smpte2084` (PQ), `arib-std-b67` (HLG), … */
  transfer?: string;
  /** `bt709`, `bt2020nc`, `smpte170m`, … */
  matrix?: string;
}

export interface MediaVideoInfo {
  codec?: string;
  width: number;
  height: number;
  frameRate?: Rational;
  durationSeconds?: number;
  /** Absent when the stream carries no color tags. */
  color?: MediaVideoColor;
  /**
   * Set for HDR video. `<Video>` tone maps it to SDR the way Chrome draws
   * it onto a canvas.
   */
  hdr?: 'pq' | 'hlg';
}

export interface MediaAudioInfo {
  codec?: string;
  sampleRate?: number;
  channels?: number;
  durationSeconds?: number;
}

/** An animated GIF, APNG, or WebP, as `<Image>` plays it. */
export interface MediaAnimationInfo {
  frameCount: number;
  /** One pass through every frame. */
  durationSeconds: number;
}

export interface MediaInfo {
  src: string;
  durationSeconds?: number;
  video?: MediaVideoInfo;
  audio: MediaAudioInfo[];
  /** Present for an animated image; `durationSeconds` is then its length too. */
  animation?: MediaAnimationInfo;
}

export type ProbedMediaInfo = Omit<MediaInfo, 'src'>;

let probe: ((path: string) => Promise<ProbedMediaInfo>) | undefined;
const cached = new Map<string, Promise<ProbedMediaInfo>>();

/** @internal Installed by the Celesta CLI before an entry's `prepare()` runs. */
export function setMediaProbe(next: (path: string) => Promise<ProbedMediaInfo>): void {
  probe = next;
  cached.clear();
}

/**
 * `@celesta.studio/media-utils`' `preloadMedia()`: probes and caches media metadata
 * during an entry's async `prepare()`. The cache lives here, beside the probe
 * the CLI installs. Relative paths resolve from the entry file, matching
 * `<Video>` and `<Audio>`; an `http`/`https` URL is downloaded into Celesta's
 * cache first.
 */
export async function probeMedia(src: string): Promise<MediaInfo> {
  if (!probe) {
    throw new Error('preloadMedia() requires a Celesta editor or exporter runtime');
  }
  const path = isRemoteUrl(src) ? src : entryRelativePath(src);
  let pending = cached.get(path);
  if (!pending) {
    pending = probe(path);
    cached.set(path, pending);
    // A failure is not cached, so a later call can probe the path again.
    // The check keeps a probe installed since then from losing its entry.
    const failed = pending;
    failed.catch(() => {
      if (cached.get(path) === failed) cached.delete(path);
    });
  }
  return { src, ...(await pending) };
}

/** Mono PCM: the mean of the source's channels, at its own sample rate. */
export interface DecodedAudio {
  sampleRate: number;
  samples: Float32Array;
}

let audioDecoder: ((path: string) => Promise<DecodedAudio>) | undefined;
const decoded = new Map<string, Promise<DecodedAudio>>();

/**
 * @internal Installed by the Celesta CLI, or the browser runtime, before an
 * entry's `prepare()` runs.
 */
export function setAudioDecoder(next: (path: string) => Promise<DecodedAudio>): void {
  audioDecoder = next;
  decoded.clear();
}

/**
 * @internal Decodes an audio file, or a video's soundtrack, during an entry's
 * async `prepare()`, once per path. Paths resolve like `probeMedia()`. A path
 * gets the same promise until it fails or a decoder is installed again, so
 * callers can key their own caches on it.
 */
export function decodeAudio(src: string): Promise<DecodedAudio> {
  if (!audioDecoder) {
    return Promise.reject(new Error('decoding audio requires a Celesta editor, exporter, or @celesta.studio/web runtime'));
  }
  const path = isRemoteUrl(src) ? src : entryRelativePath(src);
  let pending = decoded.get(path);
  if (!pending) {
    pending = audioDecoder(path);
    decoded.set(path, pending);
    // A failure is not cached, so a later call can try again.
    const failed = pending;
    failed.catch(() => {
      if (decoded.get(path) === failed) decoded.delete(path);
    });
  }
  return pending;
}
