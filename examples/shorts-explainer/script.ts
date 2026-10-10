// The script: who says what, in which scene, with which face.
//
// `text` is the subtitle. 【…】 marks a word to emphasize (drawn in the
// speaker's color). `reading` is what VOICEVOX reads when it differs from
// the subtitle (English names, words it misreads). After editing, run
// make-voices.ts again; film.tsx re-times every scene from the new voices.
//
// Plain data only: make-voices.ts imports this file with Node.js directly.

export const SPEAKERS = {
  kiritan: { name: '東北きりたん', style: 108, speed: 1.12, pitch: 0, intonation: 1.15 },
  zunko: { name: '東北ずん子', style: 107, speed: 1.12, pitch: 0, intonation: 1.15 },
} as const;
export type SpeakerId = keyof typeof SPEAKERS;

export type SceneId = 'hook' | 'react' | 'frame' | 'voice' | 'psd' | 'sync' | 'outro';

export interface ScriptLine {
  id: string;
  scene: SceneId;
  speaker: SpeakerId;
  text: string;
  reading?: string;
  // A face from character.ts; 'normal' when omitted.
  expression?: 'smile' | 'happy' | 'smug' | 'puzzled';
  speed?: number;
  gap?: number;
}

export const SCRIPT: ScriptLine[] = [
  { id: 'hook-1', scene: 'hook', speaker: 'kiritan', expression: 'smug',
    text: 'この動画、じつは【全部コード】です。', reading: 'この動画、じつは全部コードです。' },
  { id: 'hook-2', scene: 'hook', speaker: 'zunko', expression: 'puzzled',
    text: 'えっ、編集ソフトじゃないの？' },

  { id: 'react-1', scene: 'react', speaker: 'kiritan',
    text: 'Celesta は【React と TypeScript】で動画を書くツールです。',
    reading: 'セレスタは、リアクトとタイプスクリプトで動画を書くツールです。' },
  { id: 'react-2', scene: 'react', speaker: 'zunko',
    text: 'ウェブページみたいに？' },
  { id: 'react-3', scene: 'react', speaker: 'kiritan', expression: 'smile',
    text: 'はい。【四角や文字の部品】を組み合わせて作ります。' },

  { id: 'frame-1', scene: 'frame', speaker: 'zunko',
    text: 'タイムラインはどこにあるの？' },
  { id: 'frame-2', scene: 'frame', speaker: 'kiritan',
    text: '【フレーム番号】から絵を計算する関数。それだけです。' },

  { id: 'voice-1', scene: 'voice', speaker: 'zunko', expression: 'smile',
    text: 'じゃあ、わたしたちの声は？' },
  { id: 'voice-2', scene: 'voice', speaker: 'kiritan',
    text: '【VOICEVOX】の音素の長さから、口パクを作っています。',
    reading: 'ボイスボックスの音素の長さから、口パクを作っています。' },

  { id: 'psd-1', scene: 'psd', speaker: 'zunko',
    text: 'まばたきや表情も？' },
  { id: 'psd-2', scene: 'psd', speaker: 'kiritan', expression: 'smile',
    text: '【PSD のレイヤー】を、フレームごとに切り替えています。',
    reading: 'ピーエスディーのレイヤーを、フレームごとに切り替えています。' },

  { id: 'sync-1', scene: 'sync', speaker: 'zunko', expression: 'puzzled',
    text: '台本を直したら、作り直し？' },
  { id: 'sync-2', scene: 'sync', speaker: 'kiritan', expression: 'smug',
    text: '声を作り直すだけ。映像は【音声の長さ】に合わせて動きます。' },

  { id: 'outro-1', scene: 'outro', speaker: 'zunko', expression: 'happy',
    text: '動画も、【コードで書ける】んだね。' },
  { id: 'outro-2', scene: 'outro', speaker: 'kiritan', expression: 'smile',
    text: 'Celesta で、作ってみてください。', reading: 'セレスタで、作ってみてください。', gap: 1.2 },
];

// The subtitle without its 【】 marks.
export function plainText(text: string): string {
  return text.replace(/[【】]/g, '');
}
