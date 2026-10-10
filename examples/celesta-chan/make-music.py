"""Generate the original, deterministic score for film.tsx ("Celesta-chan").

A kawaii future bass cue: 150 BPM in F major, 33 bars (52.8 s), stereo.
At 30 fps one beat is exactly 12 frames, so the picture cuts and hits on the
grid. The progression is the J-pop "royal road" (IV - V - iii - vi). The hook
is a chiptune square lead doubled by a glockenspiel, over side-chained
supersaw chord stabs that bend up into each note, with formant-synthesised
vocal chops ("a", "i", "o") and toy percussion. A celesta-like FM bell plays
the overture and the breakdown.

  bars 0-3    overture: music-box bells, the baton taps, riser, "READY?"
  bars 4-5    first drop (tonight's conductor)
  bars 6-14   groove: write, layer, render
  bars 15-18  second drop (MC time)
  bars 19-23  groove and build: built-ins, beat sync
  bars 24-26  breakdown under tonight's stats, "せーの…", roll into...
  bars 27-28  BRAVO!! drop
  bars 29-32  curtain call, "Fin.", last chord rings out

It also writes hits.json: the frames where the film pops a note (each gets a
glockenspiel ting here) and the loudness of every video frame.

Run from any directory: python3 examples/celesta-chan/make-music.py
Only Python's standard library is required (it takes about a minute).
"""

from __future__ import annotations

from array import array
import json
import math
from pathlib import Path
import sys
import wave

RATE = 44_100
BPM = 150
FPS = 30
BEAT = 60 / BPM
BAR = BEAT * 4
BARS = 33
SECONDS = BARS * BAR
N = int(RATE * SECONDS)
TAU = 2 * math.pi

# Two stereo buses: `dry`, and `pump`, which ducks under every kick.
dry = [array('d', bytes(8 * N)), array('d', bytes(8 * N))]
pump = [array('d', bytes(8 * N)), array('d', bytes(8 * N))]
KICKS: list[float] = []


def hz(midi: float) -> float:
    return 440.0 * 2 ** ((midi - 69) / 12)


def at(bar: int, beat: float = 0.0) -> float:
    return bar * BAR + beat * BEAT


def noise_source(seed: int):
    state = seed & 0xFFFFFFFF

    def next_value() -> float:
        nonlocal state
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        return state / 0x7FFFFFFF - 1.0

    return next_value


def add(start: float, length: float, voice, pan: float = 0.0, bus=None) -> None:
    """Mix mono voice(t) for `length` seconds from `start`, panned -1..1."""
    left, right = bus or dry
    gl = math.cos((pan + 1) * math.pi / 4) * 1.414
    gr = math.sin((pan + 1) * math.pi / 4) * 1.414
    i0 = max(0, int(start * RATE))
    i1 = min(N, int((start + length) * RATE))
    for i in range(i0, i1):
        v = voice((i - i0) / RATE)
        left[i] += v * gl
        right[i] += v * gr


def add_stereo(start: float, length: float, voice_l, voice_r, bus=None) -> None:
    left, right = bus or dry
    i0 = max(0, int(start * RATE))
    i1 = min(N, int((start + length) * RATE))
    for i in range(i0, i1):
        t = (i - i0) / RATE
        left[i] += voice_l(t)
        right[i] += voice_r(t)


def gate(t: float, length: float, attack: float = 0.004, release: float = 0.03) -> float:
    return min(1.0, t / attack, max(0.0, (length - t) / release))


# ---------------------------------------------------------------- voices

def kick(gain: float = 1.0):
    def v(t: float) -> float:
        phase = TAU * (50 * t + (170 / 38) * (1 - math.exp(-38 * t)))
        click = math.exp(-t * 400) * 0.4
        return (math.sin(phase) * math.exp(-t * 7.5) + click) * gain
    return v


