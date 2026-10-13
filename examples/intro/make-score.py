"""Generate the original, deterministic 60-second score for film.tsx.

120 BPM, 30 bars, stereo. The arrangement follows the picture:
  bars 0-2   cold open: kick and a bell per beat, sparse, as the ball bounces
  bar 3      reveal: an impact, then the groove drops out under a held pad
  bars 5-8   problem: dry ticks on every eighth, a stumble when the keyframes
             slip (bar 7, beat 3), a reverse swell into bar 9
  bars 9-12  idea: a half-time pulse, a pluck each time the scrub bar turns
  bars 13-21 components and live edit: the full groove, a click on each save
  bars 22-25 export: the groove plus a rising arpeggio while frames render,
             a bell chord at the check mark
  bars 26-29 outro: an impact at 52 s, a pad and bells; from 55 s only the
             pad, fading to silence by 60 s

Run from any directory: python3 examples/intro/make-score.py
Only Python's standard library is required. Writes music.wav next to this file.
"""

from array import array
import math
from pathlib import Path
import sys
import wave

RATE = 44_100
BPM = 120
BEAT = 60 / BPM
BAR = BEAT * 4
BARS = 30
SECONDS = BAR * BARS
N = int(RATE * SECONDS)
TAU = 2 * math.pi
FPS = 30

left = [0.0] * N
right = [0.0] * N

# Picture cues, in seconds. Keep in sync with shared/cues.ts,
# shared/scrub.ts, shared/live-edit.ts, and shared/export.ts.
REVEAL_AT = BAR * 3
PROBLEM_AT = BAR * 5
IDEA_AT = BAR * 9
COMPONENTS_AT = BAR * 13
LIVE_EDIT_AT = BAR * 17
EXPORT_AT = BAR * 22
OUTRO_AT = BAR * 26
HOLD_AT = 1650 / FPS
SAVES = [LIVE_EDIT_AT + f / FPS for f in (75, 165)]
EXPORT_DONE = EXPORT_AT + 120 / FPS
SCRUB_TURNS = [IDEA_AT + f / FPS for f in (60, 110, 170)]


def hz(midi: float) -> float:
    return 440.0 * 2 ** ((midi - 69) / 12)


def noise_source(seed: int):
    state = seed & 0xFFFFFFFF

    def next_value() -> float:
        nonlocal state
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        return state / 0x7FFFFFFF - 1.0

    return next_value


def add(start: float, length: float, voice, pan: float = 0.0, gain: float = 1.0) -> None:
    """Mix voice(t) into both channels for `length` seconds from `start`.
    `pan` runs from -1 (left) to 1 (right), equal power."""
    angle = (pan + 1) * math.pi / 4
    gl = math.cos(angle) * gain * math.sqrt(2)
    gr = math.sin(angle) * gain * math.sqrt(2)
    i0 = max(0, int(start * RATE))
    i1 = min(N, int((start + length) * RATE))
    for i in range(i0, i1):
        v = voice((i - i0) / RATE)
        left[i] += v * gl
        right[i] += v * gr


# ── Voices ──────────────────────────────────────────────────────────────────


def kick(gain: float = 0.9):
    def v(t: float) -> float:
        phase = TAU * (48 * t + (110 / 28) * (1 - math.exp(-28 * t)))
        click = math.exp(-t * 400) * 0.3
        return (math.sin(phase) * math.exp(-t * 6.5) + click) * gain
    return v


def impact(seed: int):
    rnd = noise_source(seed)
    lp = [0.0]

    def v(t: float) -> float:
        phase = TAU * (34 * t + (150 / 11) * (1 - math.exp(-11 * t)))
        sub = math.sin(phase) * math.exp(-t * 1.4) * 0.95
        lp[0] += (rnd() - lp[0]) * 0.22
        crash = lp[0] * math.exp(-t * 2.0) * 0.4
        return sub + crash
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


def clap(gain: float, seed: int):
    rnd = noise_source(seed)
    lp = [0.0]

    def v(t: float) -> float:
        env = 0.0
        for k in range(3):
            dt = t - k * 0.012
            if dt >= 0:
                env = max(env, math.exp(-dt * 85))
        env = max(env, 0.5 * math.exp(-t * 14))
        lp[0] += (rnd() - lp[0]) * 0.4
        return (rnd() - lp[0]) * env * gain
    return v


def snare(gain: float, seed: int):
    rnd = noise_source(seed)

    def v(t: float) -> float:
        body = math.sin(TAU * 190 * t) * math.exp(-t * 30) * 0.5
        return (body + rnd() * math.exp(-t * 22) * 0.7) * gain
    return v


def bass(freq: float, length: float, gain: float = 0.34):
    lp = [0.0]

    def v(t: float) -> float:
        env = min(1.0, t * 300) * min(1.0, max(0.0, (length - t) * 60))
        saw = 2 * ((freq * t) % 1.0) - 1
        lp[0] += (saw - lp[0]) * 0.08
        return (lp[0] * 0.8 + 0.6 * math.sin(TAU * freq * t)) * env * gain
    return v


