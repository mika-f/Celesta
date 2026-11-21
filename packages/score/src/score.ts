// The score as data: `defineScore()` runs a builder that records notes, drums,
// and effects against the composition's frames, seconds, and beat grid, and
// checks every name and value as it goes.

import { SAMPLE_RATE } from './dsp.js';
import { parsePitch } from './pitch.js';
import type { Pitch } from './pitch.js';
import { DRUMS, EFFECTS, EFFECTS_WITH_DURATION, PATCHES } from './voices.js';
import type { DrumName, EffectName, PatchName } from './voices.js';

export type { DrumName, EffectName, PatchName } from './voices.js';
export type { Pitch } from './pitch.js';

/** Bumped whenever a voice, the mixer, or the file changes, so cached files are rendered again. */
export const SYNTH_VERSION = 1;

/** When something happens: exactly one of a frame, a time in seconds, or a beat on the grid set by `tempo()`. */
export interface TimeOptions {
  /** Frame of the composition; may be fractional. */
  frame?: number;
  /** Seconds from the start of the composition. */
  t?: number;
  /** Beat on the grid set by `tempo()`, counted from 0; may be fractional. */
  beat?: number;
}

/** How long something lasts: at most one of frames, seconds, or beats. */
export interface LengthOptions {
  frames?: number;
  /** Seconds. */
  dur?: number;
  beats?: number;
}

/** A time: a frame number, or `{ frame }`, `{ t }`, or `{ beat }`. */
export type At = number | TimeOptions;

export interface ProgramOptions {
  /** Linear level of the track. Defaults to 0.7. */
  gain?: number;
  /** -1 (left) to 1 (right). Defaults to 0. */
  pan?: number;
  /** Share sent to the room, 0 to 1. Defaults to 0.2. */
  reverb?: number;
  /** 0 (dull) to 1 (bright). Defaults to 0.5. */
  bright?: number;
}

export interface NoteOptions extends TimeOptions, LengthOptions {
  /** 0 to 1. Defaults to 0.8. */
  vel?: number;
}

export interface RunOptions extends TimeOptions, LengthOptions {
  /** Time between notes, in the unit of the start: frames after `frame`, seconds after `t`, beats after `beat`. */
  step: number;
  /** 0 to 1. Defaults to 0.8. */
  vel?: number;
  /** Added to `vel` over the run, from 0 on the first note to `cresc` on the last; negative fades. Defaults to 0. */
  cresc?: number;
}

export interface PatternOptions extends TimeOptions {
  /** Beats per character. Defaults to 0.5. */
  step?: number;
  /** Times the pattern plays. Defaults to 1. */
  repeat?: number;
  /** The note of every hit, or notes taken in turn, one per hit. Defaults to C4. */
  pitch?: Pitch | readonly Pitch[];
  /** Beats each note lasts. Defaults to `step`. */
  beats?: number;
  /** Velocity of `x`; `X` is louder and `o` softer. Defaults to 0.8. */
  vel?: number;
}

export interface HitOptions {
  /** 0 to 1. Defaults to 0.8. */
  vel?: number;
  /** Semitones up (or down, negative). Defaults to 0. */
  pitch?: number;
  /** -1 (left) to 1 (right). Defaults to 0. */
  pan?: number;
  /** Linear level. Defaults to 1. */
  gain?: number;
  /** Share sent to the room, 0 to 1. Defaults to 0.08 for drums and 0.15 for effects. */
  reverb?: number;
}

export interface DrumOptions extends TimeOptions, HitOptions {}

export interface DrumsOptions extends TimeOptions, Omit<HitOptions, 'pitch'> {
  /** Beats per character. Defaults to 0.5. */
  step?: number;
  /** Times each line plays. Defaults to 1. */
  repeat?: number;
}

export interface FxOptions extends TimeOptions, LengthOptions, HitOptions {
  /** For `whoosh`: swell into the time instead of away from it. */
  reverse?: boolean;
}

export interface TicksOptions extends HitOptions {
  /** The effect each time plays. Defaults to `tick`. */
  sound?: EffectName;
  /** Semitones each tick's pitch may wander, at random but the same every run. Defaults to 0. */
  vary?: number;
}

