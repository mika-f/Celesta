"""Generate the original, deterministic background score for film.tsx.

128 BPM, 32 bars (60 s), stereo. The film's length follows its voices, so
the score runs longer than any cut of the script and film.tsx fades it out
on the last second. It stays out of the voices' way: a soft kick, offbeat
hats, a low pad and a sparse bell melody, mixed quietly.
  bar 0      the hook: an impact and a bright bell chord on the first frame
  bars 1-31  the groove (C - Am - F - G), a fill and a whoosh every 8 bars

Run from any directory: python3 examples/shorts-explainer/make-score.py
Only Python's standard library is required. Writes score.wav next to this file.
"""

from array import array
import math
from pathlib import Path
import sys
import wave

RATE = 44_100
BPM = 128
BEAT = 60 / BPM
BAR = BEAT * 4
BARS = 32
SECONDS = BAR * BARS
N = int(RATE * SECONDS)
TAU = 2 * math.pi

left = [0.0] * N
right = [0.0] * N


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


def kick(gain: float):
    def v(t: float) -> float:
        phase = TAU * (50 * t + (90 / 30) * (1 - math.exp(-30 * t)))
        return math.sin(phase) * math.exp(-t * 9) * gain
    return v


def impact(seed: int):
    rnd = noise_source(seed)
    lp = [0.0]

    def v(t: float) -> float:
        phase = TAU * (36 * t + (140 / 12) * (1 - math.exp(-12 * t)))
        sub = math.sin(phase) * math.exp(-t * 2.2) * 0.9
        lp[0] += (rnd() - lp[0]) * 0.2
        return sub + lp[0] * math.exp(-t * 3.0) * 0.35
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


def snap(gain: float, seed: int):
    rnd = noise_source(seed)
    lp = [0.0]

    def v(t: float) -> float:
        lp[0] += (rnd() - lp[0]) * 0.5
        return (rnd() - lp[0]) * math.exp(-t * 40) * gain
    return v


def bass(freq: float, length: float, gain: float):
    def v(t: float) -> float:
        env = min(1.0, t * 200) * min(1.0, max(0.0, (length - t) * 40))
        return (math.sin(TAU * freq * t) + 0.25 * math.sin(TAU * freq * 2 * t)) * env * gain
    return v


def bell(freq: float, gain: float, decay: float = 3.0):
    """A small FM bell, close to the sound of a celesta."""
    def v(t: float) -> float:
        index = 1.8 * math.exp(-t * 6)
        mod = math.sin(TAU * freq * 3.5 * t) * index
        env = min(1.0, t * 1200) * math.exp(-t * decay)
        return math.sin(TAU * freq * t + mod) * env * gain
    return v


def pad(freqs, length: float, gain: float):
    lp = [0.0, 0.0]
    a = 1 - math.exp(-TAU * 900 / RATE)

    def v(t: float) -> float:
        env = min(1.0, t / 0.08, max(0.0, (length - t) / 0.12))
        saw = 0.0
        for f in freqs:
            for detune in (0.997, 1.003):
                saw += 2 * ((f * detune * t) % 1.0) - 1
        lp[0] += (saw - lp[0]) * a
        lp[1] += (lp[0] - lp[1]) * a
        return lp[1] * env * gain / len(freqs)
    return v


def whoosh(length: float, gain: float, seed: int):
    rnd = noise_source(seed)
    lp = [0.0, 0.0]

    def v(t: float) -> float:
        env = (t / length) ** 2.5
        a = 0.02 + 0.25 * t / length
        lp[0] += (rnd() - lp[0]) * a
        lp[1] += (lp[0] - lp[1]) * a
        return (lp[0] - lp[1] * 0.5) * env * gain
    return v


# ── Harmony ─────────────────────────────────────────────────────────────────

# C major: C - Am - F - G, one chord per bar. (bass root, pad notes)
CHORDS = [
    (36, [60, 64, 67, 71]),  # Cmaj7
    (33, [57, 60, 64, 67]),  # Am7
    (29, [57, 60, 65, 69]),  # Fmaj7
    (31, [55, 59, 62, 67]),  # G
]
# A bell phrase per chord: (sixteenth, midi note).
MELODY = [
    [(0, 79), (6, 76), (10, 74), (12, 76)],
    [(0, 72), (6, 76), (10, 79)],
    [(0, 81), (6, 79), (10, 77), (12, 76)],
    [(0, 74), (8, 71), (12, 74)],
]


def groove(bar: int) -> None:
    t0 = bar * BAR
    root, notes = CHORDS[bar % 4]
    add(t0, BAR, pad([hz(n) for n in notes], BAR, 0.16))
    for b in range(4):
        add(t0 + b * BEAT, 0.4, kick(0.55))
        add(t0 + b * BEAT + BEAT / 2, 0.08, hat(0.09, 40, bar * 8 + b), pan=0.3)
        add(t0 + b * BEAT + BEAT / 2, BEAT / 2 - 0.03, bass(hz(root), BEAT / 2 - 0.03, 0.22))
    for b in (1, 3):
        add(t0 + b * BEAT, 0.2, snap(0.16, bar * 4 + b), pan=-0.1)
    for s, note in MELODY[bar % 4]:
        add(t0 + s * BEAT / 4, 1.2, bell(hz(note), 0.07), pan=-0.35 + 0.1 * (s % 8))
    if bar % 8 == 7:
        for k in range(4):
            add(t0 + 3 * BEAT + k * BEAT / 4, 0.1, snap(0.08 + 0.03 * k, 500 + bar * 4 + k), pan=0.2 if k % 2 else -0.2)
        add(t0 + 2 * BEAT, 2 * BEAT, whoosh(2 * BEAT, 0.25, bar))


def main() -> None:
    # The hook: an impact and a bright bell chord on frame 0.
    add(0, 2.5, impact(7), gain=0.6)
    for k, note in enumerate([72, 76, 79, 84]):
        add(k * 0.04, 2.0, bell(hz(note), 0.12, 2.2), pan=-0.3 + 0.2 * k)
    for bar in range(BARS):
        groove(bar)

    # Master: gentle tanh limiting, a fade on the last second, 16-bit.
    peak = max(max(abs(x) for x in left), max(abs(x) for x in right)) or 1.0
    drive = 1.4 / peak
    fade = int(RATE * 1.0)
    out = array('h')
    for i in range(N):
        g = min(1.0, (N - i) / fade)
        out.append(int(math.tanh(left[i] * drive) * 0.85 * g * 32767))
        out.append(int(math.tanh(right[i] * drive) * 0.85 * g * 32767))
    if sys.byteorder == 'big':
        out.byteswap()

    path = Path(__file__).resolve().parent / 'score.wav'
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(out.tobytes())
    print(f'wrote {path} ({SECONDS:.0f} s)')


if __name__ == '__main__':
    main()
