import type { SubtitleCue } from '@celesta.studio/react';
import { CHAPTERS } from './chapters';
import { PRS } from './prs';
import { CHAPTER_COUNT, CHAPTER_LENGTHS, OPEN_LENGTH, OUTRO_AT, OUTRO_LENGTH, chapterAt } from './timeline';

// One caption per scene, in Japanese and English, for the subtitle files.
// <Subtitles> draws nothing; the export writes each language to its own
// file with a {lang} in the --subtitles path. The English of each chapter
// is the sentence lit on its module's strip.
export const CAPTION_LANGUAGES = ['ja', 'en'] as const;
export type CaptionLanguage = (typeof CAPTION_LANGUAGES)[number];

const TEXTS: Record<CaptionLanguage, readonly string[]> = {
  ja: [
    `Celesta v0.9.0 → v0.10.0。マージされた ${PRS.length} 本の PR を、ラックに差し込んでいきます`,
    'Δ01 CONTENT PLUGINS — コンテンツプラグインの別プロセスがフレームごとにメッシュを返し、プレビューでも書き出しでも Celesta が描く',
    'Δ02 3D SCENES — プラグインはカメラ・ライト・独自の WGSL マテリアルを持つ 3D シーンを返せる。Celesta が深度つきで描く',
    'Δ03 VRM — VRM プラグインが VRM のアバターを MToon で描き、VRM アニメーションとトラックを再生し、揺れ物を 60 Hz の固定の時計で揺らす',
    'Δ04 APPROVALS — プラグインを動かす前にアプリが確認し、書き出しと MCP もその答えに従う。承認待ちの間もエントリは代わりの表示で開く',
    'Δ05 MEDIA PLUGINS — 動画と音声の調査・デコード・エンコード・多重化は、それぞれ別プロセスのメディアプラグインが行う',
    'Δ06 NATIVE CODECS — macOS 版は AVFoundation の、Windows 版は Media Foundation のエンコーダーとデコーダーを同梱。GPL のライブラリなしで H.264',
    'Δ07 PIPELINE — フレームはプラグインのプロセスと共有するメモリを通り、書き出しは最大 3 フレームを先行させて、描画とエンコードを重ねる',
    'Δ08 REGISTRY — registry.celesta.studio は署名つきのファイルでプラグインを公開し、Celesta は組み込みの鍵で 1 つずつ確かめ、アーカイブとツリーも確かめる',
    'Δ09 CELESTA PLUGIN — celesta コマンドでレジストリを検索し、プロジェクトに追加し、ロックどおりに入れ、更新し、監査する',
    'Δ10 LOCK — celesta.lock が各プラグインのバージョンとハッシュを固定し、アプリ・書き出し・MCP はそのとおりに動かす。この映像も',
    'Δ11 FIND PLUGINS — Plugins タブの Find plugins… でレジストリを検索し、プロジェクトに追加し、コーデックを入れ、ロックしたものを更新する',
    'Δ12 INSPECTOR — Inspector は props をセクションに分け、各行をラベル・値・単位の 3 つのレーンに並べ、ラベルのドラッグで値を変える',
    'Δ13 TIMELINE — タイムコードはビューアーの下に 1 つ、細くなったルーラー、高さの決まったレーン。タイミング編集の残りの穴もふさいだ',
    'Δ14 EXPORT PAGE — 書き出しはページになり、設定がビューアーとタイムラインの横に並ぶ。コンポーネントはプラグイン名つきで、プリセットに保存できる',
    'MORE — x264 の別配布、BT.709、静止画、書き出しプリセット、@celesta.studio、crates.io、@font-face、インスタンスの選択、編集の穴、NaN、ツール、修正',
    'Celesta 0.10.0 — Every update is a diff.',
  ],
  en: [
    `Celesta v0.9.0 → v0.10.0, from ${PRS.length} merged pull requests, patched into a rack.`,
    ...CHAPTERS.map((chapter, i) => `Δ${String(i + 1).padStart(2, '0')} ${chapter.title} — ${chapter.caption}`),
    'MORE — x264 as a download, BT.709, stills, export presets, @celesta.studio, crates.io, @font-face, instances, editing gaps, NaN, tooling, and fixes.',
    'Celesta 0.10.0 — Every update is a diff.',
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
/** The Japanese the film draws nowhere but in its subtitle files. */
export const CAPTION_STRINGS = TEXTS.ja;
