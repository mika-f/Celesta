// How closely Web Audio follows keyframed volume and playback rate in
// browser export. Run `pnpm --dir packages/web exec vite` and open
// /test/manual/audio-automation.html in the browser to check.
import { evaluateAnimated, integrateAnimated } from '@celesta.studio/react/internal/animated-number';
import { scheduleClip } from '../../src/audio-support';
import type { AnimatedNumber, AudioClip, KeyframeEasing } from '../../src/types';

const RATE = 48_000;
const key = (time: number, value: number, easing?: KeyframeEasing) => ({ time: { value: Math.round(time * 1_000_000), timescale: 1_000_000 }, value, ...(easing ? { easing } : {}) });
const clip = (fields: Partial<AudioClip>): AudioClip => ({ src: 'check', sourceStart: 0, playbackRate: 1, volume: 1, muted: false, start: 0, duration: 0, ...fields });

/** A constant source of 1, so each output sample is the gain at that sample. */
async function measureVolume() {
  const seconds = 4, cut = 2;
  const context = new OfflineAudioContext(2, seconds * RATE, RATE);
  const buffer = context.createBuffer(2, seconds * RATE, RATE);
  for (let channel = 0; channel < 2; channel++) buffer.getChannelData(channel).fill(1);
  const volume: AnimatedNumber = { type: 'keyframes', keyframes: [key(0, 1), key(0.5, 0.3, 'ease-in-out'), key(cut, 0.3), key(cut, 0.8), key(3, 1, 'ease-in-out-cubic')] };
  scheduleClip(context, clip({ volume, duration: seconds }), buffer, seconds);
  const output = (await context.startRendering()).getChannelData(0);
  let maxError = 0, maxStep = 0, maxExactStep = 0;
  for (let index = 1; index < output.length; index++) {
    // The cut itself lands on one side of a sample or the other.
    if (Math.abs(index - cut * RATE) <= 2) continue;
    const exact = evaluateAnimated(volume, index / RATE);
    maxError = Math.max(maxError, Math.abs(output[index] - exact));
    maxStep = Math.max(maxStep, Math.abs(output[index] - output[index - 1]));
    maxExactStep = Math.max(maxExactStep, Math.abs(exact - evaluateAnimated(volume, (index - 1) / RATE)));
  }
  return { maxError, maxStep, maxExactStep };
}

/** Pulses every 0.05 s of source time, found again in the output and timed against the integrated rate. */
async function measureRate() {
  const sourceSeconds = 12, renderSeconds = 10, spacing = 0.05, width = 41;
  const context = new OfflineAudioContext(2, renderSeconds * RATE, RATE);
  const buffer = context.createBuffer(2, sourceSeconds * RATE, RATE);
  const centers: number[] = [];
  for (let pulse = 1; pulse * spacing < sourceSeconds - 1; pulse++) {
    const center = Math.round(pulse * spacing * RATE);
    centers.push(center / RATE);
    for (let offset = -(width >> 1); offset <= width >> 1; offset++) {
      // A Hann pulse, wide enough to survive resampling at any of the rates below.
      const sample = 0.5 + 0.5 * Math.cos((Math.PI * offset) / ((width >> 1) + 1));
      for (let channel = 0; channel < 2; channel++) buffer.getChannelData(channel)[center + offset] = sample;
    }
  }
  const rate: AnimatedNumber = { type: 'keyframes', keyframes: [key(0, 1), key(4, 2, 'linear'), key(8, 0.5, 'ease-in-out')] };
  scheduleClip(context, clip({ playbackRate: rate, duration: renderSeconds }), buffer, renderSeconds);
  const output = (await context.startRendering()).getChannelData(0);
  const found: number[] = [];
  for (let index = 0; index < output.length; index++) {
    if (output[index] <= 0.05) continue;
    let weight = 0, moment = 0;
    for (; index < output.length && output[index] > 0.05; index++) {
      weight += output[index];
      moment += output[index] * index;
    }
    found.push(moment / weight / RATE);
  }
  const expected = (source: number) => {
    let low = 0, high = renderSeconds;
    for (let step = 0; step < 60; step++) {
      const middle = (low + high) / 2;
      if (integrateAnimated(rate, middle) < source) low = middle;
      else high = middle;
    }
    return (low + high) / 2;
  };
  let maxErrorMs = 0, compared = 0;
  for (let pulse = 0; pulse < found.length && pulse < centers.length; pulse++) {
    const time = expected(centers[pulse]);
    if (time >= renderSeconds - 0.01) break;
    maxErrorMs = Math.max(maxErrorMs, Math.abs(found[pulse] - time) * 1000);
    compared++;
  }
  return { pulsesCompared: compared, maxErrorMs };
}

const result = { userAgent: navigator.userAgent, volume: await measureVolume(), rate: await measureRate() };
console.log(JSON.stringify(result));
document.getElementById('result')!.textContent = JSON.stringify(result, null, 2);
