import { createElement } from 'react';
import type { ReactNode } from 'react';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Span } from '@celesta/react';
import { planDialogue } from '@celesta/character';
import type { DialogueLine, DialoguePlan, LipSyncTrack, MouthShape, PlannedDialogueLine } from '@celesta/character';
import { lipSyncFromVoicevox, timingFromVoicevox, voicevoxVowelShape } from '@celesta/voicevox';
import type { VoicevoxAudioQuery } from '@celesta/voicevox';
import { CAST } from './character';
import { FPS } from './constants';
import { SCRIPT } from './script';
import type { Face, SceneId, ScriptLine, SpeakerId } from './script';

// The voiced script, timed from the WAVs make-voices.ts wrote, with each
// line's lip sync, mora times and face changes taken from the timing of the
// AudioQuery next to it.

export type Mora = { text: string; start: number; end: number; mouth: MouthShape };
export type VoicedLine = DialogueLine & {
  id: string;
  scene: SceneId;
  speaker: SpeakerId;
  script: ScriptLine;
  lipSync: LipSyncTrack;
  moras: Mora[];
  // The speaker's face from `at` seconds into the voice on.
  faces: { at: number; face: Face }[];
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
    const timing = timingFromVoicevox(query);
    // One face per clause; the next clause's face starts with the pause before it.
    const faces = [line.expression ?? 'normal'].flat();
    if (faces.length > timing.pauses.length + 1) {
      throw new Error(`script line "${line.id}" has ${faces.length} faces but its voice has only ${timing.pauses.length + 1} clauses`);
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
      lipSync: lipSyncFromVoicevox(timing),
      moras: timing.moras.map((mora) => ({
        text: mora.text, start: mora.start, end: mora.end, mouth: voicevoxVowelShape(mora.vowel.phoneme),
      })),
      faces: faces.map((face, i) => ({ at: i === 0 ? 0 : timing.pauses[i - 1].start, face })),
    };
  }));
  // Lines follow each other tightly; a new scene waits a beat for its panel.
  plan = await planDialogue(lines, { fps: FPS, gap: 0.18, sceneLeadIn: 0.3 });
}

// The face a line's speaker shows at an absolute frame (the first face
// before the voice starts, the last one through the gap after it).
export function faceAt(planned: PlannedDialogueLine<VoicedLine>, frame: number): Face {
  const seconds = (frame - planned.from) / FPS;
  let face = planned.line.faces[0].face;
  for (const cue of planned.line.faces) {
    if (cue.at <= seconds) face = cue.face;
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
