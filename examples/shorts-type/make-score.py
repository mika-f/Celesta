"""Synthesize the 28-second, 120 BPM score for "Shorts Type" with the standard library.

Keep the section boundaries in sync with SCENES in timeline.ts: one bar is
two seconds, every scene is two bars, and the hits below land on the cues
the scenes animate on.
"""
from array import array
import math
from pathlib import Path
import random
import sys
import wave

RATE, BARS = 48000, 14
BEAT = 0.5
BAR = 4 * BEAT
SCENE = 2 * BAR
SECONDS = BARS * BAR
N = int(RATE * SECONDS)
TAU = math.tau
rng = random.Random(1920)
left = array("f", [0]) * N
right = array("f", [0]) * N


def add(at, duration, voice, gain=1, pan=0):
    start = round(at * RATE)
    a = gain * math.sqrt((1 - pan) / 2)
    b = gain * math.sqrt((1 + pan) / 2)
    for j in range(min(round(duration * RATE), N - start)):
        sample = voice(j / RATE)
        left[start + j] += sample * a
        right[start + j] += sample * b


def kick(t):
    return math.sin(TAU * (46 * t + 9 * (1 - math.exp(-38 * t)))) * math.exp(-10 * t)


def hat(t):
    return rng.uniform(-1, 1) * math.exp(-170 * t)


def clap(t):
    # Three quick noise bursts, then a short tail.
    bursts = sum(math.exp(-260 * (t - d)) for d in (0, 0.011, 0.022) if t >= d)
    return rng.uniform(-1, 1) * (bursts + 0.5 * math.exp(-22 * t))


def click(t):
    return rng.uniform(-1, 1) * math.exp(-400 * t) + 0.3 * math.sin(TAU * 2400 * t) * math.exp(-200 * t)


def zip_down(t):
    # A falling sweep for the column narrowing.
    return math.sin(TAU * (1400 * t - 900 * t * t)) * math.exp(-9 * t) * min(1, t / 0.005)


def bass(hz):
    return lambda t: math.tanh(1.8 * (
        math.sin(TAU * hz * t) + 0.35 * math.sin(TAU * hz * 2 * t)
    )) * (1 - math.exp(-90 * t)) * math.exp(-3 * t)


def stab(hzs):
    return lambda t: sum(
        math.tanh(2 * math.sin(TAU * hz * t)) + 0.3 * math.sin(TAU * hz * 2.003 * t) for hz in hzs
    ) / len(hzs) * (1 - math.exp(-300 * t)) * math.exp(-7 * t)


def bell(hz, decay):
    return lambda t: (math.sin(TAU * hz * t)
        + 0.32 * math.sin(TAU * hz * 2.01 * t)
        + 0.12 * math.sin(TAU * hz * 4.02 * t)
    ) * (1 - math.exp(-200 * t)) * math.exp(-decay * t)


def pad(hzs, length):
    return lambda t: sum(math.sin(TAU * hz * t + i) for i, hz in enumerate(hzs)) / len(hzs) \
        * min(1, t / 0.5) * min(1, (length - t) / 0.8)


def swell(length):
    return lambda t: rng.uniform(-1, 1) * (t / length) ** 3


def impact(at, gain=1):
    add(at, 0.6, kick, 0.6 * gain)
    add(at, 0.5, lambda t: rng.uniform(-1, 1) * math.exp(-9 * t), 0.18 * gain)


# Dm  B♭  F  C, one chord per bar.
ROOTS = (73.42, 58.27, 87.31, 65.41)
CHORDS = ((293.66, 349.23, 440.0), (233.08, 293.66, 349.23),
          (261.63, 349.23, 440.0), (261.63, 329.63, 392.0))
PENTA = (587.33, 659.26, 783.99, 880.0, 987.77, 1174.66, 1318.51)

