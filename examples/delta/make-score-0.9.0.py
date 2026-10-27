"""Original 100 BPM newsroom theme for DELTA 0.9.0. Python standard library; no samples.

Bars follow film-0.9.0.tsx: one bar = 2.4 s = 72 frames, one beat = 18 frames.
  bar 0      signal: a low drone, and a ping for every monitor of the multiviewer
             that lights, one a sixteenth
  bar 1      the countdown: a 1 kHz beep on beats 2, 3 and 4, a timpani roll
             and a riser under them
  bars 2-3   TAKE: the ident lands on bar 2 (a timpani hit, a crash, the brass
             chord), and the theme's motif plays over drums and the synth pulse;
             a snare fill as the camera dives into the program monitor
  bars 4-15  LOWER THIRD to DUCKING: the full bed in C minor
             (Cm - Ab - Eb - Bb): four-on-the-floor kick, claps on 2 and 4,
             sixteenth hats, an eighth-note bass, the sixteenth pulse, and a
             brass stab and a whoosh at every cut. Full under the voices too,
             so the film's ducking is heard
  bars 16-17 LOUDNESS: the breakdown; the kick on 1 only, strings, the
             pulse low-passed
  bars 18-27 PROPERTIES to ADD, the editor: the bed again, with a
             glockenspiel line
  bars 28-33 MORE: a stab every two beats, a card on each; a snare build
  bars 34-37 outro: the ident again, then the last chord and a timpani roll
             ring out under the film's fadeOut
The film ducks this bed under three voice lines with <Audio duck>; nothing
here makes room for them.
Writes score-0.9.0.wav next to this script (48 kHz, 16-bit stereo, 91.2 s).
"""
from array import array
import math
from pathlib import Path
import random
import wave

RATE = 48000
BPM = 100
BEAT = 60 / BPM
BAR = 4 * BEAT
STEP = BEAT / 4
OPEN_BARS = 4
CHAPTERS = 12
CHAPTER_BARS = 2
MORE_BARS = 6
OUTRO_BARS = 4
TOTAL_BARS = OPEN_BARS + CHAPTERS * CHAPTER_BARS + MORE_BARS + OUTRO_BARS
SECONDS = TOTAL_BARS * BAR
SAMPLES = round(SECONDS * RATE)
TAU = math.tau
rng = random.Random(909)

drums_l = array('d', [0]) * SAMPLES
drums_r = array('d', [0]) * SAMPLES
music_l = array('d', [0]) * SAMPLES
music_r = array('d', [0]) * SAMPLES
pump = array('d', [1]) * SAMPLES

CHAPTER_START = OPEN_BARS * BAR
MORE_START = CHAPTER_START + CHAPTERS * CHAPTER_BARS * BAR
OUTRO_START = MORE_START + MORE_BARS * BAR
BREAK = (6,)    # zero-based: LOUDNESS
LIFT = 7        # PROPERTIES on, the editor: the glockenspiel


def add(start, duration, voice, gain, pan=0.0, bus='music'):
    """Mixes voice(t) in from `start` seconds for `duration` seconds."""
    left, right = (drums_l, drums_r) if bus == 'drums' else (music_l, music_r)
    offset = round(start * RATE)
    count = min(round(duration * RATE), SAMPLES - offset)
    l_gain = gain * math.sqrt((1 - pan) / 2)
    r_gain = gain * math.sqrt((1 + pan) / 2)
    tail = RATE * 0.01
    for i in range(max(0, count)):
        t = i / RATE
        s = voice(t) * min(1.0, (count - 1 - i) / tail)
        left[offset + i] += s * l_gain
        right[offset + i] += s * r_gain


def sidechain(start, depth=0.35, release=0.2):
    offset = round(start * RATE)
    count = min(round(release * 1.6 * RATE), SAMPLES - offset)
    for i in range(max(0, count)):
        g = 1 - depth * math.exp(-(i / RATE) / (release / 3))
        if g < pump[offset + i]:
            pump[offset + i] = g


def hz(midi):
    return 440 * 2 ** ((midi - 69) / 12)


def lp_coef(cutoff):
    return 1 - math.exp(-TAU * min(cutoff, RATE * 0.45) / RATE)


