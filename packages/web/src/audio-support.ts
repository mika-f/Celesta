import { checkDuckGroup, duckEnvelope, duckGroups } from '@celesta.studio/media-utils/duck';
import type { AnimatedNumber, AudioClip } from './types';
import { type AutomationEvent, automationEvents, integrateAnimated } from './keyframes';

const RATE_MESSAGE = 'Audio playback rate must be positive.';

/** Throws when the Web Audio export cannot reproduce the clip faithfully. */
export function assertBrowserMixable(clip: AudioClip): void {
  if (typeof clip.playbackRate === 'number' && !(clip.playbackRate > 0)) throw new Error(RATE_MESSAGE);
  if ((clip.pitch !== undefined && clip.pitch !== 0) || (clip.preservePitch === true && clip.playbackRate !== 1)) {
    throw new Error('Audio pitch and preservePitch are not supported by browser export yet.');
  }
}

/**
 * The decoded source of each clip (`undefined` for a muted one), each `src`
 * decoded once. Every source is decoded, as a silent voice still marks
 * speech for ducking, but every audible clip is checked with
 * `assertBrowserMixable` before the first is fetched.
 */
export async function decodeSources<T>(clips: AudioClip[], decode: (src: string) => Promise<T>): Promise<(T | undefined)[]> {
  for (const clip of clips) {
    if (!clip.muted && clip.volume !== 0) assertBrowserMixable(clip);
  }
  const decoded = new Map<string, T>();
  const sources: (T | undefined)[] = [];
  for (const clip of clips) {
    if (clip.muted) {
      sources.push(undefined);
      continue;
    }
    let source = decoded.get(clip.src);
    if (source === undefined) {
      source = await decode(clip.src);
      decoded.set(clip.src, source);
    }
    sources.push(source);
  }
  return sources;
}

/** Schedules `events` from `automationEvents` on `param`. */
export function applyEvents(param: Pick<AudioParam, 'setValueAtTime' | 'linearRampToValueAtTime'>, events: AutomationEvent[]): void {
  for (const event of events) {
    if (event.kind === 'set') param.setValueAtTime(event.value, event.time);
    else param.linearRampToValueAtTime(event.value, event.time);
  }
}

/**
 * Timeline seconds at which `clip` stops sounding: the end of its range, or
 * earlier, unless it loops, where its source position reaches the end of its
 * `sourceSeconds` long source. As `audible_end` in `crates/media`.
 */
export function audibleEnd(clip: AudioClip, sourceSeconds: number): number {
  const end = clip.start + clip.duration;
  if (clip.loop === true) return end;
  const remaining = sourceSeconds - clip.sourceStart;
  if (remaining <= 0) return clip.start;
  const rate = clip.playbackRate;
  let local: number;
  if (typeof rate === 'number') {
    if (!(rate > 0)) return end;
    local = remaining / rate;
  } else {
    if (integrateAnimated(rate, clip.duration) < remaining) return end;
    let low = 0, high = clip.duration;
    for (let step = 0; step < 64; step++) {
      const middle = (low + high) / 2;
      if (integrateAnimated(rate, middle) < remaining) low = middle;
      else high = middle;
    }
    local = high;
  }
  return Math.min(clip.start + local, end);
}

/** Runs `check`, naming `clip`'s `src` in what it throws, as the native bridge does. */
function namingSrc<T>(clip: AudioClip, check: () => T): T {
  try {
    return check();
  } catch (error) {
    throw new Error(`audio \`${clip.src}\`: ${error instanceof Error ? error.message : String(error)}`, { cause: error });
  }
}

/**
 * The ducking curve of each clip (`undefined` for one without `duck`), on
 * the clip's own clock, from the intervals where the clips of its groups
 * sound; `sourceSeconds` holds each clip's source length. Muted clips do not
 * count, nor does a clip toward its own curve. As `duck_curves` in
 * `crates/composition`.
 */
