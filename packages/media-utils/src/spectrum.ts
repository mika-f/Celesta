// Frequency bands and loudness of an audio file, analyzed once in an entry's
// `prepare()` and read per frame. The whole file is analyzed in order, at a
// fixed hop independent of the composition's fps, so smoothing carries from
// one hop to the next and still every frame gets the same value whichever
// order frames are rendered in.

import { useCurrentTime } from '@celesta.studio/react';
import { decodeAudio } from '@celesta.studio/react/internal';
import type { DecodedAudio } from '@celesta.studio/react/internal';

/** How the frequency range is divided into bands: equal steps on this scale. */
export type FrequencyScale = 'log' | 'linear' | 'mel';

/** Exponential smoothing time constants in seconds: about 63% of a change is reached after this long. */
export interface AudioSmoothing {
  /** For values rising. */
  attack: number;
  /** For values falling. */
  release: number;
}

export interface AudioSpectrumOptions {
  /** Samples in each FFT: a power of two from 32 to 32768. Defaults to 2048. */
  fftSize?: number;
  /** Number of frequency bands. Defaults to 32. */
  bands?: number;
  /** Defaults to `'log'`, equal steps in octaves. */
  scale?: FrequencyScale;
  /** Lower edge of the first band in Hz. Defaults to 20. */
  minFrequency?: number;
  /** Upper edge of the last band in Hz, at most half the sample rate. Defaults to 16000. */
  maxFrequency?: number;
  /** Band level that maps to 0, in dB relative to a full-scale sine. Defaults to -80. */
  minDecibels?: number;
  /**
   * Band level that maps to 1. Defaults to `'auto'`: the loudest band level
   * anywhere in the file, before smoothing, so only its peaks reach 1 however
   * loud it was mastered.
   */
  maxDecibels?: number | 'auto';
  /**
   * Seconds, or separate attack and release times; 0 (the default) does not
   * smooth. Applies to the bands and the RMS.
   */
  smoothing?: number | AudioSmoothing;
  /** Seconds between analyses; frames in between are interpolated. Defaults to 1/240. */
  hopSeconds?: number;
}

/** The analysis at one moment. */
export interface AudioSpectrumFrame {
  /** Level of each band from low to high frequency, 0 to 1. */
  bands: number[];
  /** Root mean square of the samples in the FFT window: about 0.707 for a full-scale sine. */
  rms: number;
}

/** A whole file's analysis, as `loadAudioSpectrum()` returns it. Plain data. */
export interface AudioSpectrum {
  /** The path or URL it was loaded from, for `<Audio src>`. */
  src?: string;
  sampleRate: number;
  durationSeconds: number;
  hopSeconds: number;
  /** Lower edge of every band, then the upper edge of the last: `bands + 1` frequencies in Hz. */
  bandEdges: number[];
  /** Hop after hop, every band's level: `hops × bands` values. */
  bandLevels: Float32Array;
  /** The RMS at every hop. */
  rms: Float32Array;
}

interface ResolvedOptions {
  fftSize: number;
  bands: number;
  scale: FrequencyScale;
  minFrequency: number;
  maxFrequency: number;
  minDecibels: number;
  maxDecibels: number | 'auto';
  attack: number;
  release: number;
  hopSeconds: number;
}

