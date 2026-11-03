import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';

import { Composition, Rect, Sequence } from '@celesta.studio/react';
import type { Layer, LayerContent } from '@celesta.studio/react';
import { mount, setAudioDecoder } from '@celesta.studio/react/internal';
import type { DecodedAudio } from '@celesta.studio/react/internal';
import { analyzeAudioSpectrum, loadAudioSpectrum, spectrumAt, useAudioSpectrum } from '../src/index.js';
import type { AudioSpectrum, AudioSpectrumOptions, UseAudioSpectrumOptions } from '../src/index.js';

const RATE = 48_000;

function sine(hz: number, seconds: number, amplitude = 1, rate = RATE): DecodedAudio {
  const samples = new Float32Array(Math.round(seconds * rate));
  for (let i = 0; i < samples.length; i += 1) samples[i] = amplitude * Math.sin((2 * Math.PI * hz * i) / rate);
  return { sampleRate: rate, samples };
}

function close(actual: number, expected: number, tolerance: number, message?: string) {
  assert.ok(Math.abs(actual - expected) <= tolerance, `${message ?? ''} expected ${expected} ± ${tolerance}, got ${actual}`);
}

test('a full-scale sine centered on a bin reads 0 dB in a band holding only that bin', () => {
  // 48 kHz / 1024 = 46.875 Hz per bin; bin 21 is 984.375 Hz.
  const hz = 21 * 46.875;
  const spectrum = analyzeAudioSpectrum(sine(hz, 0.5), {
    fftSize: 1024,
    bands: 1,
    scale: 'linear',
    minFrequency: hz - 1,
    maxFrequency: hz + 1,
    minDecibels: -60,
    maxDecibels: 6,
  });
  close(spectrumAt(spectrum, 0.25).bands[0], 60 / 66, 1e-6);
});

test('the loudest band holds the tone, and RMS reads about 0.707 for a full-scale sine', () => {
  const spectrum = analyzeAudioSpectrum(sine(1000, 1), { bands: 16, minDecibels: -100, maxDecibels: 0 });
  const frame = spectrumAt(spectrum, 0.5);
  const loudest = frame.bands.indexOf(Math.max(...frame.bands));
  assert.ok(spectrum.bandEdges[loudest] <= 1000 && 1000 < spectrum.bandEdges[loudest + 1]);
  // 2048 samples hold 42⅔ cycles, so the RMS is not exactly 1/√2.
  close(frame.rms, Math.SQRT1_2, 2e-3);
  close(spectrumAt(analyzeAudioSpectrum(sine(1000, 1, 0.5)), 0.5).rms, Math.SQRT1_2 / 2, 1e-3);
});

test('by default the loudest band level in the file reads 1, however loud the file is', () => {
  // A tone, then the same tone 20 dB louder.
  const quiet = sine(1000, 1, 0.01);
  const samples = new Float32Array(quiet.samples.length * 2);
  samples.set(quiet.samples);
  samples.set(quiet.samples.map((sample) => sample * 10), quiet.samples.length);
  const audio = { sampleRate: RATE, samples };
  const loudest = (spectrum: AudioSpectrum, seconds: number) => Math.max(...spectrumAt(spectrum, seconds).bands);
  // The louder tone's level in dB, read through a fixed range of 0 to -200 dB.
  const peak = -200 + 200 * loudest(analyzeAudioSpectrum(audio, { bands: 16, minDecibels: -200, maxDecibels: 0 }), 1.5);

  const spectrum = analyzeAudioSpectrum(audio, { bands: 16 });
  assert.deepEqual(analyzeAudioSpectrum(audio, { bands: 16, maxDecibels: 'auto' }), spectrum);
  close(loudest(spectrum, 1.5), 1, 1e-6);
  close(loudest(spectrum, 0.5), (peak - 20 + 80) / (peak + 80), 1e-4);
  // Ten times louder throughout: the peak still reads 1, and only the peak.
  const louder = analyzeAudioSpectrum({ sampleRate: RATE, samples: samples.map((sample) => sample * 10) }, { bands: 16 });
  close(loudest(louder, 1.5), 1, 1e-6);
  close(loudest(louder, 0.5), (peak + 80) / (peak + 100), 1e-4);
  // Below minDecibels throughout: zeros, as with a fixed ceiling.
  const faint = analyzeAudioSpectrum(sine(1000, 1, 1e-6), { bands: 16 });
  assert.deepEqual(spectrumAt(faint, 0.5).bands, new Array(16).fill(0));
});

