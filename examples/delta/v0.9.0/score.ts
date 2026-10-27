import { loadAudioSpectrum, spectrumAt } from '@celesta/media-utils';
import type { AudioSpectrum, AudioSpectrumFrame } from '@celesta/media-utils';
import { FPS } from '../components/theme';

// The film's own score, analyzed once in prepare(): the meters, the
// waveform in FADES and the bars that move with the music read it here.
// Relative to the entry file. The analysis is of the file: what the film
// does to it on the way out (the fades and the ducking) is applied by the
// readers that show it, through bedGain() in mix.ts.
export const SCORE_SRC = './score-0.9.0.wav';
export const BANDS = 32;

let spectrum: AudioSpectrum | undefined;

export async function loadScore() {
  spectrum = await loadAudioSpectrum(SCORE_SRC, {
    bands: BANDS, minDecibels: -75, smoothing: { attack: 0.004, release: 0.14 },
  });
}

const SILENT: AudioSpectrumFrame = { bands: Array<number>(BANDS).fill(0), rms: 0 };

/** The score at a composition frame. Chapters sit in sequences, so they ask by absolute frame. */
export function scoreAt(frame: number): AudioSpectrumFrame {
  return spectrum ? spectrumAt(spectrum, frame / FPS) : SILENT;
}