function resolveOptions(options: AudioSpectrumOptions, sampleRate: number): ResolvedOptions {
  const {
    fftSize = 2048,
    bands = 32,
    scale = 'log',
    minFrequency = 20,
    minDecibels = -80,
    maxDecibels = 'auto',
    smoothing = 0,
    hopSeconds = 1 / 240,
  } = options;
  if (!Number.isInteger(fftSize) || fftSize < 32 || fftSize > 32768 || (fftSize & (fftSize - 1)) !== 0) {
    throw new Error(`fftSize must be a power of two from 32 to 32768, got ${fftSize}`);
  }
  if (!Number.isInteger(bands) || bands <= 0) {
    throw new Error(`bands must be a positive integer, got ${bands}`);
  }
  if (scale !== 'log' && scale !== 'linear' && scale !== 'mel') {
    throw new Error(`scale must be 'log', 'linear', or 'mel', got ${JSON.stringify(scale)}`);
  }
  const maxFrequency = Math.min(options.maxFrequency ?? 16000, sampleRate / 2);
  if (!Number.isFinite(minFrequency) || minFrequency < 0 || (scale === 'log' && minFrequency <= 0)) {
    throw new Error(`minFrequency must be ${scale === 'log' ? 'positive' : 'zero or more'} on the ${scale} scale, got ${minFrequency}`);
  }
  if (!Number.isFinite(maxFrequency) || maxFrequency <= minFrequency) {
    throw new Error(`maxFrequency (${maxFrequency} Hz, at most half the ${sampleRate} Hz sample rate) must be above minFrequency (${minFrequency} Hz)`);
  }
  if (maxDecibels === 'auto') {
    if (!Number.isFinite(minDecibels)) throw new Error(`minDecibels must be a finite number, got ${minDecibels}`);
  } else if (!Number.isFinite(minDecibels) || !Number.isFinite(maxDecibels) || minDecibels >= maxDecibels) {
    throw new Error(`minDecibels (${minDecibels}) must be below maxDecibels (${JSON.stringify(maxDecibels)})`);
  }
  const { attack, release } = typeof smoothing === 'number' ? { attack: smoothing, release: smoothing } : smoothing;
  for (const [name, value] of [['attack', attack], ['release', release]] as const) {
    if (!Number.isFinite(value) || value < 0) {
      throw new Error(`smoothing ${name} must be zero or more seconds, got ${value}`);
    }
  }
  if (!Number.isFinite(hopSeconds) || hopSeconds <= 0) {
    throw new Error(`hopSeconds must be positive, got ${hopSeconds}`);
  }
  return { fftSize, bands, scale, minFrequency, maxFrequency, minDecibels, maxDecibels, attack, release, hopSeconds };
}

const toScale: Record<FrequencyScale, (hz: number) => number> = {
  log: (hz) => Math.log(hz),
  linear: (hz) => hz,
  mel: (hz) => 2595 * Math.log10(1 + hz / 700),
};

const fromScale: Record<FrequencyScale, (value: number) => number> = {
  log: (value) => Math.exp(value),
  linear: (value) => value,
  mel: (value) => 700 * (10 ** (value / 2595) - 1),
};

function bandEdges({ bands, scale, minFrequency, maxFrequency }: ResolvedOptions): number[] {
  const low = toScale[scale](minFrequency);
  const high = toScale[scale](maxFrequency);
  const edges = Array.from({ length: bands + 1 }, (_, i) => fromScale[scale](low + ((high - low) * i) / bands));
  // Exact ends, without the round trip through the scale.
  edges[0] = minFrequency;
  edges[bands] = maxFrequency;
  return edges;
}

/** Which power-spectrum bins each band averages; a band narrower than a bin interpolates at its center. */
type BandBins = { first: number; last: number } | { at: number };

function bandBins(edges: number[], options: ResolvedOptions, sampleRate: number): BandBins[] {
  const binHz = sampleRate / options.fftSize;
  const bins: BandBins[] = [];
  for (let band = 0; band < options.bands; band += 1) {
    const low = edges[band];
    const high = edges[band + 1];
    const first = Math.ceil(low / binHz);
    // Upper edges are exclusive, except for the last band's.
    const last = band === options.bands - 1 ? Math.floor(high / binHz) : Math.ceil(high / binHz) - 1;
    if (first <= last) {
      bins.push({ first, last });
    } else {
      const center = fromScale[options.scale]((toScale[options.scale](low) + toScale[options.scale](high)) / 2);
      bins.push({ at: center / binHz });
    }
  }
  return bins;
}

interface FftTables {
  reverse: Uint32Array;
  cos: Float64Array;
  sin: Float64Array;
  window: Float64Array;
  /** Scales a bin's magnitude so a full-scale sine at its frequency reads 1. */
  gain: number;
}

const fftTables = new Map<number, FftTables>();

function tables(size: number): FftTables {
  let found = fftTables.get(size);
  if (found) return found;
  const bits = Math.log2(size);
  const reverse = new Uint32Array(size);
  for (let i = 0; i < size; i += 1) {
    let r = 0;
    for (let b = 0; b < bits; b += 1) r = (r << 1) | ((i >> b) & 1);
    reverse[i] = r;
  }
  const cos = new Float64Array(size / 2);
  const sin = new Float64Array(size / 2);
  for (let i = 0; i < size / 2; i += 1) {
    cos[i] = Math.cos((2 * Math.PI * i) / size);
    sin[i] = -Math.sin((2 * Math.PI * i) / size);
  }
  // Periodic Hann.
  const window = new Float64Array(size);
  let sum = 0;
  for (let i = 0; i < size; i += 1) {
    window[i] = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / size);
    sum += window[i];
  }
  found = { reverse, cos, sin, window, gain: 2 / sum };
  fftTables.set(size, found);
  return found;
}