def snare(gain: float, seed: int):
    """A bright clap-snare, the kind future bass leans on."""
    rnd = noise_source(seed)
    lp = [0.0]

    def v(t: float) -> float:
        env = 0.0
        for k in range(3):
            dt = t - k * 0.009
            if dt >= 0:
                env = max(env, math.exp(-dt * 120))
        env = max(env, 0.7 * math.exp(-t * 14))
        n = rnd()
        lp[0] += (n - lp[0]) * 0.6
        body = math.sin(TAU * 230 * t) * math.exp(-t * 35) * 0.5
        return ((n - lp[0] * 0.5) * env + body) * gain
    return v


def hat(gain: float, decay: float, seed: int):
    rnd = noise_source(seed)
    prev = [0.0]

    def v(t: float) -> float:
        n = rnd()
        high = n - prev[0]
        prev[0] = n
        return high * math.exp(-t * decay) * gain
    return v


def squeak(gain: float, up: bool = True):
    """A toy squeak: a fast pitch sweep with a wobble."""
    phase = [0.0]

    def v(t: float) -> float:
        x = min(1.0, t / 0.09)
        f = (900 + 1500 * x) if up else (2200 - 1300 * x)
        f *= 1 + 0.04 * math.sin(TAU * 30 * t)
        phase[0] += TAU * f / RATE
        return math.sin(phase[0]) * gate(t, 0.12, 0.003, 0.03) * gain
    return v


def pop(gain: float):
    """A bubble pop."""
    phase = [0.0]

    def v(t: float) -> float:
        f = 300 + 1400 * math.exp(-t * 40)
        phase[0] += TAU * f / RATE
        return math.sin(phase[0]) * math.exp(-t * 30) * gain
    return v


def sub(freq: float, length: float, gain: float = 0.42):
    def v(t: float) -> float:
        tone = math.sin(TAU * freq * t) + 0.18 * math.sin(TAU * freq * 2 * t)
        return tone * gate(t, length, 0.006, 0.04) * gain
    return v


def supersaw(notes, length: float, gain: float, detunes, slide: float = 0.7, bright: float = 1.0):
    """Detuned saws through a filter that closes; each note bends up into pitch."""
    lp = [0.0, 0.0]
    phases = [0.0] * (len(notes) * len(detunes))
    freqs = [hz(n) * 2 ** (d / 12) for n in notes for d in detunes]
    scale = 1.0 / len(freqs) ** 0.5

    def v(t: float) -> float:
        bend = 2 ** (-slide * math.exp(-t * 28) / 12)
        s = 0.0
        for k, f in enumerate(freqs):
            p = phases[k] + f * bend / RATE
            p -= math.floor(p)
            phases[k] = p
            s += 2 * p - 1
        cutoff = (1400 + 5200 * math.exp(-t * 5)) * bright
        a = 1 - math.exp(-TAU * cutoff / RATE)
        lp[0] += (s - lp[0]) * a
        lp[1] += (lp[0] - lp[1]) * a
        return lp[1] * scale * gate(t, length, 0.003, 0.05) * gain
    return v


def square_lead(freq: float, length: float, gain: float, slide_from: float | None = None):
    """A chiptune lead: 25% pulse, a little glide, vibrato that blooms."""
    phase = [0.0]
    lp = [0.0]

    def v(t: float) -> float:
        f = freq
        if slide_from is not None:
            f = slide_from + (freq - slide_from) * min(1.0, t / 0.05)
        f *= 2 ** (0.25 * min(1.0, max(0.0, t - 0.12) * 5) * math.sin(TAU * 6 * t) / 12)
        phase[0] = (phase[0] + f / RATE) % 1.0
        s = 1.0 if phase[0] < 0.25 else -0.33
        lp[0] += (s - lp[0]) * 0.45
        return lp[0] * gate(t, length, 0.003, 0.04) * (0.75 + 0.25 * math.exp(-t * 6)) * gain
    return v


