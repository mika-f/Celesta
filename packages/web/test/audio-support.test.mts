import assert from 'node:assert/strict';
import { test } from 'vitest';
import { duckEnvelope } from '@celesta.studio/media-utils/duck';
import { assertBrowserMixable, audibleEnd, decodeSources, duckCurves, fadeGains, scheduleClip } from '../src/audio-support.js';
import { automationEvents } from '../src/keyframes.js';
import type { AutomationEvent } from '../src/keyframes.js';
import type { AnimatedNumber, AudioClip, Keyframe, KeyframeEasing } from '../src/types.js';

const base: AudioClip = { src: 'a.wav', sourceStart: 0, playbackRate: 1, volume: 1, muted: false, start: 0, duration: 1 };
const message = /Audio pitch and preservePitch are not supported by browser export yet\./;

function key(time: number, value: number, easing?: KeyframeEasing): Keyframe {
  return { time: { value: Math.round(time * 1_000_000), timescale: 1_000_000 }, value, ...(easing ? { easing } : {}) };
}
function keys(...list: [time: number, value: number, easing?: KeyframeEasing][]): AnimatedNumber {
  return { type: 'keyframes', keyframes: list.map(([time, value, easing]) => key(time, value, easing)) };
}

test('plain clips, and preservePitch at rate 1, are mixable', () => {
  assertBrowserMixable(base);
  assertBrowserMixable({ ...base, preservePitch: true });
  assertBrowserMixable({ ...base, pitch: 0 });
});
test('pitch or preservePitch with a rate change is rejected', () => {
  assert.throws(() => assertBrowserMixable({ ...base, pitch: 2 }), message);
  assert.throws(() => assertBrowserMixable({ ...base, pitch: { type: 'keyframes', keyframes: [] } }), message);
  assert.throws(() => assertBrowserMixable({ ...base, playbackRate: 2, preservePitch: true }), message);
});
test('decodeSources decodes each src once and skips muted clips', async () => {
  const fetched: string[] = [];
  const decode = async (src: string) => {
    fetched.push(src);
    return `decoded ${src}`;
  };
  const clips = [base, { ...base, src: 'b.wav', muted: true }, { ...base, start: 2 }, { ...base, src: 'c.wav', volume: 0 }];
  assert.deepEqual(await decodeSources(clips, decode), ['decoded a.wav', undefined, 'decoded a.wav', 'decoded c.wav']);
  assert.deepEqual(fetched, ['a.wav', 'c.wav']);
});
test('decodeSources rejects an unmixable clip before fetching any source', async () => {
  const fetched: string[] = [];
  const decode = async (src: string) => {
    fetched.push(src);
    return src;
  };
  await assert.rejects(decodeSources([base, { ...base, src: 'b.wav', pitch: 2 }], decode), message);
  assert.deepEqual(fetched, []);
  // A silent or muted clip is never played, so it is not checked.
  await decodeSources([{ ...base, pitch: 2, volume: 0 }, { ...base, pitch: 2, muted: true }], decode);
});
test('keyframed volume and rate are mixable', () => {
  assertBrowserMixable({ ...base, volume: keys([0, 0], [1, 1, 'linear']) });
  assertBrowserMixable({ ...base, playbackRate: keys([0, 1], [1, 2, 'linear']) });
  assert.throws(() => assertBrowserMixable({ ...base, playbackRate: 0 }), /playback rate must be positive/);
});
test('preservePitch with a keyframed rate is rejected', () => {
  assert.throws(() => assertBrowserMixable({ ...base, playbackRate: keys([0, 1], [1, 1, 'linear']), preservePitch: true }), message);
});

type Recorded = [kind: 'set' | 'ramp', value: number, time: number];
interface RecordingGain { events: Recorded[] }
interface RecordingSource { events: Recorded[]; calls: unknown[][] }

/**
 * A context whose params record their automation as `[method, value, time]`
 * and whose sources record their `start` / `stop` calls.
 */
function recording() {
  const param = (events: Recorded[]) => ({
    setValueAtTime: (value: number, time: number) => events.push(['set', value, time]),
    linearRampToValueAtTime: (value: number, time: number) => events.push(['ramp', value, time]),
  });
  const gains: RecordingGain[] = [];
  const sources: RecordingSource[] = [];
  const context = {
    gains,
    sources,
    destination: {},
    createGain() {
      const node = { events: [] as Recorded[], connect: (next: unknown) => next };
      gains.push(node);
      return { ...node, gain: param(node.events) };
    },
    createBufferSource() {
      const node = {
        events: [] as Recorded[], calls: [] as unknown[][], buffer: null, loop: false,
        connect: (next: unknown) => next,
        start: (...args: unknown[]) => node.calls.push(['start', ...args]),
        stop: (...args: unknown[]) => node.calls.push(['stop', ...args]),
      };
      sources.push(node);
      return Object.assign(node, { playbackRate: param(node.events) });
    },
  };
  // A recording stand-in for the few members of an AudioContext the code uses.
  return context as typeof context & Pick<BaseAudioContext, 'createBufferSource' | 'createGain' | 'destination'>;
}
const recorded = (events: AutomationEvent[]): Recorded[] => events.map(({ kind, value, time }) => [kind, value, time]);
// Only `duration` is read from the buffer.
const buffer = { duration: 10 } as AudioBuffer;

