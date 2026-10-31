"""Original mechanical sound design. 48 kHz, stereo, 18 s; no samples/dependencies."""
from array import array
import math
from pathlib import Path
import random
import sys
import wave

RATE = 48000
FRAMES = 540
FPS = 30
rng = random.Random(614)
left = array('f', [0]) * (RATE * FRAMES // FPS)
right = array('f', [0]) * len(left)


def sound(frame, duration, voice, gain, pan=0):
    offset = round(frame / FPS * RATE)
    for j in range(min(round(duration * RATE), len(left) - offset)):
        t = j / RATE
        envelope = min(1, t / .0015) * min(1, (duration - t) / .012)
        value = voice(t) * envelope * gain
        left[offset + j] += value * math.sqrt((1 - pan) / 2)
        right[offset + j] += value * math.sqrt((1 + pan) / 2)


def impact(t):
    phase = math.tau * (48 * t + 4.2 * (1 - math.exp(-42 * t)))
    return math.sin(phase) * math.exp(-12 * t) + .17 * rng.uniform(-1, 1) * math.exp(-120 * t)


def click(t):
    return (.65 * math.sin(math.tau * 1450 * t) + .35 * rng.uniform(-1, 1)) * math.exp(-130 * t)


def scrape(t):
    return (rng.uniform(-1, 1) * .45 + math.sin(math.tau * (190 * t + 950 * t * t)) * .55) * math.sin(math.pi * t / .18) ** 2


for frame, gain in [(0, .63), (16, .42), (60, .38), (92, .5), (128, .46),
                    (174, .6), (204, .35), (240, .24), (299, .64),
                    (342, .62), (359, .62), (389, .68), (414, .55), (519, .76)]:
    sound(frame, .45, impact, gain)

for i, frame in enumerate([5, 7, 9, 11, 13, 68, 74, 80, 86, 104, 108, 112, 116,
                           156, 158, 160, 162, 164, 194, 198, 211, 214, 231, 234,
                           265, 269, 273, 277, 280, 284, 288, 318, 324, 330,
                           335, 340, 345, 349, 353, 356, 359, 377, 381, 385]):
    sound(frame, .09, click, .13 if frame < 318 else .19, .38 * (-1 if i % 2 else 1))

for frame in [44, 140, 265, 318, 398, 510]:
    sound(frame, .18, scrape, .16, -.12)

# 14.3–17 seconds is deliberately silent, matching the YOU hold.
sound(519, .65, lambda t: math.sin(math.tau * 880 * t) * math.exp(-12 * t), .11, .2)

peak = max(max(abs(x) for x in left), max(abs(x) for x in right))
gain = .79 / peak
pcm = array('h')
for l, r in zip(left, right):
    pcm.extend((round(l * gain * 32767), round(r * gain * 32767)))
if sys.byteorder != 'little':
    pcm.byteswap()
output = Path(__file__).parent / 'assets' / 'score.wav'
output.parent.mkdir(parents=True, exist_ok=True)
with wave.open(str(output), 'wb') as wav:
    wav.setnchannels(2)
    wav.setsampwidth(2)
    wav.setframerate(RATE)
    wav.writeframes(pcm.tobytes())
print(f'Wrote {output}: 18 seconds, stereo 48 kHz, peak {20 * math.log10(.79):.2f} dBFS')
