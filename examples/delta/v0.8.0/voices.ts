import { importVoiceFolder, matchVoiceLevels } from '@celesta.studio/character';
import type { VoiceLevelReport, VoiceLine } from '@celesta.studio/character';

// The VOICES chapter's data, read in prepare() from the repository's voice
// folder fixture (examples/assets/voice-folder): the lines importVoiceFolder()
// pairs from its .wav and .txt files, and what matchVoiceLevels() measures
// and does to them. Paths are relative to the entry file.
const FOLDER = '../assets/voice-folder/shizuku';
/** The level matchVoiceLevels() brings every line to. */
export const TARGET_DBFS = -20;

let lines: VoiceLine[] | undefined;
let report: VoiceLevelReport | undefined;

export async function loadVoices() {
  lines = await importVoiceFolder({ directory: FOLDER, manifest: `${FOLDER}/manifest.json`, speaker: 'shizuku', lipSync: false });
  report = (await matchVoiceLevels(lines, { targetRmsDbfs: TARGET_DBFS })).report;
}

export function voices() {
  if (!lines || !report) throw new Error('the voice folder is not loaded: call loadVoices() in prepare()');
  return { lines, report };
}