def glock(freq: float, gain: float):
    def v(t: float) -> float:
        a = min(1.0, t * 3000)
        return a * gain * (
            math.sin(TAU * freq * t) * math.exp(-t * 3.2)
            + 0.35 * math.sin(TAU * freq * 2.76 * t) * math.exp(-t * 9)
            + 0.14 * math.sin(TAU * freq * 5.40 * t) * math.exp(-t * 16))
    return v


def bell(freq: float, gain: float, decay: float = 2.6):
    """A small FM bell, close to the sound of a celesta."""
    def v(t: float) -> float:
        index = 2.0 * math.exp(-t * 7)
        mod = math.sin(TAU * freq * 3.5 * t) * index
        env = min(1.0, t * 1500) * math.exp(-t * decay)
        return math.sin(TAU * freq * t + mod) * env * gain
    return v


VOWELS = {
    'a': ((850, 1.0, 7), (1250, 0.6, 9), (2900, 0.25, 12)),
    'i': ((330, 1.0, 7), (2500, 0.5, 12), (3300, 0.3, 14)),
    'o': ((520, 1.0, 7), (900, 0.7, 9), (2700, 0.15, 12)),
    'u': ((350, 1.0, 7), (850, 0.4, 9), (2500, 0.1, 12)),
}


def chop(freq: float, vowel: str, length: float, gain: float, bend: float = 2.0):
    """A pitched vocal chop: a buzzing source through three formant filters."""
    filters = []
    for f, g, q in VOWELS[vowel]:
        w0 = TAU * f * 1.12 / RATE
        alpha = math.sin(w0) / (2 * q)
        a0 = 1 + alpha
        filters.append([alpha / a0, -2 * math.cos(w0) / a0, (1 - alpha) / a0, g, 0.0, 0.0, 0.0, 0.0])
    phase = [0.0]

    def v(t: float) -> float:
        f = freq * 2 ** ((-bend * math.exp(-t * 30) + 0.12 * math.sin(TAU * 5.5 * t)) / 12)
        phase[0] = (phase[0] + f / RATE) % 1.0
        src = 2 * phase[0] - 1
        out = 0.0
        for flt in filters:
            b0, a1, a2, g, x1, x2, y1, y2 = flt
            y = b0 * src - b0 * x2 - a1 * y1 - a2 * y2
            flt[4], flt[5], flt[6], flt[7] = src, x1, y, y1
            out += y * g
        return out * gate(t, length, 0.006, 0.05) * gain
    return v


def pad(notes, length: float, gain: float, attack: float, release: float, cutoff_from: float, cutoff_to: float):
    lp = [0.0]
    freqs = [hz(n) * d for n in notes for d in (0.996, 1.004)]

    def v(t: float) -> float:
        env = min(1.0, t / attack, max(0.0, (length - t) / release))
        saw = 0.0
        for f in freqs:
            p = (f * t) % 1.0
            saw += 2 * p - 1
        cutoff = cutoff_from + (cutoff_to - cutoff_from) * min(1.0, t / length)
        a = 1 - math.exp(-TAU * cutoff / RATE)
        lp[0] += (saw - lp[0]) * a
        return lp[0] * env * gain
    return v


def riser(length: float, gain: float, seed: int):
    rnd = noise_source(seed)
    state = [0.0, 0.0]
    phase = [0.0]

    def v(t: float) -> float:
        x = t / length
        cutoff = 300 + 9000 * x * x
        a = 1 - math.exp(-TAU * cutoff / RATE)
        n = rnd()
        state[0] += (n - state[0]) * a
        state[1] += (state[0] - state[1]) * a
        phase[0] += TAU * (300 + 1500 * x * x) / RATE
        return (state[0] - state[1] * 0.5 + math.sin(phase[0]) * 0.22) * x * x * gain
    return v


