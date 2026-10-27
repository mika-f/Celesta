import { LOWER_THIRD_STRINGS } from './demos/LowerThirdDemo';
import { EMPHASIS_TEXT } from './demos/EmphasisDemo';
import { DIALOGUE_PAGES } from './voices';

// Japanese the film draws, in one place, so fonts.tsx can subset Noto Sans
// JP to exactly these characters. The voice lines are read in prepare(),
// after the fonts are declared, so their transcripts are written here too;
// reel.tsx checks them against the voice folder.

/** The transcripts and the kana of examples/assets/voice-folder/shizuku. */
export const VOICE_TEXTS = [
  'こんにちは。ボイスフォルダーを読み込んでみます。',
  '音声と文字を、同じ名前で保存してください。',
  '保存し直すと、プレビューが自動で更新されます。',
  'おんせいともじを、おなじなまえでほぞんしてください。',
];

export const DEMO_STRINGS = [
  ...VOICE_TEXTS, ...LOWER_THIRD_STRINGS, EMPHASIS_TEXT, ...DIALOGUE_PAGES,
  // Code in the chapters' diffs.
  'しずく', 'ナレーター', 'おんせいと…',
];
