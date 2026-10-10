"""Synthesize the looping score for SHORTS LOOP with the standard library.

Four bars of D minor (i-VI-III-VII) at 75 BPM: exactly the film's 12.8 seconds.
The tempo, bar count, and frame rate come from constants.ts, so the score and
the picture cannot drift apart.

The mix is a ring: a note that rings past the end of the fourth bar wraps to
the start of the first, the delay's echoes wrap the same way, and nothing
fades in or out. Played on repeat, sample N - 1 runs into sample 0 like any
other pair of neighbours.
"""
from array import array
import math
from pathlib import Path
import random
import re
import sys
import wave

HERE = Path(__file__).resolve().parent
constants = (HERE / "constants.ts").read_text()


def constant(name):
    match = re.search(rf"^export const {name} = (\d+);", constants, re.M)
    if not match:
        sys.exit(f"constants.ts has no whole-number {name}")
    return int(match.group(1))


FPS, BPM, BEATS_PER_BAR, BARS = (constant(n) for n in ("FPS", "BPM", "BEATS_PER_BAR", "BARS"))
RATE = 48000
STEP = RATE * 60 // (BPM * 4)  # samples per sixteenth note
if STEP * BPM * 4 != RATE * 60 or (60 * FPS) % BPM:
    sys.exit(f"{BPM} BPM does not divide into whole samples and frames")
N = STEP * 4 * BEATS_PER_BAR * BARS
SECONDS = N / RATE
STEPS_PER_BAR = 4 * BEATS_PER_BAR
TAU = math.tau
rng = random.Random(75)

# Two buses, so the kick can duck the music without ducking itself.
drums = [array("f", [0]) * N, array("f", [0]) * N]
music = [array("f", [0]) * N, array("f", [0]) * N]
echo = [array("f", [0]) * N, array("f", [0]) * N]


def add(bus, step, seconds, voice, gain=1.0, pan=0.0):
    """Adds `voice(t)` from sixteenth `step`, wrapping past the end to the start."""
    start = round(step * STEP)
    a = gain * math.sqrt((1 - pan) / 2)
    b = gain * math.sqrt((1 + pan) / 2)
    left, right = bus
    for j in range(min(round(seconds * RATE), N)):
        s = voice(j / RATE)
        i = (start + j) % N
        left[i] += s * a
        right[i] += s * b


hz = lambda semis: 55.0 * 2 ** (semis / 12)  # semitones above A1
kick = lambda t: math.sin(TAU * (46 * t + 7 * (1 - math.exp(-38 * t)))) * math.exp(-7 * t)
clap = lambda t: rng.uniform(-1, 1) * (math.exp(-30 * t) + 0.5 * math.exp(-70 * max(0, t - 0.011)))
hat = lambda decay: (lambda t: rng.uniform(-1, 1) * math.exp(-decay * t))
sub = lambda f, length: (lambda t: (math.sin(TAU * f * t) + 0.25 * math.sin(TAU * 2 * f * t))
                         * min(1, t / 0.01) * min(1, max(0, length - t) / 0.15))
pad = lambda f, length: (lambda t: (math.sin(TAU * f * t) + 0.6 * math.sin(TAU * f * 1.006 * t)
                                    + 0.6 * math.sin(TAU * f * 0.994 * t) + 0.2 * math.sin(TAU * 2 * f * t))
                         * min(1, t / 0.9) * min(1, max(0, length - t) / 1.2))
bell = lambda f, decay: (lambda t: (math.sin(TAU * f * t) + 0.35 * math.sin(TAU * f * 2.76 * t) * math.exp(-6 * t)
                                    + 0.15 * math.sin(TAU * f * 5.4 * t) * math.exp(-12 * t))
                         * min(1, t / 0.003) * math.exp(-decay * t))

# D minor: Dm9, Bbmaj7, Fmaj7, Cadd9. Semitones above A1; bass roots an octave or two down.
CHORDS = [[17, 20, 24, 27, 31], [13, 17, 20, 24], [20, 24, 27, 31], [15, 19, 22, 29]]
ROOTS = [5, 1, 8, 3]
ARP = [0, 2, 1, 3, 2, 0, 3, 1, 0, 2, 1, 3, 2, 3, 1, 2]
KICKS = [0, 10]
BAR_SECONDS = STEP * STEPS_PER_BAR / RATE

for bar in range(BARS):
    first = bar * STEPS_PER_BAR
    chord, root = CHORDS[bar], ROOTS[bar]
    for k in KICKS:
        add(drums, first + k, 0.6, kick, 0.55)
    for k in (4, 12):
        add(drums, first + k, 0.35, clap, 0.16, -0.1)
    for k in range(STEPS_PER_BAR):
        add(drums, first + k, 0.25 if k == 14 else 0.06, hat(16 if k == 14 else 130),
            0.05 if k % 2 == 0 else 0.022, 0.35)
    add(music, first, 1.9, sub(hz(root), 1.9), 0.30)
    add(music, first + 10, 1.15, sub(hz(root), 1.15), 0.26)
    for n, semis in enumerate(chord):
        # A bar plus a long release: the last chord's tail wraps into the first bar.
        add(music, first, BAR_SECONDS + 1.2, pad(hz(semis + 12), BAR_SECONDS + 1.2), 0.035, -0.5 + n / max(1, len(chord) - 1))
    for k, index in enumerate(ARP):
        semis = chord[index % len(chord)] + 24
        add(echo, first + k, 0.7, bell(hz(semis), 7), 0.05 if k % 4 == 0 else 0.032, -0.4 if k % 2 else 0.4)
    add(echo, first, 2.4, bell(hz(chord[0] + 36), 2.2), 0.05, 0.2)
    add(echo, first + 8, 2.0, bell(hz(chord[2] + 36), 2.6), 0.035, -0.2)

# A ping-pong delay of a dotted eighth (three sixteenths) on the bells. Its
# feedback runs round the ring until it settles, so the echoes from the end
# of the loop sound at its start, as they would on the second time through.
DELAY, FEEDBACK = 3 * STEP, 0.42
left, right = echo
dry_left, dry_right = array("f", left), array("f", right)
for _ in range(6):
    for i in range(N):
        left[i] = dry_left[i] + FEEDBACK * right[i - DELAY]
        right[i] = dry_right[i] + FEEDBACK * left[i - DELAY]
for channel in range(2):
    for i in range(N):
        music[channel][i] += echo[channel][i]

# Duck the music under each kick; the envelope repeats with the loop too.
duck = array("f", [1]) * N
for bar in range(BARS):
    for k in KICKS:
        start = (bar * STEPS_PER_BAR + k) * STEP
        for j in range(round(0.5 * RATE)):
            duck[(start + j) % N] *= 1 - 0.55 * math.exp(-j / (0.11 * RATE))

mix = [array("f", (d + m * g for d, m, g in zip(drums[c], music[c], duck))) for c in range(2)]
peak = max(max(abs(v) for v in channel) for channel in mix)
gain = 0.85 / peak
pcm = array("h")
for l, r in zip(*mix):
    pcm.extend((round(l * gain * 32767), round(r * gain * 32767)))
if sys.byteorder != "little":
    pcm.byteswap()
out = HERE / "assets" / "score.wav"
out.parent.mkdir(parents=True, exist_ok=True)
with wave.open(str(out), "wb") as file:
    file.setnchannels(2)
    file.setsampwidth(2)
    file.setframerate(RATE)
    file.writeframes(pcm.tobytes())
print(f"Wrote {out}: {SECONDS} seconds ({N} samples), {BARS} bars at {BPM} BPM, 48 kHz stereo")