export function duckCurves(clips: AudioClip[], sourceSeconds: number[]): (AnimatedNumber | undefined)[] {
  for (const clip of clips) {
    const group = clip.duckGroup;
    if (group !== undefined) namingSrc(clip, () => checkDuckGroup(group));
  }
  return clips.map((clip, index) => {
    const duck = clip.duck;
    if (!duck) return undefined;
    const under = namingSrc(clip, () => duckGroups(duck.under));
    const intervals: [number, number][] = [];
    clips.forEach((other, at) => {
      if (at === index || other.muted || other.duckGroup === undefined || !under.includes(other.duckGroup)) return;
      intervals.push([other.start, audibleEnd(other, sourceSeconds[at])]);
    });
    return namingSrc(clip, () => duckEnvelope(intervals, duck, { origin: clip.start }));
  });
}

/**
 * Plays `clip` from `buffer` into `context`'s destination: its playback rate
 * and volume follow their keyframes (volume clamped to 0–16, as in the native
 * mixer), its fades and then `duck`, its ducking curve, chain after the
 * volume, and it stops at the clip's end on the timeline or at `total`, the
 * mix's length in seconds.
 */
export function scheduleClip(
  context: Pick<BaseAudioContext, 'createBufferSource' | 'createGain' | 'destination'>,
  clip: AudioClip,
  buffer: AudioBuffer,
  total: number,
  duck?: AnimatedNumber,
): void {
  assertBrowserMixable(clip);
  const begin = Math.max(0, clip.start);
  const end = clip.start + clip.duration;
  if (clip.start >= total || end <= begin) return;
  const rate = automationEvents(clip.playbackRate, clip.start, end);
  // Web Audio would play a negative rate backwards; the native mixer plays silence.
  if (rate.some((event) => !(event.value > 0))) throw new Error(RATE_MESSAGE);
  const source = context.createBufferSource();
  source.buffer = buffer;
  applyEvents(source.playbackRate, rate);
  // Looping starts the source over from its beginning past its end.
  source.loop = clip.loop === true;
  const offset = source.loop ? ((clip.sourceStart % buffer.duration) + buffer.duration) % buffer.duration : clip.sourceStart;
  const gain = context.createGain();
  applyEvents(gain.gain, automationEvents(clip.volume, clip.start, end, { min: 0, max: 16 }));
  let output: AudioNode = source.connect(gain);
  for (const fade of fadeGains(context, clip)) output = output.connect(fade);
  if (duck !== undefined) {
    const ducking = context.createGain();
    applyEvents(ducking.gain, automationEvents(duck, clip.start, end));
    output = output.connect(ducking);
  }
  output.connect(context.destination);
  // Stopping on the timeline holds for any rate, where start()'s duration
  // argument is in source seconds and only right for a constant rate.
  source.start(begin, offset);
  source.stop(Math.min(end, total));
}

/**
 * Gain nodes for the clip's fade-in and fade-out, to chain after its volume.
 * Each fade is a node of its own, so fades that overlap multiply as they do
 * in the native mixer.
 */
export function fadeGains(context: Pick<BaseAudioContext, 'createGain'>, clip: AudioClip): GainNode[] {
  const end = clip.start + clip.duration;
  const gains: GainNode[] = [];
  if (clip.fadeIn) gains.push(ramp(context, clip.start, clip.start + clip.fadeIn, 0, 1));
  if (clip.fadeOut) gains.push(ramp(context, end - clip.fadeOut, end, 1, 0));
  return gains;
}

/** A gain moving linearly from `fromValue` at `from` seconds to `toValue` at `to`. */
function ramp(context: Pick<BaseAudioContext, 'createGain'>, from: number, to: number, fromValue: number, toValue: number): GainNode {
  const node = context.createGain();
  // Automation cannot start before 0, so a ramp under way by then starts
  // from where it has got to.
  const start = Math.max(0, from);
  node.gain.setValueAtTime(fromValue + (toValue - fromValue) * Math.min(1, (start - from) / (to - from)), start);
  if (to > start) node.gain.linearRampToValueAtTime(toValue, to);
  return node;
}