export interface ScoreMaster {
  /** Linear level of the whole mix. Defaults to 0.8. */
  gain?: number;
  /** Seconds the mix rises from silence at the start. Defaults to 0. */
  fadeIn?: number;
  /** Seconds the mix falls to silence at the end. Defaults to 0. */
  fadeOut?: number;
  /** Level of the room every send goes to, 0 to 1. Defaults to 1. */
  reverb?: number;
}

/** The grid `beat` times are on; the same options `useBeat()` and `beatAt()` take. */
export interface ScoreTempo {
  bpm: number;
  /** Beats in one bar. Defaults to 4. */
  beatsPerBar?: number;
  /** Frame beat 0 lands on. Defaults to 0. */
  offset?: number;
}

/** What `defineScore()` builds a score with. */
export interface ScoreBuilder {
  /** Sets the beat grid for the calls after it. */
  tempo(tempo: ScoreTempo | number): void;
  /** The frame a beat lands on, on the current grid. */
  beatFrame(beat: number): number;
  /** Sets the master level and fades. */
  master(settings: ScoreMaster): void;
  /** Gives a track a patch. Calling it again changes the notes after it. */
  program(track: string, patch: PatchName, options?: ProgramOptions): void;
  note(track: string, pitch: Pitch, options: NoteOptions): void;
  chord(track: string, pitches: readonly Pitch[], options: NoteOptions): void;
  run(track: string, pitches: readonly Pitch[], options: RunOptions): void;
  /** One character per `step`: `x` a note, `X` an accent, `o` a ghost, `.` or `-` a rest; spaces and `|` are ignored. */
  pattern(track: string, pattern: string, options: PatternOptions): void;
  /** Drum lines by drum name, in the characters `pattern()` takes. */
  drums(lines: Partial<Record<DrumName, string>>, options: DrumsOptions): void;
  drum(name: DrumName, at: At | DrumOptions): void;
  fx(name: EffectName, at: At | FxOptions): void;
  /** One effect (a `tick` by default) per time, such as `typewriterFrames()`. */
  ticks(times: readonly At[], options?: TicksOptions): void;
}

export interface ScoreOptions {
  fps: number;
  /** The composition's length; nothing sounds after it. */
  durationInFrames: number;
  /** Changes the noise every hit is made from. Defaults to 0. */
  seed?: number;
  /** Throw instead of warning when an event starts at or after the end. Defaults to false. */
  strict?: boolean;
}

/** One recorded sound, resolved to seconds. */
export interface ScoreEvent {
  kind: 'note' | 'drum' | 'fx';
  /** The patch, drum, or effect. */
  name: PatchName | DrumName | EffectName;
  /** The track of a note. */
  track?: string;
  /** Seconds: where the sound starts, or for a `reverse` whoosh, where it arrives. */
  time: number;
  /** `time` in frames. */
  frame: number;
  /** Seconds a note is held, or an effect's `dur`. */
  length?: number;
  /** MIDI note of a note, possibly fractional. */
  midi?: number;
  vel: number;
  gain: number;
  pan: number;
  reverb: number;
  /** Semitones a drum or effect is moved. */
  pitch: number;
  bright: number;
  reverse: boolean;
}

/** The checked, resolved score that `defineScore()` returns before it is rendered. */
export interface ScoreData {
  fps: number;
  durationInFrames: number;
  /** Seconds. */
  duration: number;
  /** Samples per second of the rendered file. */
  sampleRate: number;
  seed: number;
  /** Every event that sounds, ordered by time (then by when it was recorded). */
  events: readonly ScoreEvent[];
  master: Required<ScoreMaster>;
  /** What was dropped or adjusted, one line per call. Also written to stderr. */
  warnings: readonly string[];
}

const PATTERN_LEVEL: Record<string, number> = { x: 1, X: 1.25, o: 0.45 };
const PATTERN_IGNORED = new Set([' ', '|', '\n', '\t']);
const PATTERN_REST = new Set(['.', '-']);