# The groove under scenes 1–6 (bars 0–11).
for beat in range(0, 48):
    at = beat * BEAT
    bar = beat // 4
    add(at, 0.45, kick, 0.42 if beat % 4 == 0 else 0.3)
    add(at + BEAT / 2, 0.05, hat, 0.08, 0.3)
    add(at, 0.46, bass(ROOTS[bar % 4]), 0.2)
    if beat % 4 == 0:
        add(at, BAR, pad(CHORDS[bar % 4], BAR), 0.04, -0.1)

# A swell into every scene, an impact on its first frame, and on every cut
# a stutter of clicks under the glitch slices (Fx in film.tsx: 7 frames,
# a new slice pattern every 2 frames).
for scene in range(7):
    at = scene * SCENE
    if at > 0:
        add(at - 0.4, 0.4, swell(0.4), 0.1)
        for k in range(4):
            add(at + k * 2 / 30, 0.04, click, 0.1 * (1 - k / 4), rng.uniform(-0.6, 0.6))
    impact(at, 1.4 if scene in (0, 6) else 1)

# HOOK: the headline is up by frame 12, a stab on each bar.
for bar in (0, 1):
    add(bar * BAR, 0.5, stab(CHORDS[0]), 0.16)

# FRAME: one bell per line of the reveal (beats 1–3), then the counter ticks
# in sixteenths from beat 4.
base = 1 * SCENE
for i, hz in enumerate((587.33, 698.46, 880.0)):
    add(base + (i + 1) * BEAT, 1.2, bell(hz, 3), 0.1, 0.2)
for k in range(16):
    add(base + 4 * BEAT + k * BEAT / 4, 0.03, click, 0.05, rng.uniform(-0.4, 0.4))

# PHRASE: the column narrows every other beat.
base = 2 * SCENE
for i in range(4):
    add(base + i * 2 * BEAT, 0.35, zip_down, 0.12, -0.3 + 0.2 * i)

# FIT: the box snaps to a new shape every other beat.
base = 3 * SCENE
for i in range(4):
    add(base + i * 2 * BEAT, 0.3, clap, 0.16, 0.15)

# SPAN: one rising bell per lit phrase, then every phrase at once.
base = 4 * SCENE
for i in range(6):
    add(base + i * BEAT, 1.0, bell(PENTA[i], 3.2), 0.1, -0.3 + 0.12 * i)
for hz in (587.33, 783.99, 987.77, 1174.66):
    add(base + 6 * BEAT, 1.6, bell(hz, 2), 0.06)

# BEAT: a word per beat, a stab per word, climbing.
base = 5 * SCENE
for i in range(8):
    chord = CHORDS[(i // 2) % 4]
    lift = 2 if i >= 6 else 1
    add(base + i * BEAT, 0.4, stab(tuple(hz * lift for hz in chord)), 0.17, -0.2 if i % 2 else 0.2)
    add(base + i * BEAT, 0.2, clap, 0.07)

# OUTRO: no drums, a pad and the bells, then silence for the loop.
base = 6 * SCENE
add(base, 3.6, pad(CHORDS[0], 3.6), 0.09)
for i, hz in enumerate((587.33, 698.46, 880.0, 1174.66)):
    add(base + i * BEAT, 2.4, bell(hz, 1.4), 0.07, -0.3 + 0.2 * i)

peak = max(max(abs(v) for v in left), max(abs(v) for v in right))
assert peak > 0
gain = 0.8 / peak
pcm = array("h")
for i, (l, r) in enumerate(zip(left, right)):
    fade = min(1, (N - 1 - i) / (RATE * 0.4))
    pcm.extend((round(l * gain * fade * 32767), round(r * gain * fade * 32767)))
if sys.byteorder != "little":
    pcm.byteswap()
out = Path(__file__).parent / "score.wav"
with wave.open(str(out), "wb") as wav:
    wav.setnchannels(2)
    wav.setsampwidth(2)
    wav.setframerate(RATE)
    wav.writeframes(pcm.tobytes())
print(f"Wrote {out}: {SECONDS:g} seconds, 48 kHz stereo")