test('a clip without fades gets no fade gains', () => {
  assert.deepEqual(fadeGains(recording(), base), []);
  assert.deepEqual(fadeGains(recording(), { ...base, fadeIn: 0, fadeOut: 0 }), []);
});
test('fades ramp linearly at the edges of the clip, one gain each', () => {
  const context = recording();
  fadeGains(context, { ...base, start: 2, duration: 10, fadeIn: 1.5, fadeOut: 3 });
  assert.deepEqual(context.gains.map((gain) => gain.events), [
    [['set', 0, 2], ['ramp', 1, 3.5]],
    [['set', 1, 9], ['ramp', 0, 12]],
  ]);
});
test('a fade under way at 0 starts from where it has got to', () => {
  const context = recording();
  fadeGains(context, { ...base, start: -1, duration: 10, fadeIn: 4 });
  assert.deepEqual(context.gains[0].events, [['set', 0.25, 0], ['ramp', 1, 3]]);
});

test('scheduleClip starts with two arguments and stops at the clip end', () => {
  const clip: AudioClip = { ...base, start: 1, duration: 3, loop: true, playbackRate: keys([0, 1], [2, 2, 'linear']) };
  const context = recording();
  scheduleClip(context, clip, buffer, 10);
  assert.deepEqual(context.sources[0].calls, [['start', 1, 0], ['stop', 4]]);
  const short = recording();
  scheduleClip(short, clip, buffer, 2.5);
  assert.deepEqual(short.sources[0].calls, [['start', 1, 0], ['stop', 2.5]]);
});
test('scheduleClip schedules the rate and volume curves', () => {
  const clip: AudioClip = { ...base, start: 2, duration: 4, playbackRate: keys([0, 1], [1, 1.5, 'ease-in-out']), volume: keys([0, 0.5], [2, 20, 'linear']) };
  const context = recording();
  scheduleClip(context, clip, buffer, 10);
  assert.deepEqual(context.sources[0].events, recorded(automationEvents(clip.playbackRate, 2, 6)));
  assert.deepEqual(context.gains[0].events, recorded(automationEvents(clip.volume, 2, 6, { min: 0, max: 16 })));
});
test('a rate that overshoots below 0 is rejected', () => {
  const clip: AudioClip = { ...base, duration: 2, playbackRate: keys([0, 1], [1, 0.05, 'ease-out-back']) };
  assert.throws(() => scheduleClip(recording(), clip, buffer, 10), /playback rate must be positive/);
});
test('a rate reaching 0 only after the clip ends is played', () => {
  const clip: AudioClip = { ...base, duration: 5, playbackRate: keys([0, 1], [10, 0, 'linear']) };
  const context = recording();
  scheduleClip(context, clip, buffer, 10);
  assert.deepEqual(context.sources[0].events, [['set', 1, 0], ['ramp', 0.5, 5]]);
});
test('a clip after the end of the mix is not scheduled', () => {
  const context = recording();
  scheduleClip(context, { ...base, start: 5 }, buffer, 5);
  assert.equal(context.sources.length, 0);
});

const voice = (start: number, duration: number, fields: Partial<AudioClip> = {}): AudioClip =>
  ({ ...base, src: 'voice.wav', start, duration, duckGroup: 'voice', ...fields });
const bed: AudioClip = { ...base, src: 'bed.wav', duration: 6, duck: { under: 'voice' } };

test('duckCurves gives a ducked clip the curve of the voices, on its own clock', () => {
  const later = { ...bed, start: 1 };
  const curves = duckCurves([voice(2, 2), later], [10, 10]);
  assert.equal(curves[0], undefined);
  assert.deepEqual(curves[1], duckEnvelope([[2, 4]], {}, { origin: 1 }));
});
test('a muted voice and the ducked clip itself do not count', () => {
  const curves = duckCurves([voice(2, 2, { muted: true }), { ...bed, duckGroup: 'voice' }], [10, 10]);
  assert.equal(curves[1], 1);
});
test('a voice counts until its source ends, or for its whole range when it loops', () => {
  assert.equal(audibleEnd(voice(2, 3), 1), 3);
  assert.equal(audibleEnd(voice(2, 3, { loop: true }), 1), 5);
  assert.equal(audibleEnd(voice(2, 3, { sourceStart: 2 }), 1), 2);
  // At a rate going 1 → 2 over 2 s, a 2 s source runs out where t + t² / 4 = 2.
  const rate = keys([0, 1], [2, 2, 'linear']);
  assert.ok(Math.abs(audibleEnd(voice(0, 4, { playbackRate: rate }), 2) - (Math.sqrt(12) - 2)) < 1e-6);
});
test('invalid ducking throws, naming the clip as the native bridge does', () => {
  assert.throws(() => duckCurves([voice(0, 1, { duckGroup: '' }), bed], [1, 1]), /^Error: audio `voice\.wav`: duckGroup must be a non-empty name$/);
  assert.throws(() => duckCurves([{ ...bed, duck: { under: [] } }], [1]), /^Error: audio `bed\.wav`: duck\.under must be a non-empty name$/);
  assert.throws(() => duckCurves([{ ...bed, duck: { under: 'voice', to: 2 } }], [1]), /^Error: audio `bed\.wav`: duck\.to must be a multiplier/);
  assert.throws(() => duckCurves([{ ...bed, duck: { under: 'voice', attack: -1 } }], [1]), /^Error: audio `bed\.wav`: duck\.attack must be finite and not negative$/);
});
test('scheduleClip chains the ducking curve after the fades', () => {
  const curve = duckEnvelope([[2, 4]], {}, { origin: 0 });
  const context = recording();
  scheduleClip(context, { ...bed, fadeIn: 1 }, buffer, 10, curve);
  assert.equal(context.gains.length, 3);
  assert.deepEqual(context.gains[2].events, recorded(automationEvents(curve, 0, 6)));
});