# --- drums: tight, bright -------------------------------------------------------

def kick(t):
    phase = TAU * (50 * t + 110 * (1 - math.exp(-40 * t)) / 40)
    click = math.exp(-400 * t) * 0.4
    return math.tanh(2.0 * math.sin(phase) * math.exp(-9 * t)) * (1 - math.exp(-2000 * t)) + click


def clap(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - 0.6 * state[0], x  # brighten
    bursts = sum(math.exp(-180 * (t - d)) for d in (0, 0.011, 0.022) if t >= d)
    return x * (0.5 * bursts + 0.6 * math.exp(-16 * t)) * (1 - math.exp(-3000 * t))


def hat(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - state[0], x
    return x * math.exp(-80 * t)


def crash(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - 0.9 * state[0], x
    shimmer = 0.3 * math.sin(TAU * 5400 * t) * math.sin(TAU * 7300 * t)
    return (x + shimmer) * (0.7 * math.exp(-1.6 * t) + 0.3 * math.exp(-14 * t)) * (1 - math.exp(-800 * t))


def timpani(freq, decay=2.4):
    s = [0.0]

    def voice(t):
        f = freq * (1 + 0.04 * math.exp(-20 * t))
        s[0] += 0.08 * (rng.uniform(-1, 1) - s[0])
        body = math.sin(TAU * f * t) + 0.4 * math.sin(TAU * f * 1.5 * t) * math.exp(-4 * t)
        return (body * math.exp(-decay * t) + 1.6 * s[0] * math.exp(-30 * t)) * (1 - math.exp(-900 * t))
    return voice


def roll(start, length, freq, peak):
    """A timpani roll: strokes every 32nd, swelling to `peak`."""
    strokes = int(length / (STEP / 2))
    for i in range(strokes):
        p = i / max(1, strokes - 1)
        add(start + i * STEP / 2, 0.6, timpani(freq, 5), peak * (0.15 + 0.85 * p ** 2), -0.2 + 0.4 * (i % 2), bus='drums')


def boom(t):
    phase = TAU * (32 * t + 50 * (1 - math.exp(-9 * t)) / 9)
    return math.sin(phase) * math.exp(-1.5 * t) * (1 - math.exp(-500 * t))


def whoosh(length):
    """Noise that rises into the cut and stops on it."""
    s = [0.0]

    def voice(t):
        p = t / length
        s[0] += (0.01 + 0.4 * p ** 2) * (rng.uniform(-1, 1) - s[0])
        return s[0] * 2.2 * p ** 2.5
    return voice


def riser(length):
    s = [0.0, 0.0]

    def voice(t):
        p = t / length
        s[0] += (0.005 + 0.3 * p ** 2) * (rng.uniform(-1, 1) - s[0])
        s[1] += TAU * (220 + 1400 * p ** 2) / RATE
        return (s[0] * 1.6 + 0.25 * math.sin(s[1])) * p ** 2
    return voice


def ping(freq):
    def voice(t):
        return math.sin(TAU * freq * t * (1 + 0.02 * math.exp(-50 * t))) * math.exp(-18 * t) * (1 - math.exp(-3000 * t))
    return voice


def beep(t):
    return math.sin(TAU * 1000 * t) * min(1.0, t / 0.003)


def drone(length):
    def voice(t):
        env = min(1.0, t / (length * 0.8))
        return env * (math.sin(TAU * hz(24) * t) + 0.5 * math.sin(TAU * hz(31) * t) + 0.2 * math.sin(TAU * hz(36) * t * 1.003))
    return voice


def at(bar_start, step):
    return bar_start + step * STEP


def groove(start, gain=1.0, kick_beats=(0, 1, 2, 3), claps=True, hats=16, open_hat=True):
    """One bar of the bed: kick, claps on 2 and 4, hats."""
    for beat in kick_beats:
        add(start + beat * BEAT, 0.45, kick, 0.85 * gain, bus='drums')
        sidechain(start + beat * BEAT)
    if claps:
        for beat in (1, 3):
            add(start + beat * BEAT, 0.4, clap, 0.42 * gain, 0.05, bus='drums')
    every = 16 // hats
    for step in range(0, 16, every):
        accent = 1.0 if step % 4 == 2 else 0.55
        add(at(start, step), 0.06, hat, 0.08 * gain * accent, 0.3, bus='drums')
    if open_hat:
        for beat in (0.5, 1.5, 2.5, 3.5):
            add(start + beat * BEAT, 0.18, lambda t: hat(t) * math.exp(-12 * t) / math.exp(-80 * t) * 0.6, 0.05 * gain, -0.3, bus='drums')


def snare_fill(start, beats=1.0, gain=1.0):
    hits = int(beats * 4)
    for i in range(hits):
        add(start + i * STEP, 0.3, clap, gain * (0.12 + 0.3 * i / hits), -0.3 + 0.6 * i / hits, bus='drums')


# --- tonal voices ----------------------------------------------------------------

def saw(freq, t, detune=0.0):
    x = (freq * (1 + detune) * t) % 1.0
    return 2 * x - 1


def brass(freqs, length, bright=1.0):
    """A brass stab: detuned saws through a low-pass that opens on the attack."""
    s = [0.0, 0.0]

    def voice(t):
        env = min(1.0, t / 0.012) * (0.55 + 0.45 * math.exp(-6 * t)) * min(1.0, max(0.0, (length - t) / 0.08))
        x = 0.0
        for k, f in enumerate(freqs):
            x += saw(f, t, 0.004) + saw(f, t + k * 0.0011, -0.005)
        x /= 2 * len(freqs)
        cutoff = (900 + 4200 * math.exp(-9 * t)) * bright
        k = lp_coef(cutoff)
        s[0] += k * (x - s[0])
        s[1] += k * (s[0] - s[1])
        return env * s[1] * 1.6
    return voice


def pulse(freq, bright):
    """One sixteenth of the synth pulse: a plucked square-ish note."""
    s = [0.0]

    def voice(t):
        x = 1.0 if (freq * t) % 1.0 < 0.42 else -1.0
        s[0] += lp_coef(600 + 3000 * bright * math.exp(-25 * t)) * (x - s[0])
        return s[0] * math.exp(-14 * t) * (1 - math.exp(-2000 * t))
    return voice


def bass(freq, length):
    s = [0.0]

    def voice(t):
        env = min(1.0, t / 0.006) * min(1.0, max(0.0, (length - t) / 0.03)) * (0.7 + 0.3 * math.exp(-10 * t))
        x = saw(freq, t) * 0.6 + math.sin(TAU * freq * t)
        s[0] += lp_coef(380 + 900 * math.exp(-18 * t)) * (x - s[0])
        return env * s[0]
    return voice


def strings(freqs, length):
    s = [0.0]

    def voice(t):
        env = min(1.0, t / 0.5) * min(1.0, max(0.0, (length - t) / 0.4))
        vib = 1 + 0.003 * math.sin(TAU * 5.5 * t)
        x = sum(saw(f * vib, t, d) for f in freqs for d in (-0.003, 0.003)) / (2 * len(freqs))
        s[0] += lp_coef(1800) * (x - s[0])
        return env * s[0]
    return voice


def glock(freq):
    def voice(t):
        env = (1 - math.exp(-600 * t)) * math.exp(-3.2 * t)
        return env * (math.sin(TAU * freq * t) + 0.25 * math.sin(TAU * freq * 2.76 * t) * math.exp(-6 * t)
                      + 0.1 * math.sin(TAU * freq * 5.4 * t) * math.exp(-12 * t))
    return voice


# --- harmony -----------------------------------------------------------------------

# Cm - Ab - Eb - Bb: the bass root, and the brass and string voicing.
CHORDS = [
    (36, [55, 60, 63, 67]),  # Cm: G C Eb G
    (32, [56, 60, 63, 68]),  # Ab: Ab C Eb Ab
    (39, [55, 58, 63, 67]),  # Eb: G Bb Eb G
    (34, [53, 58, 62, 65]),  # Bb: F Bb D F
]
# The pulse's sixteenths: chord tones by index, an octave up past 3.
PULSE = [0, 2, 1, 3, 2, 1, 4, 2, 0, 2, 1, 3, 5, 3, 2, 1]
# The theme's motif on the brass: (beat, length in beats), the ident's rhythm.
MOTIF = [(0, 0.4), (0.75, 0.4), (1.5, 0.9), (2.5, 0.4), (3, 0.9)]
# From PROPERTIES on, the editor, a glockenspiel phrase a bar: (beat, note).
MELODY = [
    [(0, 79), (0.75, 75), (1.5, 72), (2.5, 75), (3, 79)],
    [(0, 80), (1, 79), (1.5, 75), (3, 72)],
    [(0, 79), (0.75, 82), (1.5, 79), (2.5, 75), (3, 74)],
    [(0, 77), (1, 74), (1.5, 70), (2, 74), (3, 77)],
]


def chord_at(bar_index):
    return CHORDS[bar_index % 4]


def tone(chord, index):
    return chord[index % 4] + 12 * (index // 4)


def bed_bar(start, bar_index, gain=1.0, bright=1.0, pulse_on=True, bass_on=True, strings_on=False):
    root, chord = chord_at(bar_index)
    if bass_on:
        for e in range(8):
            note = root + (12 if e % 4 == 3 else 0)
            add(start + e * BEAT / 2, BEAT / 2 * 0.9, bass(hz(note), BEAT / 2 * 0.9), 0.30 * gain)
    if pulse_on:
        for step, index in enumerate(PULSE):
            accent = 1.0 if step % 4 == 0 else 0.7
            add(at(start, step), STEP * 1.8, pulse(hz(tone(chord, index) + 12), bright), 0.07 * gain * accent,
                -0.35 if step % 2 else 0.35)
    if strings_on:
        add(start, BAR, strings([hz(n) for n in chord] + [hz(chord[0] + 12)], BAR), 0.16 * gain)


def stab(start, chord, length=0.4 * BEAT, gain=1.0, octave=0):
    add(start, length + 0.1, brass([hz(n + octave) for n in chord] + [hz(chord[0] - 12 + octave)], length + 0.1), 0.30 * gain)


def motif(start, bar_index, gain=1.0):
    _, chord = chord_at(bar_index)
    for beat, length in MOTIF:
        stab(start + beat * BEAT, chord, length * BEAT, gain)


def impact(start, gain=1.0):
    add(start, 3.0, boom, 0.9 * gain)
    add(start, 3.0, crash, 0.22 * gain, 0.2, bus='drums')
    add(start, 2.5, timpani(hz(36)), 0.7 * gain, bus='drums')
    sidechain(start, 0.5, 0.4)


# --- open ------------------------------------------------------------------------------

# Bar 0: the multiviewer lights, a ping for each of its fourteen monitors.
add(0, 2 * BAR, drone(2 * BAR), 0.10)
PINGS = [72, 75, 79, 84, 79, 82, 86, 91, 84, 87, 91, 96, 91, 94]
for i, note in enumerate(PINGS):
    add(at(0, i), 0.4, ping(hz(note)), 0.10, -0.6 + 1.2 * i / len(PINGS))
# Bar 1: the countdown.
for beat in (1, 2, 3):
    add(BAR + beat * BEAT, 0.14, beep, 0.11)
roll(BAR, BAR, hz(31), 0.5)
add(BAR, BAR, riser(BAR), 0.10, bus='drums')
add(BAR, BAR, strings([hz(n) for n in CHORDS[3][1]], BAR), 0.10)
# Bars 2-3: TAKE, and the theme.
impact(2 * BAR)
for b in (2, 3):
    start = b * BAR
    groove(start, gain=0.9, open_hat=b == 3)
    bed_bar(start, b - 2, bright=0.8)
    motif(start, b - 2, 1.1)
snare_fill(3 * BAR + 3 * BEAT, 1.0, 1.1)
add(4 * BAR - 0.5, 0.5, whoosh(0.5), 0.14, bus='drums')

# --- chapters ----------------------------------------------------------------------------

for chapter in range(CHAPTERS):
    base = CHAPTER_START + chapter * CHAPTER_BARS * BAR
    calm = chapter in BREAK
    for b in range(CHAPTER_BARS):
        start = base + b * BAR
        bar_index = chapter * CHAPTER_BARS + b
        if calm:
            groove(start, gain=0.6, kick_beats=(0,), claps=False, hats=8, open_hat=False)
            bed_bar(start, bar_index, gain=0.8, bright=0.3, bass_on=True, strings_on=True)
        else:
            groove(start, open_hat=chapter >= LIFT)
            bed_bar(start, bar_index, bright=1.0 if chapter >= LIFT else 0.75)
        if chapter >= LIFT:
            for beat, note in MELODY[bar_index % 4]:
                add(start + beat * BEAT, 1.6, glock(hz(note)), 0.06, 0.25)
    # The cut into this chapter: a whoosh up to it, a stab on it.
    _, chord = chord_at(chapter * CHAPTER_BARS)
    if chapter > 0:
        add(base - 0.4, 0.4, whoosh(0.4), 0.12, -0.3 if chapter % 2 else 0.3, bus='drums')
    stab(base, chord, 0.5 * BEAT, 0.7 if calm else 1.0)
    add(base, 1.2, timpani(hz(36 if chapter % 2 == 0 else 31)), 0.35 if calm else 0.5, bus='drums')
    if chapter in (5, 6, 11):
        snare_fill(base + CHAPTER_BARS * BAR - BEAT, 1.0)

# --- MORE: a card every two beats ---------------------------------------------------------

for b in range(MORE_BARS):
    start = MORE_START + b * BAR
    groove(start)
    bed_bar(start, b, bright=1.0)
    _, chord = chord_at(b)
    for half in range(2):
        stab(start + half * 2 * BEAT, chord, 0.35 * BEAT, 0.9, 12 if half else 0)
add(MORE_START - 0.4, 0.4, whoosh(0.4), 0.12, bus='drums')
snare_fill(MORE_START + (MORE_BARS - 1) * BAR, 4.0, 1.0)
add(MORE_START + (MORE_BARS - 1) * BAR, BAR, riser(BAR), 0.12, bus='drums')

# --- outro ---------------------------------------------------------------------------------

impact(OUTRO_START)
for b in range(2):
    start = OUTRO_START + b * BAR
    groove(start, gain=0.9 - 0.3 * b, claps=b == 0, open_hat=False)
    bed_bar(start, b, bright=0.9)
    motif(start, b, 1.0)
end = OUTRO_START + 2 * BAR
final = [48, 55, 60, 62, 63, 67]  # Cm(add9)
add(end, 2 * BAR, strings([hz(n) for n in final], 2 * BAR), 0.22)
add(end, 2 * BAR, brass([hz(n) for n in final[1:]], 1.2 * BAR, 0.6), 0.18)
add(end, 2 * BAR, bass(hz(36), 2 * BAR), 0.28)
impact(end, 0.8)
roll(end + BAR, BAR * 0.75, hz(36), 0.35)
for i, note in enumerate([79, 75, 72, 67, 72, 75, 79, 84]):
    add(end + i * BEAT * 0.5, 2.5, glock(hz(note)), 0.05, -0.4 + i * 0.1)

# --- master: pump, sum, a little air off the top, soft clip, normalize ---------------------

left = array('d', (d + m * g for d, m, g in zip(drums_l, music_l, pump)))
right = array('d', (d + m * g for d, m, g in zip(drums_r, music_r, pump)))
for channel in (left, right):
    s = 0.0
    k = lp_coef(14000)
    for i in range(SAMPLES):
        s += k * (channel[i] - s)
        channel[i] = s
peak = max(max(abs(x) for x in left), max(abs(x) for x in right))
gain = 1.3 / peak

pcm = array('h')
for i in range(SAMPLES):
    for channel in (left, right):
        pcm.append(round(math.tanh(channel[i] * gain) * 0.89 * 32767))

out = Path(__file__).with_name('score-0.9.0.wav')
with wave.open(str(out), 'wb') as f:
    f.setnchannels(2)
    f.setsampwidth(2)
    f.setframerate(RATE)
    f.writeframes(pcm.tobytes())
print(f'wrote {out} ({SECONDS:.1f} s)')
