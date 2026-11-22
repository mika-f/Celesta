import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import * as path from 'node:path';

import { Audio, beatAt, defineScenes } from '@celesta.studio/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { cos, exp, semitoneRatio, sin } from '../src/dmath.js';
import { DRUM_NAMES, EFFECT_NAMES, PATCH_NAMES, ScoreAudio, defineScore, parsePitch } from '../src/index.js';
import type { ScoreBuilder, ScoreOptions } from '../src/index.js';

const RATE = 48000;

function quiet<T>(fn: () => T): T {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    return fn();
  } finally {
    warn.mockRestore();
  }
}

function score(options: Partial<ScoreOptions>, build: (s: ScoreBuilder) => void) {
  return quiet(() => defineScore({ fps: 30, durationInFrames: 60, ...options }, build));
}

/** Interleaved 16-bit samples of a WAV written by the score. */
function pcm(wav: Uint8Array): Int16Array {
  return new Int16Array(wav.slice(44).buffer);
}

function firstSoundingFrame(samples: Int16Array, from = 0): number {
  for (let i = from * 2; i < samples.length; i += 1) if (samples[i] !== 0) return Math.floor(i / 2);
  return -1;
}

function lastSoundingFrame(samples: Int16Array): number {
  for (let i = samples.length - 1; i >= 0; i -= 1) if (samples[i] !== 0) return Math.floor(i / 2);
  return -1;
}

/** A score that sounds every patch, drum, and effect once. */
function everything(s: ScoreBuilder): void {
  s.tempo({ bpm: 120 });
  s.master({ gain: 0.7, fadeIn: 0.05, fadeOut: 0.5 });
  PATCH_NAMES.forEach((patch, index) => {
    s.program(patch, patch, { pan: index / 8 - 0.5, bright: 0.7 });
    s.note(patch, 48 + index * 3, { beat: index, beats: 0.75 });
  });
  s.chord('keys', ['C4', 'E4', 'G4'], { beat: 8, beats: 2 });
  s.run('marimba', ['C5', 'D5', 'E5', 'G5'], { beat: 10, step: 0.25, cresc: 0.2 });
  s.pattern('bass', 'x.x.X.xo', { beat: 12, pitch: ['C2', 'G2'] });
  s.drums({ kick: 'x...x...', hat: '.o.x.o.x', snare: '..x...x.' }, { beat: 12 });
  DRUM_NAMES.forEach((drum, index) => s.drum(drum, { beat: 16 + index * 0.5, pan: 0.2 }));
  EFFECT_NAMES.forEach((effect, index) => s.fx(effect, { t: 15 + index * 0.6, pitch: index % 3 }));
  s.fx('whoosh', { t: 23.5, reverse: true, dur: 0.8 });
  s.ticks([700, 702, 704, 706], { vary: 1 });
}

describe('deterministic math', () => {
  it('stays close to Math within what 16-bit audio can show', () => {
    for (let x = -50; x <= 50; x += 0.37) {
      expect(Math.abs(sin(x) - Math.sin(x))).toBeLessThan(1e-11);
      expect(Math.abs(cos(x) - Math.cos(x))).toBeLessThan(1e-11);
    }
    for (let x = -30; x <= 30; x += 0.29) {
      expect(Math.abs(exp(x) / Math.exp(x) - 1)).toBeLessThan(1e-14);
    }
    expect(semitoneRatio(12)).toBeCloseTo(2, 14);
    expect(exp(-1000)).toBe(0);
  });
});

describe('determinism', () => {
  it('renders the same bytes every time', () => {
    const a = score({ durationInFrames: 750 }, everything).toWav();
    const b = score({ durationInFrames: 750 }, everything).toWav();
    expect(Buffer.from(a).equals(Buffer.from(b))).toBe(true);
  });

  it('renders the bytes this version is pinned to', () => {
    // A change here means every cached score changes: bump SYNTH_VERSION.
    const wav = score({ durationInFrames: 750 }, everything).toWav();
    expect(createHash('sha256').update(wav).digest('hex')).toBe('7e8fa83d19d49744e9fe1ef3914d7ea4869078ca5493b7d798ff1fa2bdae0451');
  });

  it('changes the noise only with the seed', () => {
    const hat = (seed: number) => score({ seed }, s => s.drum('hat', 3)).toWav();
    expect(Buffer.from(hat(1)).equals(Buffer.from(hat(1)))).toBe(true);
    expect(Buffer.from(hat(1)).equals(Buffer.from(hat(2)))).toBe(false);
  });
});

