// Mixes a score's events into stereo PCM and encodes it as WAV.

import { PI, cos, hashString, midiToHz, random, semitoneRatio, sin, softClip } from './dmath.js';
import { SAMPLE_RATE } from './dsp.js';
import type { ScoreEvent, ScoreMaster } from './score.js';
import { DRUMS, EFFECTS, PATCHES, WHOOSH_INTO } from './voices.js';
import type { DrumName, EffectName, PatchName, Rendered } from './voices.js';

const SQRT1_2 = 0.7071067811865476;

export interface RenderInput {
  events: readonly ScoreEvent[];
  /** Length of the output in samples. */
  samples: number;
  master: Required<ScoreMaster>;
  seed: number;
}

export interface StereoBuffer {
  left: Float64Array;
  right: Float64Array;
}

function renderEvent(event: ScoreEvent, rnd: () => number): Rendered {
  switch (event.kind) {
    case 'note': {
      const hold = Math.max(1, Math.round((event.length ?? 0) * SAMPLE_RATE));
      const data = PATCHES[event.name as PatchName]({ freq: midiToHz(event.midi ?? 60), hold, bright: event.bright, rnd });
      return { data, anchor: 0 };
    }
    case 'drum':
      return { data: DRUMS[event.name as DrumName]({ ratio: semitoneRatio(event.pitch), dur: undefined, rnd }), anchor: 0 };
    case 'fx': {
      const input = { ratio: semitoneRatio(event.pitch), dur: event.length, rnd };
      return event.reverse ? WHOOSH_INTO(input) : EFFECTS[event.name as EffectName](input);
    }
  }
}

/** Each event's own noise seed: from what it is and where, so adding an event elsewhere changes nothing here. */
function eventSeeds(events: readonly ScoreEvent[], seed: number): number[] {
  const seen = new Map<string, number>();
  return events.map(event => {
    const id = `${event.kind}|${event.name}|${event.reverse ? 1 : 0}|${Math.round(event.time * SAMPLE_RATE)}`;
    const count = seen.get(id) ?? 0;
    seen.set(id, count + 1);
    return hashString(`${id}|${count}`, hashString(String(seed)));
  });
}

/** Renders every event and the reverb into a stereo buffer at full scale (before the master). */
export function mix(input: RenderInput): StereoBuffer {
  const { events, samples } = input;
  const left = new Float64Array(samples);
  const right = new Float64Array(samples);
  const send = new Float64Array(samples);
  const seeds = eventSeeds(events, input.seed);
  events.forEach((event, index) => {
    const { data, anchor } = renderEvent(event, random(seeds[index]));
    const at = Math.round(event.time * SAMPLE_RATE) - anchor;
    const amplitude = event.vel * event.gain;
    const angle = ((event.pan + 1) * PI) / 4;
    const gl = (amplitude * cos(angle)) / SQRT1_2;
    const gr = (amplitude * sin(angle)) / SQRT1_2;
    const gs = amplitude * event.reverb;
    const from = Math.max(0, -at);
    const to = Math.min(data.length, samples - at);
    for (let i = from; i < to; i += 1) {
      const x = data[i];
      left[at + i] += gl * x;
      right[at + i] += gr * x;
      send[at + i] += gs * x;
    }
  });
  if (input.master.reverb > 0) reverb(send, left, right, input.master.reverb);
  return { left, right };
}

// Freeverb's tunings at 44.1 kHz, scaled to 48 kHz; the right channel is spread by 23 samples.
const COMBS = [1557, 1617, 1491, 1422, 1277, 1356, 1188, 1116].map(n => Math.round((n * 48000) / 44100));
const ALLPASSES = [556, 441, 341, 225].map(n => Math.round((n * 48000) / 44100));
const SPREAD = 25;

class Comb {
  private line: Float64Array;
  private at = 0;
  private store = 0;

  constructor(length: number, private feedback: number, private damp: number) {
    this.line = new Float64Array(length);
  }

  process(x: number): number {
    const y = this.line[this.at];
    this.store = y * (1 - this.damp) + this.store * this.damp;
    this.line[this.at] = x + this.store * this.feedback;
    this.at = this.at + 1 === this.line.length ? 0 : this.at + 1;
    return y;
  }
}

class Allpass {
  private line: Float64Array;
  private at = 0;

  constructor(length: number) {
    this.line = new Float64Array(length);
  }

  process(x: number): number {
    const delayed = this.line[this.at];
    this.line[this.at] = x + delayed * 0.5;
    this.at = this.at + 1 === this.line.length ? 0 : this.at + 1;
    return delayed - x;
  }
}

/** A Freeverb-style room on the send, added to both channels. */
function reverb(send: Float64Array, left: Float64Array, right: Float64Array, level: number): void {
  const channels = [
    { out: left, combs: COMBS.map(n => new Comb(n, 0.82, 0.24)), passes: ALLPASSES.map(n => new Allpass(n)) },
    { out: right, combs: COMBS.map(n => new Comb(n + SPREAD, 0.82, 0.24)), passes: ALLPASSES.map(n => new Allpass(n + SPREAD)) },
  ];
  const wet = 0.05 * level;
  for (const { out, combs, passes } of channels) {
    for (let i = 0; i < send.length; i += 1) {
      const x = send[i];
      let y = 0;
      for (const comb of combs) y += comb.process(x);
      for (const pass of passes) y = pass.process(y);
      out[i] += wet * y;
    }
  }
}

/** Applies the master gain and fades, limits, and quantizes to interleaved 16-bit samples. */
export function master(buffer: StereoBuffer, settings: Required<ScoreMaster>): Int16Array {
  const { left, right } = buffer;
  const n = left.length;
  const out = new Int16Array(n * 2);
  const fadeIn = Math.round(settings.fadeIn * SAMPLE_RATE);
  const fadeOut = Math.round(settings.fadeOut * SAMPLE_RATE);
  for (let i = 0; i < n; i += 1) {
    let g = settings.gain;
    if (i < fadeIn) g *= i / fadeIn;
    if (n - i <= fadeOut) g *= (n - 1 - i) / fadeOut;
    out[2 * i] = quantize(softClip(left[i] * g, 0.85));
    out[2 * i + 1] = quantize(softClip(right[i] * g, 0.85));
  }
  return out;
}

function quantize(x: number): number {
  const v = Math.round(x * 32767);
  return v > 32767 ? 32767 : v < -32768 ? -32768 : v;
}

/** A 16-bit PCM WAV file of interleaved stereo samples. */
export function encodeWav(samples: Int16Array, channels = 2, rate = SAMPLE_RATE): Uint8Array {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const view = new DataView(bytes.buffer);
  const ascii = (at: number, text: string) => {
    for (let i = 0; i < text.length; i += 1) view.setUint8(at + i, text.charCodeAt(i));
  };
  ascii(0, 'RIFF');
  view.setUint32(4, 36 + samples.length * 2, true);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, channels, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * channels * 2, true);
  view.setUint16(32, channels * 2, true);
  view.setUint16(34, 16, true);
  ascii(36, 'data');
  view.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i += 1) view.setInt16(44 + i * 2, samples[i], true);
  return bytes;
}