def pluck(freq: float, gain: float):
    def v(t: float) -> float:
        env = min(1.0, t * 900) * math.exp(-t * 13)
        s = 0.0
        for h, a in ((1, 1.0), (2, 0.35), (3, 0.2), (4, 0.08)):
            s += a * math.sin(TAU * freq * h * t) * math.exp(-t * 4 * h)
        return s * env * gain
    return v


def bell(freq: float, gain: float, decay: float = 2.4):
    """A small FM bell, close to the sound of a celesta."""
    def v(t: float) -> float:
        index = 2.2 * math.exp(-t * 5)
        mod = math.sin(TAU * freq * 3.5 * t) * index
        env = min(1.0, t * 1200) * math.exp(-t * decay)
        return math.sin(TAU * freq * t + mod) * env * gain
    return v


def pad(freqs, length: float, gain: float, attack: float, release: float, cutoff_from: float, cutoff_to: float):
    lp = [0.0, 0.0]

    def v(t: float) -> float:
        env = min(1.0, t / attack, max(0.0, (length - t) / release))
        saw = 0.0
        for f in freqs:
            for detune in (0.996, 1.004):
                saw += 2 * ((f * detune * t) % 1.0) - 1
        cutoff = cutoff_from + (cutoff_to - cutoff_from) * min(1.0, t / length)
        a = 1 - math.exp(-TAU * cutoff / RATE)
        lp[0] += (saw - lp[0]) * a
        lp[1] += (lp[0] - lp[1]) * a
        return lp[1] * env * gain / len(freqs)
    return v


def whoosh(length: float, peak: float, gain: float, seed: int, rising: bool = True):
    """Band-limited noise that swells toward `peak` seconds, then falls away."""
    rnd = noise_source(seed)
    lp = [0.0, 0.0]

    def v(t: float) -> float:
        if t < peak:
            env = (t / peak) ** 2.5
        else:
            env = math.exp(-(t - peak) * 9)
        sweep = t / length if rising else 1 - t / length
        a = 0.02 + 0.3 * sweep
        lp[0] += (rnd() - lp[0]) * a
        lp[1] += (lp[0] - lp[1]) * a
        return (lp[0] - lp[1] * 0.5) * env * gain
    return v


def tick(freq: float, gain: float):
    def v(t: float) -> float:
        return math.sin(TAU * freq * t) * math.exp(-t * 90) * gain
    return v


# ── Harmony ─────────────────────────────────────────────────────────────────

# A minor: Am - F - C - G, one chord per bar.
CHORDS = [
    (45, [57, 60, 64, 67]),  # Am7
    (41, [53, 57, 60, 64]),  # Fmaj7
    (48, [55, 60, 64, 67]),  # C
    (43, [55, 59, 62, 67]),  # G
]


def chord_at(bar: int):
    return CHORDS[bar % 4]


# ── Arrangement ─────────────────────────────────────────────────────────────


def groove(bar: int, energy: float) -> None:
    t0 = bar * BAR
    root, notes = chord_at(bar)
    for b in range(4):
        add(t0 + b * BEAT, 0.5, kick(0.95))
        add(t0 + b * BEAT + BEAT / 2, 0.12, hat(0.16, 30, bar * 8 + b), pan=0.35)
        # Bass on the offbeats, pumping under the kick.
        add(t0 + b * BEAT + BEAT / 2, BEAT / 2 - 0.02, bass(hz(root), BEAT / 2 - 0.02))
    for b in (1, 3):
        add(t0 + b * BEAT, 0.35, clap(0.34, bar * 4 + b), pan=-0.05)
    for s in range(16):
        if s % 2 == 1:
            add(t0 + s * BEAT / 4, 0.05, hat(0.07 * energy, 70, bar * 32 + s), pan=-0.4 if s % 4 == 1 else 0.4)
    # Arpeggio: chord tones in sixteenths, up an octave when energy is high.
    pattern = [0, 1, 2, 3, 2, 1, 3, 0]
    for s in range(16):
        note = notes[pattern[s % 8]] + (12 if energy > 1 and s % 8 >= 4 else 0)
        pan = -0.5 + (s % 8) / 7
        add(t0 + s * BEAT / 4, 0.35, pluck(hz(note + 12), 0.075), pan=pan)


