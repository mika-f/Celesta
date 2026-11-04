import type { SubtitleCue } from '@celesta.studio/react';
import { PRS } from './prs';
import { CHAPTER_COUNT, CHAPTER_LENGTHS, OPEN_LENGTH, OUTRO_AT, OUTRO_LENGTH, chapterAt } from './timeline';

// One caption per scene, in Japanese and English. <Subtitles> draws nothing;
// the export writes each language to its own file with a {lang} in the
// --subtitles path, as SRT or WebVTT by the file's extension.
export const CAPTION_LANGUAGES = ['ja', 'en'] as const;
export type CaptionLanguage = (typeof CAPTION_LANGUAGES)[number];

const TEXTS: Record<CaptionLanguage, readonly string[]> = {
  ja: [
    `Celesta v0.7.0 → v0.8.0。マージされた ${PRS.length} 本の PR から`,
    'Δ01 3D — rotationX・rotationY・perspective・skew を、originX・originY を軸に。backfaceVisibility で裏面を隠す',
    'Δ02 LOTTIE — After Effects の Lottie をフレームに合わせて描く。速さも向きも自由に',
    'Δ03 COMPONENTS — カウンター・統計カード・棒グラフ・バッジ・パネル・模様の @celesta.studio/components',
    'Δ04 TITLES — タイトル画面の TitleCard、入りと抜けの Motion、キャレット付きの TypewriterText',
    'Δ05 THEME — Text の省略 props と、フォントと色を名前で引ける Theme',
    'Δ06 STROKES — 文字のフチをリストで。二重フチの字幕が 1 つのレイヤーで',
    'Δ07 METRICS — キャップハイト・エックスハイトと、字形ごとのインクの範囲',
    'Δ08 PITCH — Audio の pitch と preservePitch。このリフは 1 つのファイルを移調したものです',
    'Δ09 FALLBACK — 足りない文字を読み込んだフォントから。使う文字だけの Google Fonts',
    'Δ10 EDGES — レイヤーの外を読む source_at_clamped と source_at_mirrored',
    'Δ11 EXPORTER — Node.js から書き出す @celesta.studio/exporter。JPEG と WebP の静止画',
    'Δ12 VOICES — ボイスフォルダーの読み込み、声量合わせ、表情のキュー',
    'MORE — 名前付きの章、Loop、キャンバスからの書き戻し、静止画、字幕ファイル、HDR 動画ほか',
    'Celesta 0.8.0 — Every update is a diff.',
  ],
  en: [
    `Celesta v0.7.0 → v0.8.0, from ${PRS.length} merged pull requests.`,
    'Δ01 3D — rotationX, rotationY, perspective and skew about any originX and originY, and backfaceVisibility.',
    'Δ02 LOTTIE — After Effects animations drawn in step with the frame, at any speed, either way.',
    'Δ03 COMPONENTS — counters, stat cards, bar charts, badges, panels and patterns in @celesta.studio/components.',
    'Δ04 TITLES — TitleCard for title screens, Motion for entrances and exits, TypewriterText with a measured caret.',
    'Δ05 THEME — font and color props on Text, and a Theme that names them.',
    "Δ06 STROKES — a list of text strokes. A subtitle's double outline is one layer.",
    "Δ07 METRICS — the font's cap and x heights, and every glyph's ink.",
    'Δ08 PITCH — pitch and preservePitch on Audio. This riff is one file, transposed.',
    'Δ09 FALLBACK — missing glyphs from the fonts you loaded, and Google Fonts with only the characters drawn.',
    "Δ10 EDGES — source_at_clamped and source_at_mirrored read past a layer's edge.",
    'Δ11 EXPORTER — @celesta.studio/exporter exports from Node.js. Stills as JPEG and WebP.',
    'Δ12 VOICES — voice folders, matched levels and expression cues.',
    'MORE — named scenes, Loop, canvas edits written back, stills, subtitle files, HDR video and more.',
    'Celesta 0.8.0 — Every update is a diff.',
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