def impact():
    rnd = noise_source(99)
    lp = [0.0]

    def v(t: float) -> float:
        phase = TAU * (38 * t + (140 / 12) * (1 - math.exp(-12 * t)))
        lp[0] += (rnd() - lp[0]) * 0.25
        return math.sin(phase) * math.exp(-t * 2.0) * 0.9 + lp[0] * math.exp(-t * 1.8) * 0.45
    return v


def whoosh(length: float, gain: float, seed: int):
    rnd = noise_source(seed)
    lp = [0.0]

    def v(t: float) -> float:
        x = t / length
        env = math.sin(math.pi * min(1.0, x)) ** 2
        a = 1 - math.exp(-TAU * (400 + 6000 * x) / RATE)
        lp[0] += (rnd() - lp[0]) * a
        return lp[0] * env * gain
    return v


# ---------------------------------------------------------------- score

# Royal road in F major, with added colour: (bass root, chord tones).
CHORDS = [
    (46, [62, 65, 69, 72]),   # Bbmaj9 (D F A C over Bb)
    (48, [64, 67, 70, 74]),   # C9 (E G Bb D)
    (45, [64, 67, 69, 72]),   # Am7 (E G A C)
    (50, [65, 69, 72, 76]),   # Dm9-ish (F A C E)
]

# The hook in eighths per bar: (eighth, MIDI note, length in eighths).
HOOK = [
    [(0, 81, 1), (1, 84, 1), (2, 81, 1), (3, 79, 1), (4, 77, 2), (6, 79, 1), (7, 81, 1)],
    [(0, 79, 1), (1, 76, 1), (2, 79, 1), (3, 84, 2), (6, 82, 1), (7, 81, 1)],
    [(0, 79, 1), (1, 81, 1), (2, 76, 1), (3, 77, 1), (4, 79, 2), (6, 76, 1), (7, 77, 1)],
    [(0, 81, 3), (3, 79, 1), (4, 77, 2), (6, 72, 1), (7, 74, 1)],
]
HOOK_TURN = [(0, 81, 1), (1, 84, 1), (2, 86, 2), (4, 89, 4)]
# Future bass chord rhythm: sixteenths that start a stab, and their lengths.
STABS = [(0, 3), (3, 3), (6, 2), (8, 2), (10, 3), (13, 3)]
# Vocal chops answering the lead: (eighth, interval above the root, vowel).
CHOPS = [(5, 24, 'a'), (5.5, 27, 'i')], [(4.5, 28, 'o'), (5, 26, 'a')], [(5, 24, 'i'), (5.5, 28, 'a')], [(2, 29, 'a'), (2.5, 31, 'i'), (3, 33, 'o')]

# Beats (global index, 4 per bar) where the film pops a note.
HITS: list[float] = []
for bar in range(4, 15):            # introduction, write, layer, render
    HITS += [bar * 4 + b for b in range(4)]
for bar in range(15, 19):           # MC time: every other beat
    HITS += [bar * 4, bar * 4 + 2]
for bar in range(19, 23):           # built-ins, beat sync
    HITS += [bar * 4 + b for b in range(4)]
HITS += [23 * 4 + b / 2 for b in range(8)]   # eighths into the stats
HITS += [31 * 4 + 2]                # "Fin."


def chord(bar: int):
    return CHORDS[bar % 4]


def drums(bar: int, full: bool = True, seed: int = 0, fill: bool = False) -> None:
    for beat in range(4):
        KICKS.append(at(bar, beat))
        add(at(bar, beat), 0.4, kick(1.0))
        if beat in (1, 3):
            add(at(bar, beat), 0.35, snare(0.42, seed + bar * 4 + beat))
        add(at(bar, beat + 0.5), 0.1, hat(0.24 if full else 0.15, 40, seed + bar * 8 + beat), pan=0.3)
        if full:
            for s in (0.25, 0.75):
                add(at(bar, beat + s), 0.04, hat(0.08, 170, seed + bar * 16 + beat * 2 + int(s * 4)), pan=-0.3)
    if fill:
        for k, s in enumerate((3.5, 3.75)):
            add(at(bar, s), 0.2, snare(0.3, seed + 70 + bar + k))


