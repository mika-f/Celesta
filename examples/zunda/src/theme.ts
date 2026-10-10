// 動画全体で共有する定数：画面、色、書体、素材の場所。
// 見た目を変えたいときは、まずここを触る。

import type { Paint } from '@celesta/react';

export const CANVAS = { width: 1920, height: 1080 } as const;
export const FPS = 30;

export const COLOR = {
  ink: '#050607',
  white: '#FFFFFF',
  // ずんだもん・四国めたんのイメージカラー。字幕の縁や名前札にも使う。
  zunda: '#7CC242',
  zundaDeep: '#2F6B1F',
  metan: '#E0549B',
  metanDeep: '#8E2A62',
  leaf: '#2C6B1F',
  paper: '#FFF9EC',
  navy: '#0E1420',
  // コードパネル
  panel: '#11161CEE',
  mute: '#8A96A3',
  code: '#E6EDF3',
  string: '#B8E986',
  number: '#FFD166',
  tag: '#7FD1FF',
  attribute: '#FF9BCB',
} as const;

export const FONT = {
  /** 字幕・見出しの和文 */
  ja: 'M PLUS Rounded 1c',
  /** タイトル・ロゴ */
  title: 'Dela Gothic One',
  /** コードとターミナル */
  mono: 'JetBrains Mono',
} as const;

/** JetBrains Mono はどの文字も 0.6 em 進むので、桁から x 座標を計算できる。 */
export const MONO_ADVANCE = 0.6;

/** 第三者の素材。prepare-assets.ts が assets/ に置く（リポジトリには含めない）。 */
export const ASSET = {
  zunda: './assets/zundamon.psd',
  metan: {
    talk: './assets/metan-talk.psd',
    happy: './assets/metan-happy.psd',
    worried: './assets/metan-worried.psd',
    what: './assets/metan-what.psd',
  },
  photo: './assets/field.jpg',
  clip: './assets/field.mp4',
  score: './assets/score.wav',
} as const;

export const solid = (color: string): Paint => ({ type: 'solid', color });

/** `#RRGGBB` に 0–1 の不透明度を付けて `#RRGGBBAA` にする。 */
export function alpha(color: string, opacity: number): string {
  const byte = Math.round(Math.min(1, Math.max(0, opacity)) * 255);
  return `${color.slice(0, 7)}${byte.toString(16).padStart(2, '0')}`;
}

/** `interpolate()` の範囲外を端の値に止める。フェードや移動では基本これを使う。 */
export const CLAMP = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;
