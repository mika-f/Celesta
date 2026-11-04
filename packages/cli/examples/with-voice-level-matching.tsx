// Commentary mixing: two voices recorded at very different levels are
// matched in prepare() with matchVoiceLevels(), laid out with planDialogue(),
// and BGM and game audio are each turned down while someone is actually
// speaking. Everything follows the script: edit script.json, regenerate the
// voices, and the timing, gains, and ducking all move with it.
//
// Setup (the WAVs are generated, not committed):
//
//   node packages/cli/scripts/make-voice-level-fixtures.mjs
//   node skills/celesta/scripts/inspect.mjs packages/cli/examples/with-voice-level-matching.tsx --every 30
//   target/release/celesta-exporter --react packages/cli/examples/with-voice-level-matching.tsx out.mp4
//
// Author-editable inputs: examples/assets/voice-level-demo/script.json, and
// LEVELS, BGM_DUCK, and GAME_DUCK below.
//
// The ducking follows the speech the level analysis found, not the whole
// planned lines: duckingVolume() builds each bed's volume from the plan and
// the report. It is a precomputed curve, not a realtime sidechain
// compressor, so preview and export play the same volumes. Without the
// report, `<Audio duck={{ under: 'voice' }}>` would duck under the voices'
// whole clips with no prepare() step at all.
//
// Assets: examples/assets/voice-level-demo (synthesized by the script above)
// and the portraits from examples/assets/dialogue-demo.
import * as React from 'react';

import { Assets, Audio, Composition, Group, Rect, Text, useCurrentFrame } from '@celesta.studio/react';
import {
  Character,
  CharacterView,
  DialogueSeries,
  duckingVolume,
  loadDialogueLipSync,
  matchVoiceLevels,
  planDialogue,
  speechIntervals,
} from '@celesta.studio/character';
import type { AnimatedNumber, AssetReference, TextStyle } from '@celesta.studio/react';
import type {
  CharacterSubtitle,
  CharacterViewReference,
  DialogueLine,
  DialoguePlan,
  VoiceLevelOptions,
  VoiceLevelReport,
} from '@celesta.studio/character';

import script from '../../../examples/assets/voice-level-demo/script.json';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const DEMO = '../../../examples/assets/voice-level-demo';
const PORTRAITS = '../../../examples/assets/dialogue-demo/portraits';
const TAIL_FRAMES = 30;

type Speaker = 'shizuku' | 'komugi';
type Line = DialogueLine & { speaker: Speaker };

/** Voice matching: every line toward -20 dBFS speech RMS, こむぎ 1 dB behind. */
const LEVELS: VoiceLevelOptions = {
  targetRmsDbfs: -20,
  peakCeilingDbfs: -1,
  maxBoostDb: 15,
  speakers: { komugi: { offsetDb: -1 } },
};

/**
 * How each background bed ducks under speech: `volume` with nobody speaking,
 * `to` times that under speech, `attack` / `release` seconds to go down and
 * back up, and silences shorter than `holdGap` seconds stay ducked.
 */
const BGM_DUCK = { volume: 0.8, to: 0.3125, attack: 0.2, release: 0.6, holdGap: 1.2 };
const GAME_DUCK = { volume: 1, to: 0.5, attack: 0.1, release: 0.3, holdGap: 0.4 };

const CAST: Record<Speaker, { name: string; color: string; x: number }> = {
  shizuku: { name: 'しずく', color: '#1f6feb', x: 200 },
  komugi: { name: 'こむぎ', color: '#d9480f', x: 810 },
};

const characters = { shizuku: React.createRef<AssetReference>(), komugi: React.createRef<AssetReference>() };
const views = { shizuku: React.createRef<CharacterViewReference>(), komugi: React.createRef<CharacterViewReference>() };

// Filled once by prepare().
let plan: DialoguePlan<Line>;
let levels: VoiceLevelReport;
let speech: Array<[number, number]>;
let bgmVolume: AnimatedNumber;
let gameVolume: AnimatedNumber;