def bassline(bar: int, gain: float = 0.42) -> None:
    root = chord(bar)[0]
    add(at(bar), BAR, sub(hz(root - 12), BAR - 0.02, gain), bus=pump)


def stabs(bar: int, gain: float = 0.16) -> None:
    notes = chord(bar)[1]
    for six, length in STABS:
        dur = length * BEAT / 4 * 0.92
        add_stereo(at(bar, six / 4), dur,
                   supersaw(notes, dur, gain, (-0.16, -0.05, 0.08)),
                   supersaw(notes, dur, gain, (-0.08, 0.05, 0.16)), bus=pump)
        add(at(bar, six / 4), dur, supersaw([chord(bar)[0] + 12], dur, gain * 0.6, (0.0,), bright=0.6), bus=pump)


def offbeat_chords(bar: int, gain: float = 0.09) -> None:
    notes = chord(bar)[1]
    for beat in range(4):
        dur = BEAT * 0.4
        add_stereo(at(bar, beat + 0.5), dur,
                   supersaw(notes, dur, gain, (-0.12, 0.04), slide=0.3, bright=0.7),
                   supersaw(notes, dur, gain, (-0.04, 0.12), slide=0.3, bright=0.7), bus=pump)


def lead(bar: int, gain: float = 0.12, turn: bool = False) -> None:
    line = HOOK_TURN if turn and bar % 4 == 3 else HOOK[bar % 4]
    prev = None
    for eighth, note, length in line:
        dur = length * BEAT / 2 * 0.95
        slide = hz(prev) if prev is not None and abs(prev - note) <= 3 else None
        add(at(bar, eighth / 2), dur, square_lead(hz(note), dur, gain, slide), pan=-0.08)
        add(at(bar, eighth / 2), 1.4, glock(hz(note + 12), gain * 0.55), pan=0.25)
        prev = note


def chops(bar: int, gain: float = 0.32) -> None:
    root = chord(bar)[0]
    for eighth, interval, vowel in CHOPS[bar % 4]:
        add(at(bar, eighth / 2), BEAT * 0.45, chop(hz(root + interval), vowel, BEAT * 0.45, gain), pan=0.15 if eighth % 1 else -0.15)


def arp(bar: int, gain: float = 0.07) -> None:
    tones = chord(bar)[1]
    order = [0, 1, 2, 3, 2, 3, 1, 2]
    for step in range(16):
        note = tones[order[step % 8]] + 12
        dur = BEAT / 4 * 0.7
        add(at(bar, step / 4), dur, square_lead(hz(note), dur, gain), pan=-0.4 if step % 2 else 0.4)


def drop(bar: int, turn: bool = False, gain: float = 1.0) -> None:
    drums(bar, fill=bar % 2 == 1)
    bassline(bar)
    stabs(bar, 0.16 * gain)
    lead(bar, 0.12 * gain, turn)
    chops(bar, 0.3 * gain)
    if bar % 2 == 0:
        add(at(bar, 3.75), 0.14, squeak(0.12), pan=0.5)


def groove(bar: int) -> None:
    drums(bar, full=bar >= 8)
    bassline(bar, 0.36)
    offbeat_chords(bar)
    arp(bar, 0.05 if bar < 10 else 0.065)
    if bar % 4 == 3:
        chops(bar, 0.22)


