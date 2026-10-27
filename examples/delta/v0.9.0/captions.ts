import type { SubtitleCue } from '@celesta/react';
import { CHAPTERS } from './chapters';
import { PRS } from './prs';
import { CHAPTER_COUNT, CHAPTER_LENGTHS, OPEN_LENGTH, OUTRO_AT, OUTRO_LENGTH, chapterAt } from './timeline';

// One caption per scene, in Japanese and English, for the subtitle files.
// <Subtitles> draws nothing; the export writes each language to its own
// file with a {lang} in the --subtitles path. The English of each chapter
// is the sentence lit under its monitor.
export const CAPTION_LANGUAGES = ['ja', 'en'] as const;
export type CaptionLanguage = (typeof CAPTION_LANGUAGES)[number];

const TEXTS: Record<CaptionLanguage, readonly string[]> = {
  ja: [
    `Celesta v0.8.0 → main。マージされた ${PRS.length} 本の PR から、放送でお送りします`,
    'Δ01 LOWER THIRD — 名前と肩書きを、文字に合わせた帯で、画面の隅のセーフエリアの内側に',
    'Δ02 CALLOUT — 文字に合わせた注記と、対象までの線。動く対象にも線がついていく',
    'Δ03 CAPTIONS — データから時間つきの字幕を描き、単語や文節ごとにハイライトする CaptionTrack',
    'Δ04 DIALOGUE — 台詞は spokenText を読み、text をページごとに表示し、大事な語を強調する',
    'Δ05 FADES — Audio の fadeIn と fadeOut を秒で。聞こえる範囲の端で、Sequence の長さに追従する',
    'Δ06 DUCKING — 声に duckGroup、音楽に duck を付けると、声の間だけ音楽が下がる。プレビューでも書き出しでも',
    'Δ07 LOUDNESS — 書き出しのたびにミックスを測り、--loudness で目標の LUFS に 1 つのゲインで合わせる',
    'Δ08 PROPERTIES — Properties タブでプリミティブとコンポーネント呼び出しの props を編集し、変わる場面を示す',
    'Δ09 CANVAS — キャンバスで中心やピボットを軸に回し、辺ごとに大きさを変え、文字をその場で編集する',
    'Δ10 EDIT SCOPE — 複数の場面で描かれる要素の編集を、この場面だけにするか、共有の値にするか選べる',
    'Δ11 TIMELINE — タイムラインで帯の端や中央、場面の終わりをドラッグすると、from と durationInFrames が書き戻される',
    'Δ12 ADD — Add から Rect・Ellipse・Circle・Text・Image・Video を選んでキャンバスに描くと、グループに書き込まれる',
    'MORE — 強調の範囲、fontFeatures、図形の Theme 色名、Video の幅と高さ、章の一覧、useScene の時計、失敗したフレームの名指し、ほか',
    'Celesta 0.9.0 — Every update is a diff.',
  ],
  en: [
    `Celesta v0.8.0 → main, from ${PRS.length} merged pull requests. We are on air.`,
    ...CHAPTERS.map((chapter, i) => `Δ${String(i + 1).padStart(2, '0')} ${chapter.title} — ${chapter.caption}`),
    "MORE — emphasis ranges, fontFeatures, Theme colors in shapes, Video width and height, chapters in the preview, useScene on the composition's clock, failed frames named, and more.",
    'Celesta 0.9.0 — Every update is a diff.',
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