def main() -> None:
    # Bars 0-2: the cold open. A kick on every bounce, a bell per beat.
    for bar in range(3):
        t0 = bar * BAR
        for b in range(4):
            add(t0 + b * BEAT, 0.5, kick(0.7 if b % 2 == 0 else 0.45))
            note = [72, 76, 79, 84][b] + (0 if bar < 2 else 2)
            add(t0 + b * BEAT, 1.2, bell(hz(note), 0.14, 3.0), pan=[-0.3, 0.3, -0.15, 0.15][b])
        for e in range(8):
            add(t0 + e * BEAT / 2 + BEAT / 4, 0.06, hat(0.05, 55, bar * 8 + e), pan=0.4 if e % 2 else -0.4)
    add(REVEAL_AT - 0.9, 0.95, whoosh(0.95, 0.9, 0.45, 11))

    # Bars 3-4: the reveal. Impact, then a held Fmaj9 pad with one bell.
    add(REVEAL_AT, 3.0, impact(21), gain=0.8)
    add(REVEAL_AT, BAR * 2, pad([hz(53), hz(60), hz(64), hz(67), hz(69)], BAR * 2, 0.3, 0.05, 0.6, 2600, 900))
    add(REVEAL_AT + BAR, 2.0, bell(hz(76), 0.16, 1.8), pan=0.2)

    # Bars 5-8: the problem. Dry ticks, a stumble, a reverse swell.
    for bar in range(5, 9):
        t0 = bar * BAR
        for e in range(8):
            add(t0 + e * BEAT / 2, 0.08, tick(2200 if e % 2 else 1600, 0.12), pan=0.3 if e % 2 else -0.3)
        add(t0, BAR, bass(hz(41 if bar < 7 else 40), BAR - 0.05, 0.18))
    stumble = BAR * 7 + BEAT * 2
    for k, dt in enumerate((0, 0.07, 0.19, 0.23)):
        add(stumble + dt, 0.3, snare(0.18, 50 + k), pan=-0.3 + k * 0.2)
    add(IDEA_AT - 1.9, 1.95, whoosh(1.95, 1.9, 0.5, 12))

    # Bars 9-12: the idea. Half-time pulse, a pluck at each scrub turn.
    for bar in range(9, 13):
        t0 = bar * BAR
        root, notes = chord_at(bar)
        add(t0, BAR, pad([hz(n) for n in notes], BAR, 0.24, 0.3, 0.3, 900, 1400))
        add(t0, 0.5, kick(0.8))
        add(t0 + BEAT * 2, 0.35, clap(0.28, bar), pan=-0.05)
        for e in range(8):
            add(t0 + e * BEAT / 2 + BEAT / 4, 0.06, hat(0.05, 60, bar * 8 + e), pan=0.4 if e % 2 else -0.4)
    for i, at in enumerate(SCRUB_TURNS):
        add(at, 1.0, pluck(hz([76, 79, 81][i]), 0.22), pan=-0.3 + i * 0.3)
    add(COMPONENTS_AT - 0.9, 0.95, whoosh(0.95, 0.9, 0.45, 13))

    # Bars 13-21: components and live edit. The full groove.
    add(COMPONENTS_AT, 2.5, impact(22), gain=0.5)
    for bar in range(13, 22):
        groove(bar, 0.8 if bar < 15 else 1.0)
    for i, at in enumerate(SAVES):
        add(at, 0.05, tick(4200, 0.3), pan=0.2)
        add(at, 1.6, bell(hz(84 + i * 3), 0.14, 2.2), pan=-0.2)

    # Bars 22-25: export. Groove, a rising arpeggio, a bell chord at the check.
    add(EXPORT_AT, 2.5, impact(23), gain=0.6)
    for bar in range(22, 26):
        groove(bar, 1.2)
    rise = [69, 72, 76, 79, 81, 84, 88, 91]
    for s in range(24):
        at = EXPORT_AT + 30 / FPS + s * (90 / FPS) / 24
        add(at, 0.3, pluck(hz(rise[s % 8] + (12 if s >= 16 else 0)), 0.08), pan=-0.5 + (s % 8) / 7)
    for k, note in enumerate((81, 88, 93)):
        add(EXPORT_DONE + k * 0.06, 3.0, bell(hz(note), 0.18 - k * 0.04, 1.6), pan=(-0.2, 0.2, 0.0)[k])
    add(OUTRO_AT - 1.6, 1.65, whoosh(1.65, 1.6, 0.55, 14))

    # Bars 26-29: the outro. Impact, pad and bells; from 55 s only the pad.
    add(OUTRO_AT, 4.0, impact(24), gain=1.0)
    add(OUTRO_AT, SECONDS - OUTRO_AT, pad([hz(45), hz(57), hz(64), hz(67), hz(71)], SECONDS - OUTRO_AT, 0.4, 0.05, 4.5, 2600, 500))
    add(OUTRO_AT, HOLD_AT - OUTRO_AT, bass(hz(33), HOLD_AT - OUTRO_AT - 0.1, 0.28))
    for i, note in enumerate((69, 72, 76, 81)):  # one per line of the outro text
        add(OUTRO_AT + (30 + i * 15) / FPS, 2.5, bell(hz(note), 0.13, 2.2), pan=-0.45 + i * 0.3)

    # Master: gentle tanh limiting, a fade over the last 4 seconds, 16-bit.
    peak = max(max(abs(x) for x in left), max(abs(x) for x in right)) or 1.0
    drive = 1.6 / peak
    fade = int(RATE * 4)
    out = array('h')
    for i in range(N):
        g = min(1.0, (N - i) / fade)
        out.append(int(math.tanh(left[i] * drive) * 0.89 * g * 32767))
        out.append(int(math.tanh(right[i] * drive) * 0.89 * g * 32767))
    if sys.byteorder == 'big':
        out.byteswap()

    path = Path(__file__).resolve().parent / 'music.wav'
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(out.tobytes())
    print(f'wrote {path} ({SECONDS:.0f} s)')


if __name__ == '__main__':
    main()
