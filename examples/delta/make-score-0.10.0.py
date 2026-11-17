"""Original 120 BPM modular techno for DELTA 0.10.0. Python standard library; no samples.

Bars follow film-0.10.0.tsx: one bar = 2 s = 60 frames, one beat = 15 frames.
  bar 0      power: a hum that swells, and the core's power-on thump and blip
  bar 1      the fifteen modules seat in the rack, a clack a sixteenth, rising
  bar 2      the title: an impact; the cables go in, a plug click a sixteenth
             (4 frames after the grid, as the film draws them), over the groove
  bar 3      into the first module: a riser and a snare build, a whoosh to the
             downbeat
  bars 4-11  CONTENT: four-on-the-floor, claps on 2 and 4, sixteenth hats, an
             offbeat bass in F minor (Fm - Db - Ab - Eb), the sequencer's
             sixteenth blips; every module is patched in on its downbeat (a
             plug click on frame 8, a stab), and patched out near its end
  bars 12-17 MEDIA: an acid line joins, its filter opening chapter by chapter
  bars 18-19 REGISTRY: the breakdown; the kick on 1, a pad, the blips filtered
  bars 20-25 CELESTA PLUGIN to FIND PLUGINS: everything back, the acid line high
  bars 26-31 EDITOR: a bell lead over the groove
  bars 32-37 MORE: a stab every two beats, a card on each; a build
  bars 38-41 outro: an impact on the end card, the groove thinning as the
             camera pulls back, the pad held, and on the last bar a falling
             whine and a click for each module powering down, the core last
Writes score-0.10.0.wav next to this script (48 kHz, 16-bit stereo, 84 s).
"""
from array import array
import math
from pathlib import Path
import random
import wave

RATE = 48000
BPM = 120
FPS = 30
BEAT = 60 / BPM
BAR = 4 * BEAT
STEP = BEAT / 4
OPEN_BARS = 4
CHAPTERS = 14
CHAPTER_BARS = 2
MORE_BARS = 6
OUTRO_BARS = 4
TOTAL_BARS = OPEN_BARS + CHAPTERS * CHAPTER_BARS + MORE_BARS + OUTRO_BARS
SECONDS = TOTAL_BARS * BAR
SAMPLES = round(SECONDS * RATE)
TAU = math.tau
rng = random.Random(1010)

drums_l = array('d', [0]) * SAMPLES
drums_r = array('d', [0]) * SAMPLES
music_l = array('d', [0]) * SAMPLES
music_r = array('d', [0]) * SAMPLES
pump = array('d', [1]) * SAMPLES

CHAPTER_START = OPEN_BARS * BAR
MORE_START = CHAPTER_START + CHAPTERS * CHAPTER_BARS * BAR
OUTRO_START = MORE_START + MORE_BARS * BAR
MEDIA = range(4, 7)
BREAK = 7         # REGISTRY
EDITOR = 11       # INSPECTOR on
PLUG_IN = 8       # frames into a chapter (program.tsx PLUG_IN)
PLUG_OUT = 2 * 60 - 22


def frames(n):
    return n / FPS


def step_frames(n):
    """timeline.ts step(): a sixteenth in frames, rounded."""
    return round(n * 15 / 4)


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


def sidechain(start, depth=0.4, release=0.22):
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


# --- drums ------------------------------------------------------------------------------

def kick(t):
    phase = TAU * (46 * t + 120 * (1 - math.exp(-36 * t)) / 36)
    click = math.exp(-500 * t) * 0.35
    return math.tanh(2.4 * math.sin(phase) * math.exp(-7.5 * t)) * (1 - math.exp(-2000 * t)) + click


