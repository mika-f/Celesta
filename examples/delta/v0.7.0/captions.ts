import type { SubtitleCue } from '@celesta/react';
import { PRS } from './prs';
import { CHAPTER_COUNT, CHAPTER_LENGTHS, OPEN_LENGTH, OUTRO_AT, OUTRO_LENGTH, chapterAt } from './timeline';

// One caption per scene, in Japanese and English. <Subtitles> draws nothing;
// the export writes each language to its own file with
// --subtitles 'delta-0.7.0.{lang}.srt', as SRT or WebVTT by the extension.
export const CAPTION_LANGUAGES = ['ja', 'en'] as const;
export type CaptionLanguage = (typeof CAPTION_LANGUAGES)[number];

const TEXTS: Record<CaptionLanguage, readonly string[]> = {
  ja: [
    `Celesta v0.6.0 → v0.7.0。マージされた ${PRS.length} 本の PR から`,
    'Δ01 TRANSITIONS — 新しいトランジション 7 種と defineTransition()。この映像に同じ切り替えは 2 度出てきません',
    'Δ02 MASK — テキストやパスや部分木を、グループのマスクに',
    'Δ03 MOTION BLUR — シャッターが開いている間の平均を描く MotionBlur と、残像の Trail',
    'Δ04 EFFECTS — ハーフトーン・色収差・波・ノイズ・走査線・ズームブラーとクロマキーをプロパティで',
    'Δ05 COLOR — 色補正のプロパティと、順に適用されるシェーダーの配列',
    'Δ06 BLEND — W3C の 17 種のブレンドモードと PSD のブレンドレイヤー',
    'Δ07 SHAPES — 角を丸められる三角形・多角形・星・扇形・ハートと、破線・点線',
    'Δ08 PATHS — パスの長さ・トリム・モーフィングと SVG のパスデータ',
    'Δ09 SPECTRUM — 音声のスペクトルと音量をフレームごとに。このバーはこの映像の BGM です',
    'Δ10 MEDIA — 音の鳴る Video、ループ、アニメーション画像、透過動画の書き出し',
    'Δ11 MCP — コーディングエージェント向けの Celesta MCP サーバー',
    'Δ12 EDITOR — React のコンポジションの音声・動画クリップ、ジェスチャー、ソースを開く、Problems タブ',
    'MORE — 複数のコンポジション、Google Fonts、字幕ファイル、セーフエリアほか',
    'Celesta 0.7.0 — Every update is a diff.',
  ],
  en: [
    `Celesta v0.6.0 → v0.7.0, from ${PRS.length} merged pull requests.`,
    'Δ01 TRANSITIONS — seven new types and defineTransition(). No two cuts in this film are alike.',
    "Δ02 MASK — text, paths and whole subtrees as a group's mask.",
    'Δ03 MOTION BLUR — MotionBlur averages an open shutter; Trail leaves copies behind.',
    'Δ04 EFFECTS — halftone, chromatic aberration, wave, noise, scanlines, zoom blur and chroma key as props.',
    'Δ05 COLOR — color filter props, and shader arrays that run in order.',
    'Δ06 BLEND — all seventeen W3C blend modes, and PSD blend layers.',
    'Δ07 SHAPES — triangles, polygons, stars, pies and hearts with round corners, and dashed strokes.',
    'Δ08 PATHS — measure, trim and morph paths, and read SVG path data.',
    "Δ09 SPECTRUM — a file's spectrum and loudness, frame by frame. These bars are this film's score.",
    'Δ10 MEDIA — videos with sound, loops, animated images and transparent export.',
    'Δ11 MCP — the Celesta MCP server for coding agents.',
    "Δ12 EDITOR — a React composition's audio and video clips, gestures, layer sources and the Problems tab.",
    'MORE — multiple compositions, Google Fonts, subtitle files, safe areas and more.',
    'Celesta 0.7.0 — Every update is a diff.',
  ],
};

function cues(texts: readonly string[]): readonly SubtitleCue[] {
  return [
    { from: 0, durationInFrames: OPEN_LENGTH, text: texts[0] },
    ...Array.from({ length: CHAPTER_COUNT }, (_, i) => ({ from: chapterAt(i), durationInFrames: CHAPTER_LENGTHS[i], text: texts[i + 1] })),
    { from: OUTRO_AT, durationInFrames: OUTRO_LENGTH, text: texts[CHAPTER_COUNT + 1] },
  ];
}

export const CAPTIONS: Record<CaptionLanguage, readonly SubtitleCue[]> = { ja: cues(TEXTS.ja), en: cues(TEXTS.en) };