describe('the file', () => {
  it('is 48 kHz 16-bit stereo, exactly as long as the composition', () => {
    const wav = score({ fps: 24, durationInFrames: 37 }, () => {}).toWav();
    const view = new DataView(wav.buffer);
    expect(String.fromCharCode(...wav.slice(0, 4))).toBe('RIFF');
    expect(view.getUint16(22, true)).toBe(2);
    expect(view.getUint32(24, true)).toBe(RATE);
    expect(view.getUint16(34, true)).toBe(16);
    expect(view.getUint32(40, true)).toBe(Math.round((37 / 24) * RATE) * 4);
    expect(pcm(wav).every(v => v === 0)).toBe(true);
  });
});

describe('timing', () => {
  it('places a sound on its exact sample, from frames, seconds, or beats', () => {
    for (const at of [15, { frame: 15 }, { t: 0.5 }, { beat: 1 }] as const) {
      const wav = score({}, s => {
        s.tempo({ bpm: 120 });
        s.fx('pop', at as never);
      }).toWav();
      // The 0.5 ms attack starts from zero, so the first sample is silent.
      const first = firstSoundingFrame(pcm(wav));
      expect(first).toBeGreaterThanOrEqual(RATE / 2);
      expect(first).toBeLessThanOrEqual(RATE / 2 + 2);
    }
  });

  it('puts beats where beatAt() puts them, offset included', () => {
    const grid = { bpm: 128, offset: 7 };
    const s = score({ durationInFrames: 300 }, b => {
      b.tempo(grid);
      b.fx('tick', { beat: 4 });
    });
    const frame = s.events[0].frame;
    expect(beatAt(frame, 30, grid).beat).toBe(4);
    expect(beatAt(frame, 30, grid).progress).toBeCloseTo(0, 9);
    expect(frame).toBeCloseTo(7 + (4 * 30 * 60) / 128, 9);
  });

  it('takes scene starts and lists of frames', () => {
    const scenes = defineScenes([
      { id: 'intro', durationInFrames: 40 },
      { id: 'main', durationInFrames: 50 },
    ]);
    const s = score({ durationInFrames: scenes.durationInFrames }, b => {
      for (const scene of scenes.scenes) b.fx('whoosh', scene.from);
      b.ticks([3, 5, 8]);
    });
    expect(s.events.map(e => [e.name, e.frame])).toEqual([
      ['whoosh', 0],
      ['tick', 3],
      ['tick', 5],
      ['tick', 8],
      ['whoosh', 40],
    ]);
  });

  it('reads patterns as half beats: x, X accents, o ghosts, . and - rest', () => {
    const s = score({ durationInFrames: 300 }, b => {
      b.tempo(60);
      b.program('bass', 'bass');
      b.pattern('bass', 'x.X- o|x', { beat: 2, pitch: ['C2', 'G2'], vel: 0.6, repeat: 2 });
    });
    // Six steps (space and | are ignored): hits on 0, 2, 4, 5, then again 6 steps later.
    expect(s.events.map(e => e.time)).toEqual([2, 3, 4, 4.5, 5, 6, 7, 7.5]);
    expect(s.events.slice(0, 4).map(e => e.midi)).toEqual([36, 43, 36, 43]);
    expect(s.events.slice(0, 4).map(e => e.vel)).toEqual([0.6, 0.75, 0.6 * 0.45, 0.6]);
    expect(s.events[0].length).toBe(0.5);
  });

  it('steps a run in the unit of its start', () => {
    const s = score({ durationInFrames: 300 }, b => {
      b.program('m', 'marimba');
      b.run('m', ['C4', 'D4', 'E4'], { frame: 30, step: 6, cresc: 0.2, vel: 0.5 });
    });
    expect(s.events.map(e => e.frame)).toEqual([30, 36, 42]);
    expect(s.events.map(e => e.vel)).toEqual([0.5, 0.6, 0.7]);
  });

  it('lands a reversed whoosh on its time and stops there', () => {
    const wav = score({}, s => s.fx('whoosh', { t: 1.5, dur: 0.5, reverse: true, reverb: 0 })).toWav();
    const samples = pcm(wav);
    expect(lastSoundingFrame(samples)).toBeLessThan(1.5 * RATE);
    expect(lastSoundingFrame(samples)).toBeGreaterThan(1.49 * RATE);
    expect(firstSoundingFrame(samples)).toBeGreaterThanOrEqual(1 * RATE);
  });

  it('trims a reversed whoosh that would start before 0', () => {
    const s = score({}, b => b.fx('whoosh', { t: 0.2, dur: 0.6, reverse: true }));
    expect(s.warnings).toEqual([]);
    expect(firstSoundingFrame(pcm(s.toWav()))).toBe(0);
  });

  it('keeps a sound that arrives at the end, and drops one that would start after it', () => {
    const s = score({ strict: true }, b => {
      b.fx('whoosh', { frame: 60, reverse: true, dur: 0.5, reverb: 0 });
      b.fx('revswell', { frame: 60, reverb: 0 });
    });
    expect(s.events.map(e => e.name)).toEqual(['whoosh', 'revswell']);
    const samples = pcm(s.toWav());
    expect(firstSoundingFrame(samples)).toBeLessThan(1.5 * RATE);
    expect(lastSoundingFrame(samples)).toBeGreaterThan(1.99 * RATE);

    const late = score({}, b => b.fx('whoosh', { frame: 80, reverse: true, dur: 0.5 }));
    expect(late.events).toEqual([]);
    expect(late.warnings).toEqual(["s.fx('whoosh'): an event starts at or after the end (2 s), first at 2.167 s; dropped"]);
    // Arriving after the end but starting before it: the part before the end plays.
    const partly = score({}, b => b.fx('whoosh', { frame: 66, reverse: true, dur: 0.5 }));
    expect(partly.events).toHaveLength(1);
  });

  it('stops roll and sparkle at the end of their dur', () => {
    for (const name of ['roll', 'sparkle'] as const) {
      for (const dur of [0.1, 0.4]) {
        const wav = score({}, b => b.fx(name, { t: 1, dur, reverb: 0 })).toWav();
        const last = lastSoundingFrame(pcm(wav));
        expect(last, `${name} ${dur}`).toBeLessThan((1 + dur) * RATE);
        expect(last, `${name} ${dur}`).toBeGreaterThan(1 * RATE);
      }
    }
  });

  it('fades the whole mix out at the end', () => {
    const wav = score({}, s => {
      s.master({ fadeOut: 0.5 });
      s.drum('crash', 0);
    }).toWav();
    const samples = pcm(wav);
    expect(samples[samples.length - 1]).toBe(0);
    expect(samples[samples.length - 2]).toBe(0);
  });
});

