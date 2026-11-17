import { loadAudioSpectrum, spectrumAt } from '@celesta.studio/media-utils';
import type { AudioSpectrum, AudioSpectrumFrame } from '@celesta.studio/media-utils';
import { FPS } from '../components/theme';

// The film's own score (make-score-0.10.0.py), and its spectrum, analyzed
// once in prepare() for the end card's meter. Relative to the entry file.
export const SCORE = { src: './score-0.10.0.wav', fadeIn: 2, fadeOut: 4 } as const;
export const BANDS = 32;

let spectrum: AudioSpectrum | undefined;

export async function loadScore() {
  spectrum = await loadAudioSpectrum(SCORE.src, {
    bands: BANDS, minDecibels: -75, smoothing: { attack: 0.004, release: 0.14 },
  });
}

const SILENT: AudioSpectrumFrame = { bands: Array<number>(BANDS).fill(0), rms: 0 };

/** The score at a composition frame. */
export function scoreAt(frame: number): AudioSpectrumFrame {
  return spectrum ? spectrumAt(spectrum, frame / FPS) : SILENT;
}
