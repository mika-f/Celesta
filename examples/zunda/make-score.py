"""Generate the original, deterministic background music for film.tsx.

A light, bouncy loop in C major at 112 BPM, the kind that sits under a
解説動画: marimba melody, off-beat piano stabs, plucked bass, soft drums.
Chords: F G Em Am | F G C C, repeated with small melodic variations.

Run from any directory: python3 examples/zunda/make-score.py [seconds]
Only Python's standard library is required. Writes assets/score.wav.
"""

from array import array
import math
from pathlib import Path
import sys
import wave

RATE = 48_000
BPM = 112
BEAT = 60 / BPM
SECONDS = float(sys.argv[1]) if len(sys.argv) > 1 else 132.0
N = int(RATE * SECONDS)
TAU = 2 * math.pi

left = array('f', bytes(4 * N))
right = array('f', bytes(4 * N))


def hz(midi: float) -> float:
    return 440.0 * 2 ** ((midi - 69) / 12)


class Noise:
    def __init__(self, seed: int):
        self.state = seed & 0xFFFFFFFF

    def __call__(self) -> float:
        self.state = (1664525 * self.state + 1013904223) & 0xFFFFFFFF
        return self.state / 0x7FFFFFFF - 1.0


def add(start: float, length: float, voice, pan: float = 0.0, gain: float = 1.0) -> None:
    s0 = int(start * RATE)
    count = min(int(length * RATE), N - s0)
    lg = gain * math.sqrt((1 - pan) / 2)
    rg = gain * math.sqrt((1 + pan) / 2)
    for i in range(max(0, count)):
        v = voice(i / RATE)
        left[s0 + i] += v * lg
        right[s0 + i] += v * rg


def marimba(f: float):
    def voice(t: float) -> float:
        env = math.exp(-t * 9) * min(1.0, t * 400)
        return env * (math.sin(TAU * f * t) + 0.25 * math.sin(TAU * f * 4 * t) * math.exp(-t * 30))
    return voice


def piano(f: float):
    def voice(t: float) -> float:
        env = math.exp(-t * 7) * min(1.0, t * 300)
        return env * (math.sin(TAU * f * t) + 0.35 * math.sin(TAU * f * 2 * t) + 0.1 * math.sin(TAU * f * 3 * t))
    return voice


def bass(f: float):
    def voice(t: float) -> float:
        env = math.exp(-t * 5) * min(1.0, t * 300)
        phase = (f * t) % 1.0
        saw = 2 * phase - 1
        return env * (0.7 * math.sin(TAU * f * t) + 0.25 * saw)
    return voice


def kick(t: float) -> float:
    f = 50 + 90 * math.exp(-t * 30)
    return math.exp(-t * 14) * math.sin(TAU * f * t)


def noise_hit(seed: int, decay: float):
    rnd = Noise(seed)
    last = [0.0]

    def voice(t: float) -> float:
        n = rnd()
        hp = n - last[0]
        last[0] = n
        return math.exp(-t * decay) * hp
    return voice


# C major: F G Em Am | F G C C. Root notes (MIDI) and triads.
PROGRESSION = [
    (53, [65, 69, 72]), (55, [67, 71, 74]), (52, [64, 67, 71]), (57, [64, 69, 72]),
    (53, [65, 69, 72]), (55, [67, 71, 74]), (48, [64, 67, 72]), (48, [64, 67, 72]),
]
PENTA = [72, 74, 76, 79, 81, 84, 86]
rnd = Noise(2026)

bar_len = BEAT * 4
bars = int(SECONDS / bar_len) + 1
melody_index = 2
for b in range(bars):
    t0 = b * bar_len
    root, triad = PROGRESSION[b % len(PROGRESSION)]
    phrase = (b // 8) % 4
    # Drums: kick on 1 and 3, a soft clap on 2 and 4, hats on the eighths.
    for beat in range(4):
        tb = t0 + beat * BEAT
        if beat in (0, 2):
            add(tb, 0.4, kick, gain=0.55)
        else:
            add(tb, 0.25, noise_hit(b * 4 + beat, 22), gain=0.16, pan=0.1)
        for half in (0, 0.5):
            add(tb + half * BEAT, 0.06, noise_hit(b * 8 + beat * 2 + int(half * 2) + 999, 90),
                gain=0.07 if half else 0.04, pan=-0.3)
    # Plucked bass: root on the beat, octave bounce on the off-beats.
    for eighth in range(8):
        note = root - 12 + (12 if eighth % 2 else 0)
        if eighth == 6:
            note = root - 12 + 7
        add(t0 + eighth * BEAT / 2, 0.3, bass(hz(note)), gain=0.32)
    # Piano stabs on the off-beats.
    for beat in range(4):
        for note in triad:
            add(t0 + (beat + 0.5) * BEAT, 0.35, piano(hz(note)), pan=0.25, gain=0.07)
    # Marimba melody: a seeded walk on the pentatonic scale, resting now and then.
    if phrase != 0 or b >= 8:
        for step in range(8):
            if rnd() > 0.55 and step % 2:
                continue
            melody_index = max(0, min(len(PENTA) - 1, melody_index + int(round(rnd() * 2))))
            add(t0 + step * BEAT / 2, 0.5, marimba(hz(PENTA[melody_index])), pan=-0.2, gain=0.16)

# Gentle limiter and fade, then 16-bit stereo.
peak = max(max(abs(v) for v in left), max(abs(v) for v in right))
scale = 0.8 / peak if peak > 0 else 1.0
fade = int(RATE * 2)
out = array('h')
for i in range(N):
    g = scale * min(1.0, (N - i) / fade)
    out.append(int(max(-1.0, min(1.0, left[i] * g)) * 32767))
    out.append(int(max(-1.0, min(1.0, right[i] * g)) * 32767))

target = Path(__file__).resolve().parent / 'assets' / 'score.wav'
target.parent.mkdir(parents=True, exist_ok=True)
with wave.open(str(target), 'wb') as w:
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(RATE)
    w.writeframes(out.tobytes())
print(f'wrote {target} ({SECONDS:.0f} s)')