/** The fundamental of a mono signal in Hz, from its autocorrelation near `expected`. */
function fundamental(samples: Float64Array, expected: number): number {
  const lagOf = (hz: number) => RATE / hz;
  const corr = (lag: number) => {
    let sum = 0;
    for (let i = 0; i + lag < samples.length; i += 1) sum += samples[i] * samples[i + lag];
    return sum;
  };
  let best = 0;
  let bestValue = -Infinity;
  for (let lag = Math.floor(lagOf(expected * 1.06)); lag <= Math.ceil(lagOf(expected / 1.06)); lag += 1) {
    const value = corr(lag);
    if (value > bestValue) {
      bestValue = value;
      best = lag;
    }
  }
  const [a, b, c] = [corr(best - 1), bestValue, corr(best + 1)];
  return RATE / (best + (0.5 * (a - c)) / (a - 2 * b + c));
}

describe('tuning', () => {
  it('plays pluck at the pitch of its note', () => {
    for (const note of ['A2', 'A3', 'A4', 'A5', 'E6']) {
      const midi = parsePitch(note, 'test');
      const hz = 440 * 2 ** ((midi - 69) / 12);
      const wav = score({}, b => {
        b.program('p', 'pluck', { reverb: 0 });
        b.note('p', note, { t: 0, dur: 1 });
      }).toWav();
      const left = pcm(wav);
      const mono = new Float64Array(9600);
      for (let i = 0; i < mono.length; i += 1) mono[i] = left[2 * (2400 + i)];
      const measured = fundamental(mono, hz);
      // Within 5 cents.
      expect(Math.abs(1200 * Math.log2(measured / hz)), note).toBeLessThan(5);
    }
  });
});

