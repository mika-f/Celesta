import { createElement } from 'react';
import type { ReactNode } from 'react';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Span } from '@celesta/react';
import { planDialogue } from '@celesta/character';
import type { DialogueLine, DialoguePlan, LipSyncTrack, MouthShape } from '@celesta/character';
import { lipSyncFromVoicevox, voicevoxVowelShape } from '@celesta/voicevox';
import type { VoicevoxAudioQuery } from '@celesta/voicevox';
import { CAST } from './character';
import { FPS } from './constants';
import { SCRIPT } from './script';
import type { SceneId, ScriptLine, SpeakerId } from './script';

// The voiced script, timed from the WAVs make-voices.ts wrote, with each
// line's lip sync and mora timeline built from the AudioQuery next to it.

export type Mora = { text: string; from: number; to: number; mouth: MouthShape };
export type VoicedLine = DialogueLine & {
  id: string;
  scene: SceneId;
  speaker: SpeakerId;
  script: ScriptLine;
  lipSync: LipSyncTrack;
  moras: Mora[];
};

export let plan: DialoguePlan<VoicedLine>;

export async function prepareVoices() {
  const lines = await Promise.all(SCRIPT.map(async (line): Promise<VoicedLine> => {
    const path = join(import.meta.dirname, 'voices', `${line.id}.json`);
    let query: VoicevoxAudioQuery;
    try {
      query = JSON.parse(await readFile(path, 'utf8'));
    } catch {
      throw new Error(`voices/${line.id}.json is missing: run node examples/shorts-explainer/make-voices.ts first`);
    }
    return {
      id: line.id,
      scene: line.scene,
      speaker: line.speaker,
      script: line,
      text: emphasize(line.text, CAST[line.speaker].accent),
      audio: `./voices/${line.id}.wav`,
      expression: line.expression,
      gap: line.gap,
      lipSync: lipSyncFromVoicevox(query),
      moras: morasOf(query),
    };
  }));
  // Lines follow each other tightly; a new scene waits a beat for its panel.
  plan = await planDialogue(lines, { fps: FPS, gap: 0.18, sceneLeadIn: 0.3 });
}

// Where each mora sits in the WAV, in seconds. Mirrors lipSyncFromVoicevox:
// the lengths are divided by speedScale and rounded to the engine's 93.75 Hz grid.
function morasOf(query: VoicevoxAudioQuery): Mora[] {
  const speed = query.speedScale ?? 1;
  const grid = 24000 / 256;
  const seconds = (length: number | null | undefined) => Math.round(((length ?? 0) / speed) * grid) / grid;
  const moras: Mora[] = [];
  let t = seconds(query.prePhonemeLength);
  for (const phrase of query.accent_phrases) {
    for (const mora of phrase.moras) {
      const from = t;
      t += seconds(mora.consonant_length) + seconds(mora.vowel_length);
      moras.push({ text: mora.text ?? '', from, to: t, mouth: voicevoxVowelShape(mora.vowel) });
    }
    if (phrase.pause_mora) {
      t += seconds((query.pauseLength ?? phrase.pause_mora.vowel_length) * (query.pauseLengthScale ?? 1));
    }
  }
  return moras;
}

// 【word】 in the script becomes a <Span> in the speaker's color.
function emphasize(text: string, color: string): ReactNode {
  return text.split(/【(.*?)】/).map((part, i) => (
    i % 2 === 1 ? createElement(Span, { key: i, style: { fill: color } }, part) : part
  ));
}

// Frames from the start of `scene` to the start of line `id`.
export function lineFrom(scene: SceneId, id: string) {
  return plan.startOf(id) - plan.scene(scene).from;
}

// The planned line playing at an absolute frame (or the last one before it).
export function lineAt(frame: number) {
  let current = plan.lines[0];
  for (const line of plan.lines) {
    if (line.from <= frame) current = line;
  }
  return current;
}