def build() -> None:
    # Overture: a music box over an opening pad, then drums and a riser.
    for bar in range(0, 4):
        add(at(bar), BAR, pad(chord(bar)[1], BAR, 0.03, 0.4 if bar == 0 else 0.05, 0.3, 500 + bar * 300, 900 + bar * 700))
        tones = chord(bar)[1]
        for step in range(8):
            note = tones[[0, 1, 2, 3, 3, 2, 1, 2][step]] + 12
            add(at(bar, step / 2), 1.4, bell(hz(note), 0.08), pan=-0.3 if step % 2 else 0.3)
    add(at(1, 3.5), 0.14, squeak(0.1), pan=0.4)
    add(at(1, 3.75), 0.12, pop(0.16), pan=-0.4)
    for bar in (2, 3):
        for beat in range(4):
            if bar == 3 and beat == 3:
                continue
            KICKS.append(at(bar, beat))
            add(at(bar, beat), 0.4, kick(0.8))
            add(at(bar, beat + 0.5), 0.1, hat(0.18, 45, 700 + bar * 4 + beat), pan=0.3)
        offbeat_chords(bar, 0.06)
    for k, (interval, vowel) in enumerate([(24, 'a'), (28, 'i'), (31, 'o'), (36, 'a')]):
        add(at(3, k * 0.5), BEAT * 0.45, chop(hz(chord(3)[0] + interval), vowel, BEAT * 0.45, 0.28))
    roll, t, step = [], at(2, 2), BEAT / 2
    while t < at(3, 3):
        roll.append(t)
        if t >= at(3):
            step = BEAT / 4
        if t >= at(3, 2):
            step = BEAT / 8
        t += step
    for k, when in enumerate(roll):
        add(when, 0.15, snare(0.06 + 0.24 * k / len(roll), 900 + k))
    add(at(2, 2), at(3, 3) - at(2, 2), riser(at(3, 3) - at(2, 2), 0.24, 5))

    # First drop.
    add(at(4), 3.0, impact())
    for bar in (4, 5):
        drop(bar)

    # Groove.
    for bar in range(6, 15):
        groove(bar)
    for bar in (6, 10, 12):
        add(at(bar) - 0.35, 0.5, whoosh(0.5, 0.25, bar))
    add(at(14, 2), BEAT * 2, riser(BEAT * 2, 0.16, 14))

    # Second drop, under the MC time.
    add(at(15), 2.0, impact())
    for bar in range(15, 19):
        drop(bar, turn=True)

    # Groove and build.
    for bar in range(19, 24):
        groove(bar)
    add(at(19) - 0.35, 0.5, whoosh(0.5, 0.25, 19))
    add(at(21) - 0.35, 0.5, whoosh(0.5, 0.25, 21))
    add(at(23), BAR, riser(BAR, 0.2, 23))

    # Breakdown under the stats: pad, music box, a heartbeat kick.
    for bar in range(24, 27):
        add(at(bar), BAR, pad(chord(bar)[1], BAR, 0.035, 0.3, 0.3, 700, 1500))
        tones = chord(bar)[1]
        for step in range(8):
            add(at(bar, step / 2), 1.2, bell(hz(tones[step % 4] + 24), 0.05), pan=-0.4 if step % 2 else 0.4)
        if bar < 26:
            add(at(bar), 0.4, kick(0.6))
            add(at(bar, 2), 0.4, kick(0.6))
    for k in range(24):
        add(at(24, 1 + k / 6), 0.6, glock(hz(chord(24 + k // 8)[1][k % 4] + 24), 0.02))
    for k, (interval, vowel) in enumerate([(24, 'o'), (28, 'a'), (31, 'i')]):
        add(at(25, 3 + k / 3), BEAT * 0.3, chop(hz(chord(25)[0] + interval), vowel, BEAT * 0.3, 0.2))
    roll, t, step = [], at(26), BEAT / 2
    while t < at(27):
        roll.append(t)
        if t >= at(26, 2):
            step = BEAT / 4
        if t >= at(26, 3):
            step = BEAT / 8
        t += step
    for k, when in enumerate(roll):
        add(when, 0.15, snare(0.06 + 0.26 * k / len(roll), 1200 + k))
    add(at(26), BAR, riser(BAR, 0.26, 26))
    for k, (interval, vowel) in enumerate([(24, 'a'), (26, 'a'), (28, 'i'), (31, 'i'), (33, 'o'), (36, 'a')]):
        add(at(26, 2 + k / 3), BEAT * 0.3, chop(hz(chord(26)[0] + interval), vowel, BEAT * 0.3, 0.24))

    # BRAVO!!
    add(at(27), 3.5, impact())
    for bar in (27, 28):
        drop(bar, gain=1.1)
    add(at(27), 0.14, squeak(0.14), pan=-0.5)

    # Curtain call.
    for bar in range(29, 32):
        drop(bar, turn=True, gain=0.9 if bar < 31 else 0.7)
    final = [53, 60, 65, 69, 72, 77]   # F with an open fifth on top
    add(at(32), BAR, pad(final, BAR, 0.035, 0.02, 1.2, 2600, 600))
    add(at(32), BAR, impact())
    KICKS.append(at(32))
    for k, note in enumerate([77, 81, 84, 89]):
        add(at(32, k * 0.25), BAR - k * 0.25 * BEAT, bell(hz(note), 0.08, 1.4), pan=-0.3 + k * 0.2)
        add(at(32, k * 0.25), 1.6, glock(hz(note + 12), 0.04), pan=0.3 - k * 0.2)
    add(at(32, 1), BEAT, chop(hz(77), 'a', BEAT, 0.22, bend=4))

    for k, beat in enumerate(HITS):
        tones = chord(int(beat // 4))[1]
        add(beat * BEAT, 1.0, glock(hz(tones[k % 4] + 24), 0.045), pan=0.4 if k % 2 else -0.4)


def duck(t: float, kicks: list[float], index: list[int]) -> float:
    while index[0] + 1 < len(kicks) and kicks[index[0] + 1] <= t:
        index[0] += 1
    if index[0] >= len(kicks) or kicks[index[0]] > t:
        return 1.0
    dt = t - kicks[index[0]]
    return 0.22 + 0.78 * min(1.0, dt / 0.2) ** 0.8


def main() -> None:
    build()
    kicks = sorted(KICKS)
    index = [0]
    out = [array('d', bytes(8 * N)), array('d', bytes(8 * N))]
    for i in range(N):
        d = duck(i / RATE, kicks, index)
        for ch in (0, 1):
            out[ch][i] = dry[ch][i] + pump[ch][i] * d
    peak = max(max(abs(v) for v in ch) for ch in out) or 1.0
    gain = 0.9 / peak
    output = Path(__file__).with_name('music.wav')
    with wave.open(str(output), 'wb') as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        values = array('h')
        norm = math.tanh(1.4)
        for i in range(N):
            for ch in (0, 1):
                v = math.tanh(out[ch][i] * gain * 1.4) / norm
                values.append(round(32767 * max(-1.0, min(1.0, v))))
        if sys.byteorder != 'little':
            values.byteswap()
        wav.writeframes(values.tobytes())

    frames_per_beat = FPS * 60 // BPM
    # Loudness per video frame, 0-1, for the waveform in the beat scene.
    per_frame = RATE // FPS
    rms = []
    for f in range(BARS * 4 * frames_per_beat):
        lo, hi = f * per_frame, min(N, (f + 1) * per_frame)
        s = sum((out[0][i] + out[1][i]) ** 2 for i in range(lo, hi)) / 4
        rms.append(math.sqrt(s / max(1, hi - lo)))
    top = max(rms) or 1.0
    hits = {
        'bpm': BPM,
        'framesPerBeat': frames_per_beat,
        'frames': [round(beat * frames_per_beat) for beat in HITS],
        'loudness': [round(v / top, 3) for v in rms],
    }
    Path(__file__).with_name('hits.json').write_text(json.dumps(hits) + '\n')
    print(output)


if __name__ == '__main__':
    main()
