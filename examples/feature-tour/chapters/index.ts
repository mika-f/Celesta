import type { ReactNode } from 'react';
import { BAR, C } from '../constants';
import { AgentDemo } from './AgentDemo';
import { ExportDemo } from './ExportDemo';
import { LayoutDemo } from './LayoutDemo';
import { MotionDemo } from './MotionDemo';
import { PreviewDemo } from './PreviewDemo';
import { ReactDemo } from './ReactDemo';
import { TimelineDemo } from './TimelineDemo';
import { TypeDemo } from './TypeDemo';
import { VoiceDemo } from './VoiceDemo';

// The nine feature chapters, in order. Each is a headline, a Japanese caption,
// the API that does it, and a live demo (one file per demo in this folder).
export type Chapter = {
  key: string;
  en: string;
  ja: string;
  jaShort: string;
  api: [number, string][];
  len: number;
  bg: string;
  Demo: () => ReactNode;
};

export const CHAPTERS: Chapter[] = [
  {
    key: 'REACT', en: 'Write it\nin React.', jaShort: 'React で組む',
    ja: '動画は、React コンポーネント。\nRect・Text・Group を重ねて、\n1 枚のフレームを組み立てる。',
    api: [[0, '<Composition fps={30}>']], len: BAR * 2, bg: C.ink, Demo: ReactDemo,
  },
  {
    key: 'MOTION', en: 'Move it\nwith math.', jaShort: '動きを計算する',
    ja: 'interpolate・spring・Easings。\n動きはすべて、フレーム番号から計算する。',
    api: [[0, 'spring({ frame, fps })']], len: BAR * 2, bg: C.ink, Demo: MotionDemo,
  },
  {
    key: 'LAYOUT', en: 'Lay it\nall out.', jaShort: 'レイアウト',
    ja: 'Grid・Stack・Center・Fit。\n座標の計算は、レイアウトに任せる。',
    api: [[0, '<Grid columns={4}>'], [45, '<Stack direction="horizontal">'], [90, '<Center>']],
    len: BAR * 2, bg: C.ink, Demo: LayoutDemo,
  },
  {
    key: 'TYPE', en: 'Set any\ntype.', jaShort: 'フォントと文字',
    ja: 'Google Fonts もローカルのフォントも、\n<Font> ひとつで。和文も、縁取りも。',
    api: [[0, '<Font src="https://fonts…" />']], len: BAR * 2, bg: C.ink, Demo: TypeDemo,
  },
  {
    key: 'TIMELINE', en: 'Or lay out\na timeline.', jaShort: 'JSON タイムライン',
    ja: '.celesta.json にクリップを並べる。\nReact のコンポーネントも、同じタイムラインに。',
    api: [[0, 'project.celesta.json'], [60, "registerComponent('LowerThird')"]],
    len: BAR * 2, bg: C.ink, Demo: TimelineDemo,
  },
  {
    key: 'VOICE', en: 'Give it\na voice.', jaShort: '立ち絵・字幕・口パク',
    ja: 'PSD の立ち絵に、字幕と音声。\n口の動きは、音声から自動で生成する。',
    api: [[0, 'loadLipSync({ src, text })']], len: BAR * 3, bg: C.ink, Demo: VoiceDemo,
  },
  {
    key: 'PREVIEW', en: 'Scrub any\nframe.', jaShort: 'プレビュー',
    ja: 'フレーム単位でスクラブ。\nどのフレームも、何度描いても同じ絵になる。',
    api: [[0, 'useCurrentFrame()']], len: BAR * 2, bg: C.ink, Demo: PreviewDemo,
  },
  {
    key: 'EXPORT', en: 'Ship it\nas MP4.', jaShort: 'MP4 書き出し',
    ja: 'H.264 + AAC の MP4 に。\nアプリからも、コマンド一発でも。',
    api: [[0, 'Celesta-export --react film.tsx']], len: BAR * 2, bg: C.blue, Demo: ExportDemo,
  },
  {
    key: 'AGENTS', en: 'Let agents\ndirect.', jaShort: 'AI エージェント',
    ja: 'Skill を読んだ AI エージェントが、\n書いて、確かめて、書き出す。\nこの動画も、そうして作られた。',
    api: [[0, 'skills/celesta/SKILL.md']], len: BAR * 2, bg: C.ink, Demo: AgentDemo,
  },
];

// Chapter start frames, back to back after the index.
export const CHAPTER_AT: number[] = [];
{
  let at = BAR * 4;
  for (const chapter of CHAPTERS) {
    CHAPTER_AT.push(at);
    at += chapter.len;
  }
}