export async function prepare(): Promise<void> {
  // The subtitle shows `text`; the mouth follows the kana `spokenText`.
  const lines = await loadDialogueLipSync(script.map((line): Line => ({
    id: line.id,
    speaker: line.speaker as Speaker,
    text: line.text,
    spokenText: line.reading,
    audio: `${DEMO}/voices/${line.id}.wav`,
    expression: line.expression,
  })));
  // Decoding and analysis happen here, once; each frame only reads volumes.
  const matched = await matchVoiceLevels(lines, LEVELS);
  levels = matched.report;
  plan = await planDialogue(matched.lines, { fps: FPS, gap: 0.4 });
  // Where a voice is actually audible, in frames for the drawing: leading
  // and trailing silence in a file and the gap after a line do not count.
  speech = speechIntervals(plan, { levels }).map(([start, end]) => [start * FPS, end * FPS]);
  // Both beds play from frame 0 at the composition root, so origin is 0.
  bgmVolume = duckingVolume(plan, BGM_DUCK, { levels });
  gameVolume = duckingVolume(plan, GAME_DUCK, { levels });
}

// --- Display ---

const INK = '#111111';
const MUTED = '#6b6b6b';
const HAIRLINE = '#d0d0d0';
const ACCENT = '#1f6feb';
const text = (size: number, color = INK, weight = 400): TextStyle => ({ fontSize: size, fontWeight: weight, fill: { type: 'solid', color } });

/** The envelope's value at a composition frame, for drawing only (linear between keys). */
function valueAt(volume: AnimatedNumber, frame: number): number {
  if (typeof volume === 'number') return volume;
  const keys = volume.keyframes.map((key) => ({ frame: (key.time.value / key.time.timescale) * FPS, value: key.value }));
  if (frame <= keys[0].frame) return keys[0].value;
  for (let i = 1; i < keys.length; i += 1) {
    if (frame <= keys[i].frame) {
      const span = keys[i].frame - keys[i - 1].frame;
      return span === 0 ? keys[i].value : keys[i - 1].value + ((keys[i].value - keys[i - 1].value) * (frame - keys[i - 1].frame)) / span;
    }
  }
  return keys[keys.length - 1].value;
}

const LANES = { x: 560, y: 470, width: 680, height: 54 };

/** BGM and game-audio volume over the whole video, with speech marked and a playhead. */
function Envelopes() {
  const frame = useCurrentFrame();
  const total = plan.durationInFrames + TAIL_FRAMES;
  const scale = LANES.width / total;
  const step = 3;
  const lane = (label: string, volume: AnimatedNumber, max: number, y: number) => (
    <Group y={y}>
      <Text x={LANES.x} y={0} anchorY={1} style={text(15, MUTED)}>{label}</Text>
      <Rect x={LANES.x} y={8} width={LANES.width} height={LANES.height} fill="#f4f4f4" />
      {speech.map(([start, end], index) => (
        <Rect key={index} x={LANES.x + start * scale} y={8} width={(end - start) * scale} height={LANES.height} fill="#e3ecfb" />
      ))}
      {Array.from({ length: Math.ceil(total / step) }, (_, index) => {
        const value = valueAt(volume, index * step) / max;
        return (
          <Rect key={index} x={LANES.x + index * step * scale} y={8 + LANES.height * (1 - value)}
            width={Math.max(1, step * scale)} height={2} fill={ACCENT} />
        );
      })}
      <Rect x={LANES.x + frame * scale} y={4} anchorX={0.5} width={2} height={LANES.height + 8} fill={INK} />
      <Text x={LANES.x + LANES.width} y={0} anchorX={1} anchorY={1} style={text(15, INK, 700)}>
        {valueAt(volume, frame).toFixed(2)}
      </Text>
    </Group>
  );
  return (
    <Group>
      {lane('BGM の音量', bgmVolume, BGM_DUCK.volume, LANES.y)}
      {lane('ゲーム音の音量', gameVolume, GAME_DUCK.volume, LANES.y + 100)}
      <Text x={LANES.x} y={LANES.y + 186} style={text(13, MUTED)}>
        青い帯: 声が実際に出ている区間。音量は duckingVolume() で計画と解析から作る
      </Text>
    </Group>
  );
}