describe('mistakes', () => {
  const throws = (build: (s: ScoreBuilder) => void) => expect(() => score({}, build));

  it('names unknown sounds and suggests the nearest', () => {
    throws(s => s.fx('wosh' as never, 0)).toThrow(/s\.fx\('wosh'\): unknown effect "wosh"; did you mean 'whoosh'\?/);
    throws(s => s.drum('kik' as never, 0)).toThrow(/unknown drum "kik"; did you mean 'kick'\?/);
    throws(s => s.program('a', 'piano' as never)).toThrow(/unknown patch "piano"/);
    throws(s => {
      s.tempo(120);
      s.drums({ kick: 'x', bongo: 'x' } as never, { frame: 0 });
    }).toThrow(/unknown drum "bongo"/);
    throws(s => s.ticks([1], { sound: 'tok' as never })).toThrow(/unknown effect "tok"/);
  });

  it('rejects a pattern character it does not know', () => {
    throws(s => {
      s.tempo(120);
      s.drums({ kick: 'x..y' }, { beat: 0 });
    }).toThrow(/"y" in "x\.\.y" is not a step/);
  });

  it('asks for a tempo before beats, and a patch before notes', () => {
    throws(s => s.fx('tick', { beat: 1 })).toThrow(/needs a tempo; call s\.tempo\(\) first/);
    throws(s => s.note('lead', 'C4', { frame: 0 })).toThrow(/track 'lead' has no patch; call s\.program\('lead', …\) first/);
  });

  it('rejects ambiguous or missing times, bad values, and times before 0', () => {
    throws(s => s.fx('tick', { frame: 1, t: 1 })).toThrow(/give only one of `frame`, `t`/);
    throws(s => s.fx('tick', {})).toThrow(/give a time/);
    throws(s => s.fx('tick', { frame: 1, vel: 2 })).toThrow(/`vel` must be from 0 to 1, not 2/);
    throws(s => s.fx('tick', -3)).toThrow(/before the composition/);
    throws(s => s.fx('pop', { frame: 1, dur: 1 })).toThrow(/'pop' has a fixed length/);
    throws(s => s.fx('ding', { frame: 1, reverse: true })).toThrow(/only `whoosh` takes `reverse`/);
    throws(s => s.note('x', 'H4', { frame: 0 })).toThrow(/track 'x' has no patch/);
    expect(() => parsePitch('H4', 'here')).toThrow(/"H4" is not a pitch/);
    expect(parsePitch('Bb2', 'here')).toBe(46);
    expect(parsePitch('F#3', 'here')).toBe(54);
  });

  it('drops events at or after the end with one warning per call', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    try {
      const s = defineScore({ fps: 30, durationInFrames: 60 }, b => {
        b.tempo(120);
        b.drums({ kick: 'x...' }, { beat: 0, repeat: 8 });
        b.fx('ding', 60);
      });
      expect(s.events).toHaveLength(2);
      expect(s.warnings).toEqual([
        's.drums(kick): 6 events start at or after the end (2 s), first at 2 s; dropped',
        "s.fx('ding'): an event starts at or after the end (2 s), first at 2 s; dropped",
      ]);
      expect(warn).toHaveBeenCalledTimes(2);
    } finally {
      warn.mockRestore();
    }
  });

  it('throws on them instead with strict', () => {
    expect(() => defineScore({ fps: 30, durationInFrames: 60, strict: true }, s => s.fx('tick', 61))).toThrow(
      /s\.fx\('tick'\): an event starts at or after the end/,
    );
  });
});

describe('prepare()', () => {
  let dir: string | undefined;

  afterEach(async () => {
    if (dir !== undefined) await rm(dir, { recursive: true, force: true });
    dir = undefined;
  });

  it('writes the WAV once, named by its content', async () => {
    dir = await mkdtemp(path.join(tmpdir(), 'celesta-score-'));
    const make = () => score({}, s => s.fx('chime', 3));
    const first = make();
    expect(() => first.src).toThrow(/call `await score\.prepare\(\)`/);
    const file = await first.prepare({ cacheDir: dir });
    expect(path.isAbsolute(file)).toBe(true);
    expect(first.src).toBe(file);
    expect(path.basename(file)).toBe(`${first.key}.wav`);
    expect(Buffer.from(await readFile(file)).equals(Buffer.from(first.toWav()))).toBe(true);

    const again = make();
    expect(await again.prepare({ cacheDir: dir })).toBe(file);
    const other = score({}, s => s.fx('chime', 4));
    expect(await other.prepare({ cacheDir: dir })).not.toBe(file);
  });

  it('plays through <Audio>', async () => {
    dir = await mkdtemp(path.join(tmpdir(), 'celesta-score-'));
    const s = score({}, b => b.fx('pop', 0));
    await s.prepare({ cacheDir: dir });
    const element = ScoreAudio({ score: s, volume: 0.5, duckGroup: 'sfx' });
    expect(element.type).toBe(Audio);
    expect(element.props).toMatchObject({ src: s.src, volume: 0.5, duckGroup: 'sfx' });
  });
});
