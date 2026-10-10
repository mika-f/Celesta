"""Generate the original, deterministic 24-second score for film.tsx.

120 BPM, 12 bars, stereo. Every variant uses the same score, so it follows
the template's fixed structure rather than any one Tip:
  bar 0      the hook: an impact on frame 0, a bright pad, a rising bell per
             beat, and a whoosh into the wipe on bar 1
  bars 1-5   the build: kick, offbeat bass and hats; code lines land on beats
  bars 6-7   the demo runs complete: claps and a higher arpeggio, then a
             snare roll and riser into the closing line
  bars 8-11  the closing line: an impact, a pad, half-time drums, a bell
             when the sign appears, a last chord and a fade

Run from any directory: python3 examples/shorts-tips/make-score.py
Only Python's standard library is required. Writes score.wav next to this file.
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
BARS = 12
SECONDS = BAR * BARS
N = int(RATE * SECONDS)
TAU = 2 * math.pi

left = [0.0] * N
right = [0.0] * N

# Picture cues, in seconds. Keep in sync with constants.ts.
WIPE_AT = BAR * 1      # the hook wipes into the build
CLOSING_AT = BAR * 8   # the closing line pops in
SIGN_AT = BAR * 9      # "CELESTA TIPS #nn" under it


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


def whoosh(length: float, peak: float, gain: float, seed: int):
    """Band-limited noise that swells toward `peak` seconds, then falls away."""
    rnd = noise_source(seed)
    lp = [0.0, 0.0]

    def v(t: float) -> float:
        env = (t / peak) ** 2.5 if t < peak else math.exp(-(t - peak) * 9)
        a = 0.02 + 0.3 * t / length
        lp[0] += (rnd() - lp[0]) * a
        lp[1] += (lp[0] - lp[1]) * a
        return (lp[0] - lp[1] * 0.5) * env * gain
    return v


# ── Harmony ─────────────────────────────────────────────────────────────────

# D major, bright: D - Bm - G - A, one chord per bar.
CHORDS = [
    (38, [62, 66, 69, 73]),  # Dmaj7
    (35, [59, 62, 66, 69]),  # Bm7
    (43, [59, 62, 67, 71]),  # Gmaj7
    (45, [61, 64, 69, 73]),  # A
]


def chord_at(bar: int):
    return CHORDS[bar % 4]


# ── Arrangement ─────────────────────────────────────────────────────────────


def groove(bar: int, arp: int, claps: bool) -> None:
    """One bar of the main groove. `arp` 0 = none, 1 = eighths, 2 = sixteenths up high."""
    t0 = bar * BAR
    root, notes = chord_at(bar)
    for b in range(4):
        add(t0 + b * BEAT, 0.5, kick(0.9))
        add(t0 + b * BEAT + BEAT / 2, 0.12, hat(0.15, 30, bar * 8 + b), pan=0.35)
        add(t0 + b * BEAT + BEAT / 2, BEAT / 2 - 0.02, bass(hz(root), BEAT / 2 - 0.02))
    if claps:
        for b in (1, 3):
            add(t0 + b * BEAT, 0.35, clap(0.32, bar * 4 + b), pan=-0.05)
    if arp:
        steps = 8 if arp == 1 else 16
        pattern = [0, 1, 2, 3, 2, 1, 3, 0]
        for s in range(steps):
            note = notes[pattern[s % 8]] + (12 if arp == 2 and s % 8 >= 4 else 0)
            add(t0 + s * BAR / steps, 0.35, pluck(hz(note + 12), 0.07), pan=-0.5 + (s % 8) / 7)


def main() -> None:
    # Bar 0: the hook. Something on frame 0, then a bell per beat rising.
    add(0, 2.5, impact(31), gain=0.75)
    add(0, BAR, pad([hz(n) for n in (62, 66, 69, 73, 76)], BAR, 0.3, 0.02, 0.3, 3200, 1400))
    for b in range(4):
        note = [74, 78, 81, 86][b]
        add(b * BEAT, 1.4, bell(hz(note), 0.2, 2.6), pan=[-0.3, 0.3, -0.15, 0.15][b])
        add(b * BEAT + 0.125, 1.0, bell(hz(note + 7), 0.06, 3.2), pan=[0.3, -0.3, 0.15, -0.15][b])
        add(b * BEAT, 0.5, kick(0.7))
    for e in range(8):
        add(e * BEAT / 2 + BEAT / 4, 0.08, hat(0.05, 45, 300 + e), pan=0.4 if e % 2 else -0.4)
    add(WIPE_AT - 0.6, 0.85, whoosh(0.85, 0.6, 0.4, 11))

    # Bars 1-5: the build, lighter, the arpeggio entering on bar 2.
    for bar in range(1, 6):
        groove(bar, arp=0 if bar == 1 else 1, claps=bar >= 4)

    # Bars 6-7: everything is on screen.
    for bar in (6, 7):
        groove(bar, arp=2, claps=True)
    add(BAR * 6, 2.0, bell(hz(86), 0.12, 1.8), pan=-0.2)
    add(BAR * 6, 2.0, bell(hz(93), 0.08, 1.8), pan=0.2)
    hits = [1.0, 1.25, 1.5, 1.625, 1.75, 1.8125, 1.875, 1.9375]
    for k, h in enumerate(hits):
        add(BAR * 7 + h, 0.2, snare(0.1 + 0.18 * k / len(hits), 40 + k), pan=0.2 if k % 2 else -0.2)
    add(CLOSING_AT - 1.2, 1.25, whoosh(1.25, 1.2, 0.5, 12))

    # Bars 8-11: the closing line. Half-time drums under a long pad.
    add(CLOSING_AT, 3.0, impact(32), gain=0.9)
    add(CLOSING_AT, BAR * 4, pad([hz(n) for n in (62, 66, 69, 73, 76)], BAR * 4, 0.36, 0.05, 2.5, 2600, 700))
    add(CLOSING_AT, BAR * 4, bass(hz(26), BAR * 4 - 0.1, 0.26))
    for bar in range(8, 11):
        t0 = bar * BAR
        for b in (0, 2):
            add(t0 + b * BEAT, 0.5, kick(0.75))
        add(t0 + 3 * BEAT, 0.35, clap(0.26, 90 + bar), pan=-0.05)
        for e in range(8):
            add(t0 + e * BEAT / 2 + BEAT / 4, 0.06, hat(0.05, 60, bar * 8 + e), pan=0.4 if e % 2 else -0.4)
    for i, note in enumerate((74, 78, 81, 85)):  # the closing line's arpeggiated chord
        add(CLOSING_AT + i * 0.09, 3.0, bell(hz(note), 0.14, 1.8), pan=-0.3 + i * 0.2)
    add(SIGN_AT, 3.0, bell(hz(90), 0.14, 1.6), pan=0.1)
    add(SIGN_AT + 0.12, 3.0, bell(hz(93), 0.09, 1.6), pan=-0.1)
    add(BAR * 11, BAR, bell(hz(62), 0.16, 1.0))
    add(BAR * 11, BAR, bell(hz(74), 0.12, 1.0))

    # Master: gentle tanh limiting, a fade on the last second, 16-bit.
    peak = max(max(abs(x) for x in left), max(abs(x) for x in right)) or 1.0
    drive = 1.6 / peak
    fade = int(RATE * 1.0)
    out = array('h')
    for i in range(N):
        g = min(1.0, (N - i) / fade)
        out.append(int(math.tanh(left[i] * drive) * 0.89 * g * 32767))
        out.append(int(math.tanh(right[i] * drive) * 0.89 * g * 32767))
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
