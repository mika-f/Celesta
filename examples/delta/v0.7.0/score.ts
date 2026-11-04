import { loadAudioSpectrum, spectrumAt } from '@celesta.studio/media-utils';
import type { AudioSpectrum, AudioSpectrumFrame } from '@celesta.studio/media-utils';
import { FPS } from '../components/theme';

// The film's own score, analyzed once in prepare(): every meter, flash and
// bar that moves with the music reads it here. Relative to the entry file.
export const SCORE_SRC = './score-0.7.0.wav';
export const BANDS = 32;

let spectrum: AudioSpectrum | undefined;

export async function loadScore() {
  // The score is mastered loud: raise the ceiling so the bass bands do not pin at 1.
  spectrum = await loadAudioSpectrum(SCORE_SRC, {
    bands: BANDS, minDecibels: -75, maxDecibels: -5, smoothing: { attack: 0.004, release: 0.14 },
  });
}

const SILENT: AudioSpectrumFrame = { bands: Array<number>(BANDS).fill(0), rms: 0 };

/**
 * The score at a composition frame. useAudioSpectrum() reads the enclosing
 * Sequence's clock; chapters sit in sequences, so they ask by absolute frame.
 */
export function scoreAt(frame: number): AudioSpectrumFrame {
  return spectrum ? spectrumAt(spectrum, frame / FPS) : SILENT;
}

export function scoreSpectrum(): AudioSpectrum {
  if (!spectrum) throw new Error('the score is not loaded: run python3 examples/delta/make-score-0.7.0.py');
  return spectrum;
}
