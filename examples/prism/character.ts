import { createRef } from 'react';
import { loadLipSync, loadPsdPreset } from '@celesta/react';
import type { AssetReference, CharacterViewReference, LipSyncTrack } from '@celesta/react';

// The talking portrait: asset paths, shared refs, and the data loaded in prepare().

export const PSD = '../assets/illust/琴葉姉妹_SD立ち絵.psd';
export const PRESET = '../assets/illust/琴葉茜.pfv';
export const VOICE = '../assets/voices/character-lipsync-demo.wav';
export const MOUTH = '琴葉姉妹/!表情/口';
export const akane = createRef<AssetReference>();
export const akaneView = createRef<CharacterViewReference>();
export let poseLayers: string[] = [];
export let lipSync: LipSyncTrack;

export async function prepare() {
  poseLayers = await loadPsdPreset({ src: PRESET });
  lipSync = await loadLipSync({ src: VOICE, text: 'こんにちは、リップシンクのデモです！' });
}