def clap(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - 0.6 * state[0], x
    bursts = sum(math.exp(-180 * (t - d)) for d in (0, 0.01, 0.02) if t >= d)
    return x * (0.5 * bursts + 0.55 * math.exp(-14 * t)) * (1 - math.exp(-3000 * t))


def hat(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - state[0], x
    return x * math.exp(-90 * t)


def open_hat(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - state[0], x
    return x * math.exp(-14 * t)


def crash(t, state=[0.0]):
    x = rng.uniform(-1, 1)
    x, state[0] = x - 0.9 * state[0], x
    shimmer = 0.3 * math.sin(TAU * 5400 * t) * math.sin(TAU * 7300 * t)
    return (x + shimmer) * (0.7 * math.exp(-1.6 * t) + 0.3 * math.exp(-14 * t)) * (1 - math.exp(-800 * t))


def boom(t):
    phase = TAU * (30 * t + 46 * (1 - math.exp(-8 * t)) / 8)
    return math.sin(phase) * math.exp(-1.4 * t) * (1 - math.exp(-500 * t))


def rim(t):
    return (math.sin(TAU * 1650 * t) * 0.6 + math.sin(TAU * 540 * t)) * math.exp(-60 * t) * (1 - math.exp(-4000 * t))


def groove(start, gain=1.0, kicks=(0, 1, 2, 3), claps=True, hats=True, opens=True, rims=False):
    for beat in kicks:
        add(start + beat * BEAT, 0.45, kick, 0.9 * gain, bus='drums')
        sidechain(start + beat * BEAT)
    if claps:
        for beat in (1, 3):
            add(start + beat * BEAT, 0.4, clap, 0.36 * gain, 0.05, bus='drums')
    if hats:
        for step in range(16):
            accent = 1.0 if step % 4 == 2 else 0.5
            add(start + step * STEP, 0.06, hat, 0.075 * gain * accent, 0.3, bus='drums')
    if opens:
        for beat in (0.5, 1.5, 2.5, 3.5):
            add(start + beat * BEAT, 0.22, open_hat, 0.05 * gain, -0.3, bus='drums')
    if rims:
        for step in (3, 6, 11, 14):
            add(start + step * STEP, 0.1, rim, 0.06 * gain, -0.5, bus='drums')


def snare_build(start, beats, gain=1.0):
    hits = int(beats * 4)
    for i in range(hits):
        add(start + i * STEP, 0.25, clap, gain * (0.08 + 0.3 * (i / hits) ** 2), -0.3 + 0.6 * i / hits, bus='drums')


def whoosh(length):
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
        s[1] += TAU * (200 + 1600 * p ** 2) / RATE
        return (s[0] * 1.5 + 0.25 * math.sin(s[1])) * p ** 2
    return voice


# --- the rack's own sounds -----------------------------------------------------------------

def plug(t, state=[0.0]):
    """A patch cable going in: a short click and the jack's ring."""
    x = rng.uniform(-1, 1)
    x, state[0] = x - 0.95 * state[0], x
    return (x * math.exp(-260 * t) * 0.8 + math.sin(TAU * 2900 * t) * math.exp(-45 * t) * 0.35) * (1 - math.exp(-6000 * t))


def clack(pitch):
    """A module seating in its rail: a knock and a rattle."""
    def voice(t):
        knock = math.sin(TAU * pitch * t * (1 + 0.5 * math.exp(-60 * t))) * math.exp(-38 * t)
        rattle = rng.uniform(-1, 1) * math.exp(-120 * t) * 0.4
        return (knock + rattle) * (1 - math.exp(-4000 * t))
    return voice


def hum(length):
    def voice(t):
        env = min(1.0, (t / length) ** 1.5)
        f = hz(29)
        return env * (math.sin(TAU * f * t) + 0.35 * math.sin(TAU * 2 * f * t) + 0.12 * math.sin(TAU * 3 * f * t * 1.002))
    return voice


def blip(freq, length=0.12, bright=1.0):
    """The sequencer: a sine pluck with a little fold."""
    def voice(t):
        env = (1 - math.exp(-2500 * t)) * math.exp(-t / (length * 0.3))
        x = math.sin(TAU * freq * t)
        return env * math.sin(x * (1 + 1.6 * bright * math.exp(-30 * t)))
    return voice


def power_down(length):
    s = [0.0]

    def voice(t):
        p = t / length
        s[0] += TAU * (hz(60) * (1 - 0.85 * p) ** 2) / RATE
        return math.sin(s[0]) * (1 - p) ** 1.5 * min(1.0, t / 0.05)
    return voice


# --- tonal voices ----------------------------------------------------------------------------

def saw(freq, t, detune=0.0):
    x = (freq * (1 + detune) * t) % 1.0
    return 2 * x - 1


def acid(freq, length, cutoff, resonance=0.82, accent=False):
    """A saw through a resonant state-variable low-pass with its own envelope."""
    st = [0.0, 0.0]

    def voice(t):
        env = min(1.0, t / 0.003) * min(1.0, max(0.0, (length - t) / 0.01))
        x = saw(freq, t)
        fc = cutoff * (1 + (3.0 if accent else 1.6) * math.exp(-t / 0.06))
        f = 2 * math.sin(math.pi * min(fc, 9000) / RATE)
        q = 1 - resonance
        low, band = st
        high = x - low - q * band
        band += f * high
        low += f * band
        st[0], st[1] = low, band
        return env * math.tanh(1.8 * low)
    return voice


def bass(freq, length):
    s = [0.0]

    def voice(t):
        env = min(1.0, t / 0.004) * min(1.0, max(0.0, (length - t) / 0.02)) * (0.6 + 0.4 * math.exp(-12 * t))
        x = saw(freq, t) * 0.5 + math.sin(TAU * freq * t)
        s[0] += lp_coef(300 + 1100 * math.exp(-20 * t)) * (x - s[0])
        return env * s[0]
    return voice


def pad(freqs, length, cutoff=1600):
    s = [0.0]

    def voice(t):
        env = min(1.0, t / 0.6) * min(1.0, max(0.0, (length - t) / 0.5))
        vib = 1 + 0.003 * math.sin(TAU * 4.8 * t)
        x = sum(saw(f * vib, t, d) for f in freqs for d in (-0.004, 0.004)) / (2 * len(freqs))
        s[0] += lp_coef(cutoff) * (x - s[0])
        return env * s[0]
    return voice


def stab(freqs, length):
    s = [0.0, 0.0]

    def voice(t):
        env = min(1.0, t / 0.008) * math.exp(-5 * t) * min(1.0, max(0.0, (length - t) / 0.05))
        x = sum(saw(f, t, 0.003) + saw(f, t, -0.004) for f in freqs) / (2 * len(freqs))
        k = lp_coef(800 + 5000 * math.exp(-10 * t))
        s[0] += k * (x - s[0])
        s[1] += k * (s[0] - s[1])
        return env * s[1] * 1.6
    return voice


def bell(freq):
    def voice(t):
        env = (1 - math.exp(-800 * t)) * math.exp(-2.6 * t)
        return env * (math.sin(TAU * freq * t + 1.2 * math.sin(TAU * freq * 3.5 * t) * math.exp(-5 * t))
                      + 0.2 * math.sin(TAU * freq * 2 * t) * math.exp(-4 * t))
    return voice


# --- harmony ------------------------------------------------------------------------------------

# Fm - Db - Ab - Eb: the bass root, and the pad and stab voicing.
CHORDS = [
    (29, [56, 60, 65, 68]),  # Fm: Ab C F Ab
    (25, [56, 61, 65, 68]),  # Db: Ab Db F Ab
    (32, [56, 60, 63, 68]),  # Ab: Ab C Eb Ab
    (27, [55, 58, 63, 67]),  # Eb: G Bb Eb G
]
# The sequencer's sixteen steps: chord tones by index (an octave up past 3), None a rest.
SEQ = [0, None, 2, 1, 3, None, 1, 4, 0, 2, None, 3, 5, 3, 1, None]
# The acid line: (step, semitones over the root, slide-ish accent).
ACID = [(0, 12, True), (2, 12, False), (3, 24, False), (6, 15, True), (8, 12, False), (10, 19, False),
        (11, 12, True), (14, 22, False)]
# The editor's bell lead, a phrase a bar: (beat, note).
LEAD = [
    [(0, 80), (0.75, 77), (1.5, 72), (2.5, 77), (3, 80)],
    [(0, 81), (1, 80), (1.5, 77), (3, 73)],
    [(0, 80), (0.75, 84), (1.5, 80), (2.5, 77), (3, 75)],
    [(0, 79), (1, 75), (1.5, 72), (2, 75), (3, 79)],
]


def chord_at(bar_index):
    return CHORDS[bar_index % 4]


def tone(chord, index):
    return chord[index % 4] + 12 * (index // 4)


def bass_bar(start, bar_index, gain=1.0):
    """The offbeat techno bass: the root on every off-eighth, an octave on the last."""
    root, _ = chord_at(bar_index)
    for beat in range(4):
        note = root + 12 + (12 if beat == 3 else 0)
        add(start + (beat + 0.5) * BEAT, BEAT * 0.42, bass(hz(note), BEAT * 0.42), 0.34 * gain)


def seq_bar(start, bar_index, gain=1.0, bright=1.0):
    _, chord = chord_at(bar_index)
    for step, index in enumerate(SEQ):
        if index is None:
            continue
        accent = 1.0 if step % 4 == 0 else 0.7
        add(start + step * STEP, 0.25, blip(hz(tone(chord, index) + 12), 0.14, bright), 0.06 * gain * accent,
            -0.4 if step % 2 else 0.4)


def acid_bar(start, bar_index, cutoff, gain=1.0):
    root, _ = chord_at(bar_index)
    for step, semis, accent in ACID:
        length = STEP * (1.6 if accent else 0.8)
        add(start + step * STEP, length, acid(hz(root + semis), length, cutoff, accent=accent), 0.13 * gain * (1.2 if accent else 1.0), 0.1)


def chord_stab(start, chord, gain=1.0, octave=0, length=0.45 * BEAT):
    add(start, length + 0.05, stab([hz(n + octave) for n in chord], length + 0.05), 0.26 * gain)


def impact(start, gain=1.0):
    add(start, 3.0, boom, 0.9 * gain)
    add(start, 3.0, crash, 0.2 * gain, 0.2, bus='drums')
    sidechain(start, 0.55, 0.45)


# --- open ------------------------------------------------------------------------------------------

# Bar 0: the hum swells; the core powers on at frame 12.
add(0, BAR, hum(BAR), 0.16)
add(frames(12), 0.6, clack(70), 0.4, bus='drums')
add(frames(12), 0.4, blip(hz(84), 0.3, 0.2), 0.08)
add(frames(12), BAR * 1.2, pad([hz(n) for n in CHORDS[0][1]], BAR * 1.2, 900), 0.08)
# Bar 1: the modules seat, one a sixteenth from frame 60.
for i in range(15):
    add(frames(60 + step_frames(i)), 0.3, clack(110 + i * 9), 0.22, -0.7 + 1.4 * i / 14, bus='drums')
add(BAR, 0.45, kick, 0.7, bus='drums')
add(BAR, BAR, pad([hz(n) for n in CHORDS[3][1]], BAR, 1200), 0.08)
# Bar 2: the title, and the cables in, from frame 124.
impact(2 * BAR)
groove(2 * BAR, gain=0.85, opens=False)
bass_bar(2 * BAR, 0)
chord_stab(2 * BAR, CHORDS[0][1], 1.1)
for s in range(15):
    add(frames(120 + 4 + step_frames(s)), 0.2, plug, 0.18, -0.7 + 1.4 * s / 14, bus='drums')
    add(frames(120 + 4 + step_frames(s)), 0.25, blip(hz(tone(CHORDS[0][1], s % 6) + 12), 0.12), 0.05, 0.3 - 0.6 * (s % 2))
# Bar 3: the dive into the first module.
groove(3 * BAR, gain=0.9)
bass_bar(3 * BAR, 1)
seq_bar(3 * BAR, 1, 0.7, 0.6)
add(3 * BAR, BAR, riser(BAR), 0.11, bus='drums')
snare_build(3 * BAR + 2 * BEAT, 2.0, 1.0)
add(4 * BAR - 0.4, 0.4, whoosh(0.4), 0.13, bus='drums')

# --- chapters ----------------------------------------------------------------------------------------

for chapter in range(CHAPTERS):
    base = CHAPTER_START + chapter * CHAPTER_BARS * BAR
    calm = chapter == BREAK
    for b in range(CHAPTER_BARS):
        start = base + b * BAR
        bar_index = chapter * CHAPTER_BARS + b
        if calm:
            groove(start, gain=0.7, kicks=(0,), claps=False, opens=False)
            add(start, BAR, pad([hz(n) for n in chord_at(bar_index)[1]] + [hz(chord_at(bar_index)[1][0] + 12)], BAR, 1800), 0.3)
            add(start, BAR, bass(hz(chord_at(bar_index)[0] + 12), BAR), 0.22)
            seq_bar(start, bar_index, 1.2, 0.3)
        else:
            groove(start, rims=chapter >= 4)
            bass_bar(start, bar_index)
            seq_bar(start, bar_index, 1.0, 1.0 if chapter >= EDITOR else 0.7)
        if chapter in MEDIA or (chapter > BREAK and chapter < EDITOR):
            cutoff = 500 + 450 * (chapter - 4) if chapter in MEDIA else 1500 + 300 * b
            acid_bar(start, bar_index, cutoff, 0.9 if chapter in MEDIA else 1.0)
        if chapter >= EDITOR:
            for beat, note in LEAD[bar_index % 4]:
                add(start + beat * BEAT, 1.4, bell(hz(note)), 0.05, 0.25)
    # The cut into this chapter: a whoosh up to it, a stab on it; the cables in and out.
    _, chord = chord_at(chapter * CHAPTER_BARS)
    if chapter > 0:
        add(base - 0.35, 0.35, whoosh(0.35), 0.11, -0.3 if chapter % 2 else 0.3, bus='drums')
    chord_stab(base, chord, 0.7 if calm else 1.0)
    add(base + frames(PLUG_IN), 0.2, plug, 0.26, -0.3, bus='drums')
    add(base + frames(PLUG_OUT), 0.2, plug, 0.2, 0.4, bus='drums')
    if chapter in (6, 10):
        snare_build(base + CHAPTER_BARS * BAR - BEAT, 1.0)

# --- MORE: a card every two beats --------------------------------------------------------------------

add(MORE_START - 0.35, 0.35, whoosh(0.35), 0.11, bus='drums')
for b in range(MORE_BARS):
    start = MORE_START + b * BAR
    groove(start, rims=True)
    bass_bar(start, b)
    seq_bar(start, b, 0.9, 1.0)
    _, chord = chord_at(b)
    for half in range(2):
        chord_stab(start + half * 2 * BEAT, chord, 0.95, 12 if half else 0, 0.35 * BEAT)
snare_build(MORE_START + (MORE_BARS - 1) * BAR, 4.0, 1.0)
add(MORE_START + (MORE_BARS - 1) * BAR, BAR, riser(BAR), 0.12, bus='drums')

# --- outro ---------------------------------------------------------------------------------------------

impact(OUTRO_START)
chord_stab(OUTRO_START, CHORDS[0][1], 1.1)
for b in range(2):
    start = OUTRO_START + b * BAR
    groove(start, gain=0.9 - 0.25 * b, claps=b == 0, opens=b == 0)
    bass_bar(start, b, 0.9 - 0.3 * b)
    seq_bar(start, b, 0.9, 0.8)
hold = OUTRO_START + 2 * BAR
final = [41, 48, 56, 60, 63, 67]  # Fm(add9)
add(hold, 2 * BAR, pad([hz(n) for n in final], 2 * BAR, 1800), 0.2)
add(hold, 2 * BAR, bass(hz(29), 2 * BAR), 0.3)
for i, note in enumerate([80, 77, 72, 68, 72, 77, 80, 84]):
    add(hold + i * BEAT * 0.5, 2.0, bell(hz(note)), 0.04, -0.4 + i * 0.1)
# The last bar: the modules power down from the end of the chain, one a sixteenth.
off = OUTRO_START + 3 * BAR
add(off, BAR, power_down(BAR), 0.16)
for slot in range(15, 0, -1):
    add(frames(OUTRO_START * FPS + 3 * 60 + step_frames(15 - slot)), 0.15, plug, 0.1, -0.7 + 1.4 * slot / 15, bus='drums')

# --- master: pump, sum, a little air off the top, soft clip, normalize ------------------------------------

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

out = Path(__file__).with_name('score-0.10.0.wav')
with wave.open(str(out), 'wb') as f:
    f.setnchannels(2)
    f.setsampwidth(2)
    f.setframerate(RATE)
    f.writeframes(pcm.tobytes())
print(f'wrote {out} ({SECONDS:.1f} s)')