/** In-place radix-2 FFT of `re` + i·`im`. */
function fft(re: Float64Array, im: Float64Array, { reverse, cos, sin }: FftTables): void {
  const size = re.length;
  for (let i = 0; i < size; i += 1) {
    const j = reverse[i];
    if (j > i) {
      const r = re[i];
      re[i] = re[j];
      re[j] = r;
      const m = im[i];
      im[i] = im[j];
      im[j] = m;
    }
  }
  // One twiddle factor per outer iteration; integer indices throughout.
  for (let half = 1; half < size; half <<= 1) {
    const span = half << 1;
    const step = (size / span) | 0;
    for (let k = 0, twiddle = 0; k < half; k += 1, twiddle += step) {
      const wr = cos[twiddle];
      const wi = sin[twiddle];
      for (let a = k; a < size; a += span) {
        const b = a + half;
        const xr = re[b];
        const xi = im[b];
        const tr = xr * wr - xi * wi;
        const ti = xr * wi + xi * wr;
        const ar = re[a];
        const ai = im[a];
        re[b] = ar - tr;
        im[b] = ai - ti;
        re[a] = ar + tr;
        im[a] = ai + ti;
      }
    }
  }
}

/**
 * Analyzes decoded mono samples. `loadAudioSpectrum()` does this for a file;
 * call it directly with samples from elsewhere. Pure: the same samples and
 * options always give the same result.
 */
export function analyzeAudioSpectrum(audio: DecodedAudio, options: AudioSpectrumOptions = {}): AudioSpectrum {
  const { sampleRate, samples } = audio;
  if (!Number.isFinite(sampleRate) || sampleRate <= 0) {
    throw new Error(`analyzeAudioSpectrum() requires a positive sample rate, got ${sampleRate}`);
  }
  const resolved = resolveOptions(options, sampleRate);
  const { fftSize, bands, minDecibels, hopSeconds } = resolved;
  const edges = bandEdges(resolved);
  const bins = bandBins(edges, resolved, sampleRate);
  const fftTable = tables(fftSize);
  const durationSeconds = samples.length / sampleRate;
  const hops = Math.floor(durationSeconds / hopSeconds) + 1;
  const bandLevels = new Float32Array(hops * bands);
  const rms = new Float32Array(hops);
  const re = new Float64Array(fftSize);
  const im = new Float64Array(fftSize);
  const power = new Float64Array(fftSize / 2 + 1);
  // Per-hop smoothing coefficients; 1 follows the input exactly.
  const rise = resolved.attack > 0 ? 1 - Math.exp(-hopSeconds / resolved.attack) : 1;
  const fall = resolved.release > 0 ? 1 - Math.exp(-hopSeconds / resolved.release) : 1;
  const smooth = (previous: number, next: number) => previous + (next > previous ? rise : fall) * (next - previous);

  // First every band's level in dB, and the RMS, unsmoothed.
  let peakDecibels = -Infinity;
  for (let hop = 0; hop < hops; hop += 1) {
    // The window is centered on the hop's time; outside the file is silence.
    const start = Math.round(hop * hopSeconds * sampleRate) - fftSize / 2;
    let squares = 0;
    for (let i = 0; i < fftSize; i += 1) {
      const index = start + i;
      const sample = index >= 0 && index < samples.length ? samples[index] : 0;
      squares += sample * sample;
      re[i] = sample * fftTable.window[i];
      im[i] = 0;
    }
    fft(re, im, fftTable);
    for (let k = 0; k <= fftSize / 2; k += 1) {
      // DC and Nyquist have no mirrored negative frequency to fold in.
      const gain = k === 0 || k === fftSize / 2 ? fftTable.gain / 2 : fftTable.gain;
      const magnitude = Math.hypot(re[k], im[k]) * gain;
      power[k] = magnitude * magnitude;
    }

    const row = hop * bands;
    for (let band = 0; band < bands; band += 1) {
      const span = bins[band];
      let bandPower: number;
      if ('at' in span) {
        const k = Math.min(Math.floor(span.at), fftSize / 2 - 1);
        const t = span.at - k;
        bandPower = power[k] + (power[k + 1] - power[k]) * t;
      } else {
        bandPower = 0;
        for (let k = span.first; k <= span.last; k += 1) bandPower += power[k];
        bandPower /= span.last - span.first + 1;
      }
      const decibels = bandPower > 0 ? 10 * Math.log10(bandPower) : -Infinity;
      bandLevels[row + band] = decibels;
      if (decibels > peakDecibels) peakDecibels = decibels;
    }
    rms[hop] = Math.sqrt(squares / fftSize);
  }

  // Then onto 0–1, smoothed in order. A file whose loudest band is at or
  // below minDecibels is all zeros, like silence.
  const maxDecibels = resolved.maxDecibels === 'auto' ? peakDecibels : resolved.maxDecibels;
  const range = maxDecibels - minDecibels;
  for (let hop = 0; hop < hops; hop += 1) {
    const row = hop * bands;
    for (let band = 0; band < bands; band += 1) {
      const level = range > 0 ? Math.min(1, Math.max(0, (bandLevels[row + band] - minDecibels) / range)) : 0;
      bandLevels[row + band] = smooth(hop === 0 ? 0 : bandLevels[row - bands + band], level);
    }
    rms[hop] = smooth(hop === 0 ? 0 : rms[hop - 1], rms[hop]);
  }
  return { sampleRate, durationSeconds, hopSeconds, bandEdges: edges, bandLevels, rms };
}

