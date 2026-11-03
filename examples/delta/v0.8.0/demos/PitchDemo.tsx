import { Audio, Group, Rect, Sequence, Text } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, solid } from '../../components/theme';
import { BAR, chapterAt } from '../timeline';

// What you hear in this chapter: two beats of a riff on D (pitch-0.8.0.wav,
// written by make-score-0.8.0.py), played four times through <Audio pitch>
// to follow the chords: 0, -4, +5 and +7 semitones. The last time also sets
// playbackRate={2} with preservePitch: twice as fast, still on A.
// <PitchRiff> plays it at the film's root; the demo draws what it plays.
export const PITCH_CHAPTER = 7;
export const RIFF_SRC = './pitch-0.8.0.wav';
const BARS = [
  { pitch: 0, chord: 'Dm' },
  { pitch: -4, chord: 'Bb' },
  { pitch: 5, chord: 'Gm' },
  { pitch: 7, chord: 'A', rate: 2 },
] as const;
/** The riff's sixteenths as semitones over D3, as make-score-0.8.0.py writes them. */
const RIFF = [0, 7, 12, null, 7, 0, 19, 12] as const;
const STEPS = RIFF.length;
const D3 = 50;

/** The riff under the chapter, one <Audio> a bar. */
export function PitchRiff() {
  const at = chapterAt(PITCH_CHAPTER);
  return (
    <>
      {BARS.map((bar, i) => (
        <Sequence key={i} from={at + i * BAR} durationInFrames={BAR}>
          {'rate' in bar
            ? <Audio src={RIFF_SRC} pitch={bar.pitch} playbackRate={bar.rate} preservePitch loop />
            : <Audio src={RIFF_SRC} pitch={bar.pitch} />}
        </Sequence>
      ))}
    </>
  );
}

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** The bar, and the riff step sounding, on the chapter's frame `frame`. */
function playing(frame: number) {
  const bar = Math.max(0, Math.min(3, Math.floor(frame / BAR)));
  const rate = 'rate' in BARS[bar] ? 2 : 1;
  const step = Math.floor(((Math.max(0, frame) - bar * BAR) / BAR) * STEPS * rate) % STEPS;
  const offset = RIFF[step];
  return { bar, step, note: offset === null ? null : D3 + offset + BARS[bar].pitch };
}

const LOW = 48; // C3
const WHITE = [0, 2, 4, 5, 7, 9, 11];
const OCTAVES = 3;
const KEY = STAGE.width / (7 * OCTAVES);

function Keyboard({ note }: { note: number | null }) {
  const keys = Array.from({ length: 12 * OCTAVES }, (_, i) => LOW + i);
  const white = keys.filter((k) => WHITE.includes(k % 12));
  const black = keys.filter((k) => !WHITE.includes(k % 12));
  const xOf = (k: number) => white.indexOf(k) * KEY;
  return (
    <Group>
      {white.map((k) => (
        <Rect key={k} x={xOf(k) + 1} width={KEY - 2} height={150} cornerRadius={3} fill={k === note ? C.orange : C.paper} />
      ))}
      {black.map((k) => (
        <Rect key={k} x={xOf(k - 1) + KEY * 0.68} width={KEY * 0.64} height={92} cornerRadius={2} fill={k === note ? C.orange : C.ink} />
      ))}
      <Text x={xOf(D3) + KEY / 2} y={172} anchorX={0.5} anchorY={0.5} style={mono(16, C.dim)}>D3</Text>
    </Group>
  );
}

export function PitchDemo({ frame }: DemoProps) {
  const now = playing(frame);
  const current = BARS[now.bar];
  const ratio = 2 ** (current.pitch / 12);
  const unit = 22;
  return (
    <Group>
      <Group opacity={enter(frame, 0)}><Keyboard note={frame >= 0 ? now.note : null} /></Group>
      <Group y={230} opacity={enter(frame, 4)}>
        <Text y={20} style={mono(76, C.paper, 700)}>
          {`pitch={${current.pitch > 0 ? '+' : ''}${current.pitch}}`}
        </Text>
        <Text x={STAGE.width} y={20} anchorX={1} style={mono(22, C.orange, 700)}>{current.chord}</Text>
        <Text x={STAGE.width} y={56} anchorX={1} style={mono(18, C.grey)}>{`× ${ratio.toFixed(3)} in frequency`}</Text>
        <Text x={STAGE.width} y={86} anchorX={1} opacity={'rate' in current ? 1 : 0.25}
          style={mono(18, 'rate' in current ? C.paper : C.dim)}>{'playbackRate={2} preservePitch'}</Text>
      </Group>
      {/* The four bars: the step each one transposes by, and the riff's notes in it. */}
      <Group y={590}>
        <Rect y={-1} width={STAGE.width} height={2} fill={C.line} />
        {BARS.map((bar, b) => {
          const w = STAGE.width / 4;
          const on = b === now.bar && frame >= 0;
          const rate = 'rate' in bar ? 2 : 1;
          return (
            <Group key={b} x={b * w} opacity={enter(frame, b * BAR - 6, 10) * (on ? 1 : 0.5)}>
              <Rect x={4} y={-bar.pitch * unit / 2 - 4} width={w - 8} height={8} fill={on ? C.orange : C.grey} />
              {Array.from({ length: STEPS * rate }, (_, s) => {
                const offset = RIFF[s % STEPS];
                if (offset === null) return null;
                return <Rect key={s} x={6 + (s / (STEPS * rate)) * (w - 12)} y={-bar.pitch * unit / 2 - 14 - offset * 4}
                  width={(w - 12) / (STEPS * rate) - 2} height={4} fill={on && s % STEPS === now.step && Math.floor(s / STEPS) === Math.floor(((frame - b * BAR) / BAR) * rate) ? C.paper : C.dim} />;
              })}
              <Text x={w / 2} y={128} anchorX={0.5} anchorY={0.5} style={mono(18, on ? C.paper : C.grey, 700)}>
                {`${bar.pitch > 0 ? '+' : ''}${bar.pitch}  ${bar.chord}`}
              </Text>
            </Group>
          );
        })}
      </Group>
    </Group>
  );
}