test('silence and times outside the file read 0', () => {
  const silence = analyzeAudioSpectrum({ sampleRate: RATE, samples: new Float32Array(RATE) });
  assert.deepEqual(spectrumAt(silence, 0.5), { bands: new Array(32).fill(0), rms: 0 });
  const tone = analyzeAudioSpectrum(sine(440, 1));
  for (const seconds of [-0.01, 1.01, Number.NaN]) {
    assert.deepEqual(spectrumAt(tone, seconds), { bands: new Array(32).fill(0), rms: 0 });
  }
});

test('the same samples always give the same values, whatever order frames are read in', () => {
  const audio = sine(220, 2, 0.8);
  const options: AudioSpectrumOptions = { bands: 12, smoothing: { attack: 0.02, release: 0.3 } };
  const a = analyzeAudioSpectrum(audio, options);
  const b = analyzeAudioSpectrum(audio, options);
  assert.deepEqual(a, b);
  const frames = [0, 59, 12, 30, 1, 45];
  const shuffled = new Map(frames.map((frame) => [frame, spectrumAt(a, frame / 30)]));
  for (const frame of [...frames].sort((x, y) => x - y)) {
    assert.deepEqual(spectrumAt(b, frame / 30), shuffled.get(frame));
  }
});

test('values between hops are interpolated linearly', () => {
  const spectrum = analyzeAudioSpectrum(sine(300, 1), { hopSeconds: 0.1, bands: 4 });
  const a = spectrumAt(spectrum, 0.3);
  const b = spectrumAt(spectrum, 0.4);
  const middle = spectrumAt(spectrum, 0.35);
  close(middle.rms, (a.rms + b.rms) / 2, 1e-6);
  middle.bands.forEach((level, band) => close(level, (a.bands[band] + b.bands[band]) / 2, 1e-6));
});

test('smoothing rises with the attack time and falls with the release time', () => {
  // One second of tone, then one of silence. 32 samples hold two whole
  // cycles of 3 kHz, so the tone's RMS is steady.
  const samples = new Float32Array(2 * RATE);
  samples.set(sine(3000, 1).samples);
  const raw = analyzeAudioSpectrum({ sampleRate: RATE, samples }, { fftSize: 32 });
  const smoothed = analyzeAudioSpectrum({ sampleRate: RATE, samples }, { fftSize: 32, smoothing: { attack: 0.05, release: 0.2 } });
  const peak = spectrumAt(raw, 0.5).rms;
  close(spectrumAt(raw, 1.2).rms, 0, 1e-9, 'unsmoothed silence');
  // About 63% of the way after one time constant (a little more: the first
  // window, half before the file, already reads half the tone).
  close(spectrumAt(smoothed, 0.05).rms, peak * (1 - Math.exp(-1)), 0.02, 'attack');
  close(spectrumAt(smoothed, 1.2).rms, peak * Math.exp(-1), 0.01, 'release');
  // A single number sets both.
  const both = analyzeAudioSpectrum({ sampleRate: RATE, samples }, { fftSize: 32, smoothing: 0.2 });
  close(spectrumAt(both, 0.2).rms, peak * (1 - Math.exp(-1)), 0.01, 'symmetric attack');
});

test('band edges follow the scale, and the top edge stays below Nyquist', () => {
  const audio = sine(100, 0.1, 1, 8000);
  const log = analyzeAudioSpectrum(audio, { bands: 3, minFrequency: 100, maxFrequency: 2700 }).bandEdges;
  [100, 300, 900, 2700].forEach((hz, i) => close(log[i], hz, 1e-6));
  assert.deepEqual(analyzeAudioSpectrum(audio, { bands: 4, scale: 'linear', minFrequency: 0, maxFrequency: 4000 }).bandEdges, [0, 1000, 2000, 3000, 4000]);
  const mel = analyzeAudioSpectrum(audio, { bands: 2, scale: 'mel', minFrequency: 0, maxFrequency: 4000 }).bandEdges;
  // Halfway on the mel scale: 2595·log10(1 + 4000/700) / 2 mel.
  close(mel[1], 700 * (10 ** (Math.log10(1 + 4000 / 700) / 2) - 1), 1e-6);
  // The default 16 kHz is above this file's 4 kHz Nyquist frequency.
  assert.equal(analyzeAudioSpectrum(audio).bandEdges.at(-1), 4000);
});