/**
 * The analysis at `seconds` into the file, interpolated between hops. Outside
 * the file every band and the RMS are 0. See `useAudioSpectrum()`.
 */
export function spectrumAt(spectrum: AudioSpectrum, seconds: number): AudioSpectrumFrame {
  const bands = spectrum.bandEdges.length - 1;
  const hops = spectrum.rms.length;
  if (!(seconds >= 0) || seconds > spectrum.durationSeconds || hops === 0) {
    return { bands: new Array<number>(bands).fill(0), rms: 0 };
  }
  const position = seconds / spectrum.hopSeconds;
  const hop = Math.min(Math.floor(position), hops - 1);
  const next = Math.min(hop + 1, hops - 1);
  const t = position - hop;
  const lerp = (a: number, b: number) => a + (b - a) * t;
  const levels: number[] = [];
  for (let band = 0; band < bands; band += 1) {
    levels.push(lerp(spectrum.bandLevels[hop * bands + band], spectrum.bandLevels[next * bands + band]));
  }
  return { bands: levels, rms: lerp(spectrum.rms[hop], spectrum.rms[next]) };
}

export interface UseAudioSpectrumOptions {
  /**
   * Seconds into the file at this component's frame 0, like `<Audio
   * startFrom>`. Defaults to 0. Playback rate is not followed.
   */
  startFrom?: number;
}

/**
 * The bands and RMS of a `loadAudioSpectrum()` analysis at the current frame,
 * read from the enclosing `<Sequence>`'s clock: put it beside the `<Audio>` it
 * visualizes.
 *
 * ```tsx
 * let music: AudioSpectrum;
 * export async function prepare() {
 *   music = await loadAudioSpectrum('./song.mp3', { bands: 24, smoothing: { attack: 0.01, release: 0.2 } });
 * }
 * function Bars() {
 *   const { bands, rms } = useAudioSpectrum(music);
 *   …
 * }
 * ```
 */
export function useAudioSpectrum(spectrum: AudioSpectrum, options: UseAudioSpectrumOptions = {}): AudioSpectrumFrame {
  const time = useCurrentTime();
  return spectrumAt(spectrum, time.value / time.timescale + (options.startFrom ?? 0));
}

// Analyses of each decoding, which the runtime replaces when it installs a
// new decoder (the browser runtime does on every compile).
const analyses = new WeakMap<Promise<DecodedAudio>, Map<string, Promise<AudioSpectrum>>>();

/**
 * Decodes an audio file (or a video's soundtrack) and analyzes it, during an
 * entry's async `prepare()`. Relative paths resolve from the entry file, like
 * `<Audio>`; an `http`/`https` URL is downloaded first. The channels are
 * averaged, at the file's own sample rate. Repeated calls in one process
 * share the result.
 */
export function loadAudioSpectrum(src: string, options: AudioSpectrumOptions = {}): Promise<AudioSpectrum> {
  const decoding = decodeAudio(src);
  let byOptions = analyses.get(decoding);
  if (!byOptions) {
    byOptions = new Map();
    analyses.set(decoding, byOptions);
  }
  const key = `${src}\0${JSON.stringify(options)}`;
  let pending = byOptions.get(key);
  if (!pending) {
    pending = decoding.then((audio) => ({ ...analyzeAudioSpectrum(audio, options), src }));
    byOptions.set(key, pending);
  }
  return pending;
}
