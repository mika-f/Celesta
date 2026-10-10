import { createElement } from 'react';
import type { ReactNode } from 'react';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Span } from '@celesta/react';
import { planDialogue } from '@celesta/character';
import type { DialogueLine, DialoguePlan, LipSyncTrack, MouthShape, PlannedDialogueLine } from '@celesta/character';
import { lipSyncFromVoicevox, voicevoxVowelShape } from '@celesta/voicevox';
import type { VoicevoxAudioQuery } from '@celesta/voicevox';
import { CAST } from './character';
import { FPS } from './constants';
import { SCRIPT } from './script';
import type { Face, SceneId, ScriptLine, SpeakerId } from './script';

// The voiced script, timed from the WAVs make-voices.ts wrote, with each
// line's lip sync, mora timeline and face changes built from the AudioQuery
// next to it.

export type Mora = { text: string; from: number; to: number; mouth: MouthShape };
export type VoicedLine = DialogueLine & {
  id: string;
  scene: SceneId;
  speaker: SpeakerId;
  script: ScriptLine;
  lipSync: LipSyncTrack;
  moras: Mora[];
  // The speaker's face from `from` seconds into the voice on.
  faces: { from: number; face: Face }[];
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
    const { moras, pauses } = timelineOf(query);
    // One face per clause; the next clause's face starts with the pause before it.
    const faces = [line.expression ?? 'normal'].flat();
    if (faces.length > pauses.length + 1) {
      throw new Error(`script line "${line.id}" has ${faces.length} faces but its voice has only ${pauses.length + 1} clauses`);
    }
    return {
      id: line.id,
      scene: line.scene,
      speaker: line.speaker,
      script: line,
      text: emphasize(line.text, CAST[line.speaker].accent),
      audio: `./voices/${line.id}.wav`,
      // No `expression` here: Stage sets the face on the view (faceAt), so
      // it can change within a line.
      gap: line.gap,
      lipSync: lipSyncFromVoicevox(query),
      moras,
      faces: faces.map((face, i) => ({ from: i === 0 ? 0 : pauses[i - 1], face })),
    };
  }));
  // Lines follow each other tightly; a new scene waits a beat for its panel.
  plan = await planDialogue(lines, { fps: FPS, gap: 0.18, sceneLeadIn: 0.3 });
}

// Where each mora and each pause starts in the WAV, in seconds. Mirrors
// lipSyncFromVoicevox: the lengths are divided by speedScale and rounded to
// the engine's 93.75 Hz grid.
function timelineOf(query: VoicevoxAudioQuery): { moras: Mora[]; pauses: number[] } {
  const speed = query.speedScale ?? 1;
  const grid = 24000 / 256;
  const seconds = (length: number | null | undefined) => Math.round(((length ?? 0) / speed) * grid) / grid;
  const moras: Mora[] = [];
  const pauses: number[] = [];
  let t = seconds(query.prePhonemeLength);
  for (const phrase of query.accent_phrases) {
    for (const mora of phrase.moras) {
      const from = t;
      t += seconds(mora.consonant_length) + seconds(mora.vowel_length);
      moras.push({ text: mora.text ?? '', from, to: t, mouth: voicevoxVowelShape(mora.vowel) });
    }
    if (phrase.pause_mora) {
      pauses.push(t);
      t += seconds((query.pauseLength ?? phrase.pause_mora.vowel_length) * (query.pauseLengthScale ?? 1));
    }
  }
  return { moras, pauses };
}

// The face a line's speaker shows at an absolute frame (the first face
// before the voice starts, the last one through the gap after it).
export function faceAt(planned: PlannedDialogueLine<VoicedLine>, frame: number): Face {
  const seconds = (frame - planned.from) / FPS;
  let face = planned.line.faces[0].face;
  for (const cue of planned.line.faces) {
    if (cue.from <= seconds) face = cue.face;
  }
  return face;
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
