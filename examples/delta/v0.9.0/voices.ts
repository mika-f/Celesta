import { importVoiceFolder, loadDialogueLipSync, matchVoiceLevels, planDialogue } from '@celesta/character';
import type { DialogueLine, DialoguePlan, VoiceLine } from '@celesta/character';
import { loadAudioSpectrum } from '@celesta/media-utils';
import type { AudioSpectrum } from '@celesta/media-utils';
import { FPS } from '../components/theme';
import { VOICE_CHAPTERS, VOICE_DELAY, chapterAt } from './timeline';

// The three lines a voice speaks in the film, read in prepare() from the
// repository's voice folder fixture (examples/assets/voice-folder): CAPTIONS
// lights voice-1 word by word, DIALOGUE plays voice-2 as a dialogue line with
// two subtitle pages, and DUCKING plays voice-10 over the bed. Every one is
// in the duck group "voice", and the score ducks under all three.
const FOLDER = '../assets/voice-folder/shizuku';
/** The speech level matchVoiceLevels() brings the lines to: well over the ducked bed. */
export const VOICE_DBFS = -15;
const PORTRAITS = '../assets/dialogue-demo/portraits';

export const SHIZUKU = {
  name: 'しずく',
  normal: `${PORTRAITS}/shizuku-normal.png`,
  smile: `${PORTRAITS}/shizuku-smile.png`,
  mouth: (shape: string) => `${PORTRAITS}/mouth-${shape}.png`,
};

export type VoiceRole = keyof typeof VOICE_CHAPTERS;
const FILES: Record<VoiceRole, string> = { captions: 'hello', dialogue: 'voice-2', ducking: 'voice-10' };

export interface Voice {
  role: VoiceRole;
  line: VoiceLine;
  /** The composition frame the voice starts on. */
  from: number;
  durationInFrames: number;
  /** The voice's own level, hop by hop, for the waveforms. */
  spectrum: AudioSpectrum;
}

/** DIALOGUE's line: the subtitle in two pages, "同じ名前" emphasized. */
export const DIALOGUE_PAGES = ['音声と文字を、', '同じ名前で保存してください。'];
export const DIALOGUE_EMPHASIS = [{ start: 7, end: 11, style: 'key' }];

let loaded: { voices: Record<VoiceRole, Voice>; plan: DialoguePlan } | undefined;

export async function loadVoices() {
  const read = await importVoiceFolder({ directory: FOLDER, manifest: `${FOLDER}/manifest.json`, speaker: 'shizuku', lipSync: false });
  // The recordings speak at about -24 dBFS; each line's volume takes it to VOICE_DBFS.
  const { lines } = await matchVoiceLevels(read, { targetRmsDbfs: VOICE_DBFS });
  const entries = await Promise.all((Object.keys(FILES) as VoiceRole[]).map(async (role) => {
    const line = lines.find((l) => l.id === FILES[role]);
    if (!line) throw new Error(`the voice folder has no ${FILES[role]}`);
    const spectrum = await loadAudioSpectrum(line.audio, { bands: 8, smoothing: { attack: 0.005, release: 0.08 } });
    const voice: Voice = {
      role, line, spectrum,
      from: chapterAt(VOICE_CHAPTERS[role]) + VOICE_DELAY,
      durationInFrames: Math.ceil(line.durationInSeconds * FPS),
    };
    return [role, voice] as const;
  }));
  const voices = Object.fromEntries(entries) as Record<VoiceRole, Voice>;
  // DIALOGUE plays its line through <DialogueSeries>: lip sync from the WAV
  // and the kana spokenText, the subtitle a page at a time.
  const spoken = voices.dialogue.line;
  const dialogue: DialogueLine = {
    id: 'dialogue', speaker: 'shizuku', audio: spoken.audio, volume: spoken.volume,
    text: spoken.text, spokenText: spoken.spokenText,
    subtitlePages: [
      { from: 0, text: DIALOGUE_PAGES[0] },
      { from: Math.round(1.45 * FPS), text: DIALOGUE_PAGES[1] },
    ],
    emphasis: DIALOGUE_EMPHASIS,
  };
  const plan = await planDialogue(await loadDialogueLipSync([dialogue]), { fps: FPS });
  loaded = { voices, plan };
}

export function voices() {
  if (!loaded) throw new Error('the voice folder is not loaded: call loadVoices() in prepare()');
  return loaded.voices;
}

export function dialoguePlan() {
  if (!loaded) throw new Error('the voice folder is not loaded: call loadVoices() in prepare()');
  return loaded.plan;
}

/** The voice's level at a composition frame, 0 outside it. */
export function voiceLevel(voice: Voice, frame: number) {
  const hop = Math.floor((frame - voice.from) / FPS / voice.spectrum.hopSeconds);
  return hop >= 0 && hop < voice.spectrum.rms.length ? voice.spectrum.rms[hop] : 0;
}
