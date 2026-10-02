import { createRef } from 'react';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { buildEnvelope, decodeWav, loadLipSync, loadPsdPreset } from '@celesta/react';
import type { AssetReference, CharacterViewReference, LipSyncTrack } from '@celesta/react';

// The dialogue chapter's voice line: asset paths, shared refs, and the data loaded in prepare().

export const PSD = '../assets/illust/琴葉姉妹_SD立ち絵.psd';
export const PRESET = '../assets/illust/琴葉茜.pfv';
export const VOICE = '../assets/voices/character-lipsync-demo.wav';
export const VOICE_TEXT = 'こんにちは、リップシンクのデモです！';
export const VOICE_AT = 24; // local frame of the dialogue chapter
export const MOUTH = '琴葉姉妹/!表情/口';

export const akane = createRef<AssetReference>();
export const akaneView = createRef<CharacterViewReference>();
export let poseLayers: string[] = [];
export let lipSync: LipSyncTrack | null = null;
// The voice's peak envelope, one value per waveform bar.
export let voiceBars: number[] = Array.from({ length: 120 }, (_, i) => 0.2 + 0.6 * Math.abs(Math.sin(i * 0.3)));
export let voiceSeconds = 3.46;

export async function prepare() {
  poseLayers = await loadPsdPreset({ src: PRESET });
  lipSync = await loadLipSync({ src: VOICE, text: VOICE_TEXT });
  voiceSeconds = lipSync.durationInSeconds;
  try {
    const audio = decodeWav(new Uint8Array(await readFile(join(import.meta.dirname, VOICE))));
    const envelope = buildEnvelope(audio, 100);
    const peak = envelope.reduce((m, v) => Math.max(m, v), 1e-6);
    voiceBars = voiceBars.map((_, i) => {
      const a = Math.floor((i / voiceBars.length) * envelope.length);
      const b = Math.max(a + 1, Math.floor(((i + 1) / voiceBars.length) * envelope.length));
      let m = 0;
      for (let j = a; j < b; j++) m = Math.max(m, envelope[j]);
      return m / peak;
    });
  } catch {
    // Keep the placeholder waveform; the portrait still lip-syncs.
  }
}