/** Builds and checks a score. Throws on unknown names, bad values, and events before 0. */
export function buildScore(options: ScoreOptions, build: (s: ScoreBuilder) => void): ScoreData {
  const { fps, durationInFrames } = options;
  if (!Number.isFinite(fps) || fps <= 0) throw new Error('defineScore(): `fps` must be a positive number');
  if (!Number.isInteger(durationInFrames) || durationInFrames <= 0) {
    throw new Error('defineScore(): `durationInFrames` must be a positive integer');
  }
  const seed = options.seed ?? 0;
  if (!Number.isInteger(seed)) throw new Error('defineScore(): `seed` must be an integer');
  const duration = durationInFrames / fps;
  const endSample = Math.round(duration * SAMPLE_RATE);

  const events: ScoreEvent[] = [];
  const warnings: string[] = [];
  const tracks = new Map<string, Required<ProgramOptions> & { patch: PatchName }>();
  let grid: Required<ScoreTempo> | undefined;
  const settings: Required<ScoreMaster> = { gain: 0.8, fadeIn: 0, fadeOut: 0, reverb: 1 };

  // The call being run, for messages and for counting what it dropped.
  let call = '';
  let dropped: number[] = [];

  const fail = (message: string): never => {
    throw new Error(`${call}: ${message}`);
  };

  const secondsOfBeat = (beat: number): number => {
    if (grid === undefined) return fail('a `beat` needs a tempo; call s.tempo() first');
    return grid.offset / fps + (beat * 60) / grid.bpm;
  };

  const finite = (value: unknown, name: string): number => {
    if (typeof value !== 'number' || !Number.isFinite(value)) return fail(`\`${name}\` must be a finite number`);
    return value;
  };

  const inRange = (value: number | undefined, name: string, min: number, max: number, fallback: number): number => {
    if (value === undefined) return fallback;
    const v = finite(value, name);
    if (v < min || v > max) return fail(`\`${name}\` must be from ${min} to ${max}, not ${v}`);
    return v;
  };

  /** The unit of the time an options object gives, and that time in seconds. */
  const timeOf = (at: At | TimeOptions): { seconds: number; unit: 'frame' | 't' | 'beat' } => {
    if (typeof at === 'number') return { seconds: finite(at, 'frame') / fps, unit: 'frame' };
    if (at === null || typeof at !== 'object') return fail('give a time: a frame number, { frame }, { t }, or { beat }');
    const given = (['frame', 't', 'beat'] as const).filter(key => at[key] !== undefined);
    if (given.length !== 1) {
      return fail(given.length === 0 ? 'give a time: { frame }, { t }, or { beat }' : `give only one of ${given.map(k => `\`${k}\``).join(', ')}`);
    }
    const unit = given[0];
    const value = finite(at[unit], unit);
    const seconds = unit === 'frame' ? value / fps : unit === 't' ? value : secondsOfBeat(value);
    return { seconds, unit };
  };

  /** Seconds in `amount` of a unit, at `from` seconds (beats are relative to the grid). */
  const span = (amount: number, unit: 'frame' | 't' | 'beat', name: string): number => {
    const v = finite(amount, name);
    if (v < 0) return fail(`\`${name}\` must not be negative`);
    if (unit === 'frame') return v / fps;
    if (unit === 't') return v;
    if (grid === undefined) return fail(`\`${name}\` in beats needs a tempo; call s.tempo() first`);
    return (v * 60) / grid.bpm;
  };

  const lengthOf = (options: LengthOptions, fallback: () => number | undefined): number | undefined => {
    const given = (['frames', 'dur', 'beats'] as const).filter(key => options[key] !== undefined);
    if (given.length > 1) return fail(`give only one of ${given.map(k => `\`${k}\``).join(', ')}`);
    if (given.length === 0) return fallback();
    const key = given[0];
    return span(options[key] as number, key === 'frames' ? 'frame' : key === 'dur' ? 't' : 'beat', key);
  };

  const defaultNoteLength = (unit: 'frame' | 't' | 'beat'): number => (unit === 'beat' ? span(1, 'beat', 'beats') : 0.5);

  const push = (event: Omit<ScoreEvent, 'frame'>): void => {
    if (event.time < -1e-9) {
      fail(`starts at ${seconds(event.time)}, before the composition; events must not start before 0`);
    }
    const time = event.time < 0 ? 0 : event.time;
    if (Math.round(time * SAMPLE_RATE) >= endSample) {
      dropped.push(time);
      return;
    }
    events.push({ ...event, time, frame: time * fps });
  };

  const run = <A extends unknown[]>(name: string, describe: (...args: A) => string, body: (...args: A) => void) => (...args: A): void => {
    call = `s.${name}(${describe(...args)})`;
    dropped = [];
    body(...args);
    if (dropped.length > 0) {
      const what = dropped.length === 1 ? 'an event starts' : `${dropped.length} events start`;
      const message = `${call}: ${what} at or after the end (${seconds(duration)}), first at ${seconds(dropped[0])}; dropped`;
      if (options.strict) throw new Error(message);
      warnings.push(message);
    }
    call = '';
  };

  const known = <T extends string>(kind: string, name: unknown, table: Record<T, unknown>): T => {
    if (typeof name === 'string' && Object.prototype.hasOwnProperty.call(table, name)) return name as T;
    const names = Object.keys(table);
    const near = typeof name === 'string' ? closest(name, names) : undefined;
    return fail(
      `unknown ${kind} ${JSON.stringify(name)}${near === undefined ? '' : `; did you mean '${near}'?`} (${kind}s: ${names.join(', ')})`,
    );
  };

  const track = (name: string) => {
    if (typeof name !== 'string' || name === '') return fail('the track must be a non-empty string');
    const program = tracks.get(name);
    if (program === undefined) return fail(`track '${name}' has no patch; call s.program('${name}', …) first`);
    return program;
  };

  const addNote = (trackName: string, pitch: Pitch, time: number, length: number, vel: number): void => {
    const program = track(trackName);
    push({
      kind: 'note',
      name: program.patch,
      track: trackName,
      time,
      length,
      midi: parsePitch(pitch, call),
      vel,
      gain: program.gain,
      pan: program.pan,
      reverb: program.reverb,
      pitch: 0,
      bright: program.bright,
      reverse: false,
    });
  };

  const hit = (kind: 'drum' | 'fx', name: DrumName | EffectName, time: number, options: HitOptions, length?: number, reverse = false): void => {
    push({
      kind,
      name,
      time,
      length,
      vel: inRange(options.vel, 'vel', 0, 1, 0.8),
      gain: inRange(options.gain, 'gain', 0, 16, 1),
      pan: inRange(options.pan, 'pan', -1, 1, 0),
      reverb: inRange(options.reverb, 'reverb', 0, 1, kind === 'drum' ? 0.08 : 0.15),
      pitch: inRange(options.pitch, 'pitch', -36, 36, 0),
      bright: 0.5,
      reverse,
    });
  };

  /** The characters of a pattern as hits: their index on the grid and level. */
  const steps = (pattern: string): { hits: { at: number; level: number }[]; total: number } => {
    if (typeof pattern !== 'string') return fail('the pattern must be a string');
    const out: { at: number; level: number }[] = [];
    let index = 0;
    for (const char of pattern) {
      if (PATTERN_IGNORED.has(char)) continue;
      if (PATTERN_REST.has(char)) {
        index += 1;
        continue;
      }
      const level = PATTERN_LEVEL[char];
      if (level === undefined) {
        return fail(`${JSON.stringify(char)} in ${JSON.stringify(pattern)} is not a step; use x (hit), X (accent), o (ghost), . or - (rest)`);
      }
      out.push({ at: index, level });
      index += 1;
    }
    return { hits: out, total: index };
  };

  const patternStart = (options: TimeOptions): number => timeOf(options).seconds;

  const repeatOf = (value: number | undefined): number => {
    if (value === undefined) return 1;
    if (!Number.isInteger(value) || value < 1) return fail('`repeat` must be a positive integer');
    return value;
  };

  const s: ScoreBuilder = {
    tempo: run('tempo', t => (typeof t === 'number' ? String(t) : `{ bpm: ${t?.bpm} }`), (tempo: ScoreTempo | number) => {
      const t = typeof tempo === 'number' ? { bpm: tempo } : tempo;
      const bpm = finite(t?.bpm, 'bpm');
      if (bpm <= 0) fail('`bpm` must be positive');
      const beatsPerBar = t.beatsPerBar ?? 4;
      if (!Number.isInteger(beatsPerBar) || beatsPerBar <= 0) fail('`beatsPerBar` must be a positive integer');
      grid = { bpm, beatsPerBar, offset: t.offset === undefined ? 0 : finite(t.offset, 'offset') };
    }),
    beatFrame: (beat: number) => {
      call = `s.beatFrame(${beat})`;
      const frame = secondsOfBeat(finite(beat, 'beat')) * fps;
      call = '';
      return frame;
    },
    master: run('master', (_m: ScoreMaster) => '…', (m: ScoreMaster) => {
      settings.gain = inRange(m.gain, 'gain', 0, 4, settings.gain);
      settings.fadeIn = inRange(m.fadeIn, 'fadeIn', 0, duration, settings.fadeIn);
      settings.fadeOut = inRange(m.fadeOut, 'fadeOut', 0, duration, settings.fadeOut);
      settings.reverb = inRange(m.reverb, 'reverb', 0, 1, settings.reverb);
    }),
    program: run('program', (t, p) => `'${t}', '${p}'`, (name: string, patch: PatchName, o: ProgramOptions = {}) => {
      if (typeof name !== 'string' || name === '') fail('the track must be a non-empty string');
      tracks.set(name, {
        patch: known('patch', patch, PATCHES),
        gain: inRange(o.gain, 'gain', 0, 16, 0.7),
        pan: inRange(o.pan, 'pan', -1, 1, 0),
        reverb: inRange(o.reverb, 'reverb', 0, 1, 0.2),
        bright: inRange(o.bright, 'bright', 0, 1, 0.5),
      });
    }),
    note: run('note', (t, p) => `'${t}', ${JSON.stringify(p)}`, (name: string, pitch: Pitch, o: NoteOptions) => {
      const { seconds, unit } = timeOf(o);
      const length = lengthOf(o, () => defaultNoteLength(unit)) as number;
      addNote(name, pitch, seconds, length, inRange(o.vel, 'vel', 0, 1, 0.8));
    }),
    chord: run('chord', (t, p) => `'${t}', ${JSON.stringify(p)}`, (name: string, pitches: readonly Pitch[], o: NoteOptions) => {
      if (!Array.isArray(pitches) || pitches.length === 0) fail('give the chord as a non-empty array of pitches');
      const { seconds, unit } = timeOf(o);
      const length = lengthOf(o, () => defaultNoteLength(unit)) as number;
      const vel = inRange(o.vel, 'vel', 0, 1, 0.8);
      for (const pitch of pitches) addNote(name, pitch, seconds, length, vel);
    }),
    run: run('run', (t, p) => `'${t}', ${JSON.stringify(p)}`, (name: string, pitches: readonly Pitch[], o: RunOptions) => {
      if (!Array.isArray(pitches) || pitches.length === 0) fail('give the run as a non-empty array of pitches');
      const { seconds, unit } = timeOf(o);
      const step = span(o.step, unit, 'step');
      const length = lengthOf(o, () => step) as number;
      const vel = inRange(o.vel, 'vel', 0, 1, 0.8);
      const cresc = o.cresc === undefined ? 0 : finite(o.cresc, 'cresc');
      pitches.forEach((pitch, index) => {
        const v = vel + (pitches.length === 1 ? 0 : (cresc * index) / (pitches.length - 1));
        addNote(name, pitch, seconds + index * step, length, Math.min(1, Math.max(0, v)));
      });
    }),
    pattern: run('pattern', (t, p) => `'${t}', ${JSON.stringify(p)}`, (name: string, pattern: string, o: PatternOptions) => {
      track(name);
      const start = patternStart(o);
      const step = span(o.step ?? 0.5, 'beat', 'step');
      const length = o.beats === undefined ? step : span(o.beats, 'beat', 'beats');
      const repeat = repeatOf(o.repeat);
      const vel = inRange(o.vel, 'vel', 0, 1, 0.8);
      const pitches = Array.isArray(o.pitch) ? o.pitch : [o.pitch ?? 'C4'];
      if (pitches.length === 0) fail('`pitch` must not be an empty array');
      const { hits, total } = steps(pattern);
      let n = 0;
      for (let r = 0; r < repeat; r += 1) {
        for (const { at, level } of hits) {
          addNote(name, pitches[n % pitches.length], start + (r * total + at) * step, length, Math.min(1, vel * level));
          n += 1;
        }
      }
    }),
    drums: run('drums', l => Object.keys(l ?? {}).join(', '), (lines: Partial<Record<DrumName, string>>, o: DrumsOptions) => {
      if (lines === null || typeof lines !== 'object') fail('give the drum lines as { kick: "x...", hat: ".x.x" }');
      const start = patternStart(o);
      const step = span(o.step ?? 0.5, 'beat', 'step');
      const repeat = repeatOf(o.repeat);
      const vel = inRange(o.vel, 'vel', 0, 1, 0.8);
      for (const [drum, line] of Object.entries(lines)) {
        const name = known('drum', drum, DRUMS);
        const { hits, total } = steps(line as string);
        for (let r = 0; r < repeat; r += 1) {
          for (const { at, level } of hits) {
            hit('drum', name, start + (r * total + at) * step, { ...o, vel: Math.min(1, vel * level) });
          }
        }
      }
    }),
    drum: run('drum', n => `'${n}'`, (name: DrumName, at: At | DrumOptions) => {
      const drum = known('drum', name, DRUMS);
      hit('drum', drum, timeOf(at).seconds, typeof at === 'number' ? {} : (at as DrumOptions));
    }),
    fx: run('fx', n => `'${n}'`, (name: EffectName, at: At | FxOptions) => {
      const effect = known('effect', name, EFFECTS);
      const o: FxOptions = typeof at === 'number' ? {} : (at as FxOptions);
      const reverse = o.reverse === true;
      if (reverse && effect !== 'whoosh') fail('only `whoosh` takes `reverse`; `revswell` already swells into its time');
      const length = lengthOf(o, () => undefined);
      if (length !== undefined && !EFFECTS_WITH_DURATION.has(effect)) {
        fail(`'${effect}' has a fixed length; only ${[...EFFECTS_WITH_DURATION].join(', ')} take a length`);
      }
      if (length !== undefined && length <= 0) fail('the length must be positive');
      hit('fx', effect, timeOf(at).seconds, o, length, reverse);
    }),
    ticks: run('ticks', t => `${Array.isArray(t) ? t.length : '?'} times`, (times: readonly At[], o: TicksOptions = {}) => {
      if (!Array.isArray(times)) fail('give the times as an array, such as typewriterFrames()');
      const effect = known('effect', o.sound ?? 'tick', EFFECTS);
      const vary = inRange(o.vary, 'vary', 0, 12, 0);
      const base = inRange(o.pitch, 'pitch', -36, 36, 0);
      // The same wander every run: a hash of the tick's place in the list.
      let state = 0x9e3779b9;
      for (const at of times) {
        state = Math.imul(state ^ (state >>> 16), 0x45d9f3b) >>> 0;
        const wander = vary === 0 ? 0 : (2 * (state / 4294967296) - 1) * vary;
        hit('fx', effect, timeOf(at).seconds, { ...o, pitch: base + wander });
      }
    }),
  };

  build(s);

  // Stable: equal times keep the order they were recorded in.
  const ordered = events.map((event, index) => ({ event, index }));
  ordered.sort((a, b) => a.event.time - b.event.time || a.index - b.index);

  return {
    fps,
    durationInFrames,
    duration,
    sampleRate: SAMPLE_RATE,
    seed,
    events: ordered.map(({ event }) => event),
    master: settings,
    warnings,
  };
}

function seconds(value: number): string {
  return `${Number(value.toFixed(3))} s`;
}

/** The name nearest to `name` by edit distance, if any is close. */
function closest(name: string, names: readonly string[]): string | undefined {
  let best: string | undefined;
  let bestDistance = Infinity;
  for (const candidate of names) {
    const distance = editDistance(name.toLowerCase(), candidate);
    if (distance < bestDistance) {
      best = candidate;
      bestDistance = distance;
    }
  }
  return bestDistance <= Math.max(2, Math.floor(name.length / 3)) ? best : undefined;
}

function editDistance(a: string, b: string): number {
  let previous = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i += 1) {
    const current = [i];
    for (let j = 1; j <= b.length; j += 1) {
      current[j] = Math.min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    }
    previous = current;
  }
  return previous[b.length];
}