/** Each line's source level before and after matching (not including BGM or master gain). */
function LevelTable() {
  const frame = useCurrentFrame();
  const x = 40;
  const y = 460;
  const dbX = (dbfs: number) => x + 120 + (dbfs + 40) * 5; // -40 … -10 dBFS
  return (
    <Group>
      <Text x={x} y={y} anchorY={1} style={text(15, MUTED)}>声の大きさ（素材, -40〜-10 dBFS）灰: 元　色: 調整後</Text>
      <Rect x={dbX(LEVELS.targetRmsDbfs!)} y={y + 6} width={1} height={plan.lines.length * 34} fill={INK} />
      {plan.lines.map((entry, index) => {
        const report = levels.lines[index];
        const before = report.analysis?.rmsDbfs ?? -40;
        const after = report.rmsAfterDbfs ?? before;
        const active = frame >= entry.from && frame < entry.from + entry.durationInFrames;
        const rowY = y + 12 + index * 34;
        return (
          <Group key={entry.id} opacity={active ? 1 : 0.55}>
            <Text x={x} y={rowY + 10} anchorY={0.5} style={text(15, CAST[entry.line.speaker].color, 700)}>
              {`${CAST[entry.line.speaker].name} ${entry.id}`}
            </Text>
            <Rect x={dbX(-40)} y={rowY + 2} width={dbX(before) - dbX(-40)} height={6} fill={HAIRLINE} />
            <Rect x={dbX(-40)} y={rowY + 12} width={dbX(after) - dbX(-40)} height={6} fill={CAST[entry.line.speaker].color} />
            <Text x={dbX(-10) + 8} y={rowY + 10} anchorY={0.5} style={text(13, active ? INK : MUTED)}>
              {`${report.gainDb >= 0 ? '+' : ''}${report.gainDb.toFixed(1)} dB${report.limitedBy.length ? ' (' + report.limitedBy.join(', ') + ')' : ''}`}
            </Text>
          </Group>
        );
      })}
    </Group>
  );
}

const SUBTITLE: CharacterSubtitle = {
  x: WIDTH / 2,
  y: 385,
  anchorX: 0.5,
  anchorY: 0.5,
  maxWidth: 1100,
  style: { fontSize: 34, fontWeight: 700, align: 'center', fill: { type: 'solid', color: INK } },
};

function mouths() {
  const mouth = (shape: string) => `${PORTRAITS}/mouth-${shape}.png`;
  return { a: mouth('a'), i: mouth('i'), u: mouth('u'), e: mouth('e'), o: mouth('o'), closed: mouth('closed') };
}

export default function Root() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames + TAIL_FRAMES}>
      <Assets>
        {(['shizuku', 'komugi'] as const).map((speaker) => (
          <Character
            key={speaker}
            ref={characters[speaker]}
            name={CAST[speaker].name}
            portrait={{
              defaultExpression: 'normal',
              expressions: { normal: `${PORTRAITS}/${speaker}-normal.png`, smile: `${PORTRAITS}/${speaker}-smile.png` },
              lipSync: mouths(),
            }}
            subtitle={SUBTITLE}
          />
        ))}
      </Assets>
      <Rect width={WIDTH} height={HEIGHT} fill="#ffffff" />
      <Audio src={`${DEMO}/bgm.wav`} volume={bgmVolume} />
      <Audio src={`${DEMO}/game.wav`} volume={gameVolume} />
      {(['shizuku', 'komugi'] as const).map((speaker) => (
        <Group key={speaker} x={CAST[speaker].x} y={30} scale={0.75}>
          <CharacterView ref={views[speaker]} character={characters[speaker]} mouth="closed" />
        </Group>
      ))}
      <Rect x={0} y={430} width={WIDTH} height={1} fill={HAIRLINE} />
      <DialogueSeries plan={plan} views={views} />
      <LevelTable />
      <Envelopes />
    </Composition>
  );
}