test('bands narrower than a bin still read the spectrum at their frequency', () => {
  // 93.75 Hz bins; the bands around 200 Hz are about 23 Hz wide.
  const spectrum = analyzeAudioSpectrum(sine(200, 1), {
    fftSize: 512,
    bands: 40,
    minFrequency: 20,
    maxFrequency: 2000,
    // Wide enough that no band clips at 1.
    minDecibels: -100,
    maxDecibels: 10,
  });
  const frame = spectrumAt(spectrum, 0.5);
  assert.ok(frame.bands.every((level) => Number.isFinite(level) && level >= 0 && level <= 1));
  const loudest = frame.bands.indexOf(Math.max(...frame.bands));
  close(Math.sqrt(spectrum.bandEdges[loudest] * spectrum.bandEdges[loudest + 1]), 200, 25);
});

test('options are checked', () => {
  const audio = sine(440, 0.1);
  assert.throws(() => analyzeAudioSpectrum(audio, { fftSize: 1000 }), /fftSize must be a power of two/);
  assert.throws(() => analyzeAudioSpectrum(audio, { bands: 0 }), /bands must be a positive integer/);
  // Deliberately not a FrequencyScale.
  assert.throws(() => analyzeAudioSpectrum(audio, { scale: 'bark' } as unknown as AudioSpectrumOptions), /scale must be/);
  assert.throws(() => analyzeAudioSpectrum(audio, { minFrequency: 0 }), /minFrequency must be positive on the log scale/);
  assert.throws(() => analyzeAudioSpectrum(audio, { minFrequency: 30000 }), /must be above minFrequency/);
  assert.throws(() => analyzeAudioSpectrum(audio, { minDecibels: -10, maxDecibels: -20 }), /minDecibels/);
  assert.throws(() => analyzeAudioSpectrum(audio, { minDecibels: Number.NaN }), /minDecibels must be a finite number/);
  assert.throws(() => analyzeAudioSpectrum(audio, { smoothing: { attack: -1, release: 0 } }), /smoothing attack/);
  assert.throws(() => analyzeAudioSpectrum(audio, { hopSeconds: 0 }), /hopSeconds must be positive/);
  assert.throws(() => analyzeAudioSpectrum({ sampleRate: 0, samples: new Float32Array(1) }), /positive sample rate/);
});

test('loadAudioSpectrum decodes once per file and keeps the src for <Audio>', async () => {
  const requests: string[] = [];
  setAudioDecoder(async (path) => {
    requests.push(path);
    return sine(440, 0.5);
  });
  const a = await loadAudioSpectrum('https://example.com/song.mp3', { bands: 8 });
  const b = await loadAudioSpectrum('https://example.com/song.mp3', { bands: 4 });
  assert.equal(a.src, 'https://example.com/song.mp3');
  assert.equal(a.bandEdges.length, 9);
  assert.equal(b.bandEdges.length, 5);
  assert.deepEqual(requests, ['https://example.com/song.mp3']);
});

test('useAudioSpectrum reads the enclosing Sequence clock, offset by startFrom', () => {
  const spectrum = analyzeAudioSpectrum(sine(440, 4), { bands: 2, hopSeconds: 0.25 });
  const h = React.createElement;
  function Meter({ startFrom }: UseAudioSpectrumOptions) {
    const { rms, bands } = useAudioSpectrum(spectrum, { startFrom });
    return h(Rect, { width: rms * 1000, height: bands[1] * 1000, fill: '#ffffff' });
  }
  function Root() {
    return h(
      Composition,
      { width: 100, height: 100, fps: 4, durationInFrames: 16 },
      h(Sequence, { from: 4 }, h(Meter, { startFrom: 1 })),
    );
  }
  const mounted = mount(Root);
  const rect = (layers: Layer[]): Extract<LayerContent, { type: 'rect' }> | undefined => {
    for (const layer of layers) {
      if (layer.content.type === 'rect') return layer.content;
      const found = layer.content.type === 'group' ? rect(layer.content.layers) : undefined;
      if (found) return found;
    }
  };
  // Frame 6 is the Sequence's frame 2: half a second in, 1.5 s into the file.
  const found = rect(mounted.renderAt({ value: 6, timescale: 4 }, null).scene.layers);
  assert.ok(found, 'a rect layer');
  const { width, height } = found;
  const expected = spectrumAt(spectrum, 1.5);
  assert.ok(expected.rms > 0.5);
  assert.equal(width, expected.rms * 1000);
  assert.equal(height, expected.bands[1] * 1000);
});
