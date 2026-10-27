import { Group, Rect, Text } from '@celesta/react';
import { Polyline } from '@celesta/shapes';
import { C, FONT, solid } from '../../components/theme';
import { BED, duckAt, toDb } from '../mix';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { scoreAt } from '../score';
import { CHAPTER_LENGTH, chapterAt } from '../timeline';
import { voiceLevel, voices } from '../voices';
import { Bars, Lane, mono } from './common';

// DUCKING: voice-10 plays, and the score really dips under it: the bed is
// <Audio duck={{ under: 'voice', to: { db: -12 } }}>, the voice is in the
// group "voice". The curve drawn over the bed is duckEnvelope() of the
// three voice lines, the same function the preview and the export follow;
// the readout is its gain on this frame.

const CHAPTER = 6;
const LANE = 640;
const COUNT = 72;

export function DuckingDemo({ frame }: DemoProps) {
  const at = chapterAt(CHAPTER);
  const voice = voices().ducking;
  const sample = (i: number) => at + ((i + 0.5) / COUNT) * CHAPTER_LENGTH;
  const shown = (i: number) => sample(i) - at <= frame;
  const voiceBars = Array.from({ length: COUNT }, (_, i) => (shown(i) ? Math.min(1, voiceLevel(voice, sample(i)) * 4) : 0));
  const bedBars = Array.from({ length: COUNT }, (_, i) => (shown(i) ? Math.min(1, scoreAt(sample(i)).rms * 3) * duckAt(sample(i)) : 0));
  const curve = Array.from({ length: COUNT + 1 }, (_, i): [number, number] =>
    [(i / COUNT) * LANE, 150 - duckAt(at + (i / COUNT) * CHAPTER_LENGTH) * 150]);
  const gain = duckAt(at + frame);
  const db = toDb(gain);
  const ducked = gain < 0.98;
  const x = (f: number) => ((f - at) / CHAPTER_LENGTH) * LANE;
  const start = x(voice.from);
  const stop = x(voice.from + voice.durationInFrames);
  return (
    <Group>
      <Group x={40} y={52}>
        <Lane x={0} y={0} width={LANE} label={'voice-10.wav   duckGroup="voice"'} />
        <Bars x={0} y={8} width={LANE} height={110} values={voiceBars} color={C.blue} />
        <Rect x={start} y={-2} width={Math.max(0, stop - start)} height={4} fill="blue" />
      </Group>
      <Group x={40} y={250}>
        <Lane x={0} y={0} width={LANE} label={"score-0.9.0.wav   duck={{ under: 'voice', to: { db: -12 } }}"} />
        <Bars x={0} y={10} width={LANE} height={150} values={bedBars} color={C.orange} />
        <Group y={10}>
          <Polyline points={curve} stroke={C.paper} strokeWidth={3} join="round" />
        </Group>
        <Text x={start} y={178} style={mono(15, C.grey)}>attack 0.15 s</Text>
        <Text x={Math.min(stop + 18, LANE)} y={178} anchorX={1} style={mono(15, C.grey)}>release 0.4 s</Text>
        <Rect x={(frame / CHAPTER_LENGTH) * LANE - 1} y={-210} width={2} height={380} fill="paper" />
      </Group>
      <Group x={730} y={70}>
        <Text style={mono(17, C.grey)}>bed gain</Text>
        <Text y={34} style={{ fontFamily: FONT.mono, fontSize: 60, fontWeight: 700, fill: solid(ducked ? C.orange : C.paper) }}>
          {`${db <= -0.05 ? '' : '+'}${db.toFixed(1)}`}
        </Text>
        <Text y={108} style={mono(20, C.grey)}>{`dB   ×${gain.toFixed(2)}`}</Text>
        <Rect y={160} width={40} height={300} fill="raised" stroke="line" strokeWidth={2} />
        <Rect x={4} y={164 + 292 * (1 - gain)} width={32} height={292 * gain} fill={ducked ? 'orange' : 'paper'} />
        <Rect x={0} y={160 + 300 - 300 * (10 ** (-12 / 20))} width={60} height={2} fill="grey" />
        <Text x={68} y={160 + 300 - 300 * (10 ** (-12 / 20))} anchorY={0.5} style={mono(16, C.grey)}>-12 dB</Text>
        <Text x={68} y={160} anchorY={0.5} style={mono(16, C.grey)}>0 dB</Text>
        <Text y={490} style={mono(15, C.dim)}>{`under '${BED.duck.under}'`}</Text>
      </Group>
      <Rect x={STAGE.width - 2} width={2} height={STAGE.height} fill="panel" />
    </Group>
  );
}
