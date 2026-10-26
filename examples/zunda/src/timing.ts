// 台詞とシーンのタイミング。prepare() で台本（script.ts）を VOICEVOX に読み上げさせ
// （prepareNarration）、音声の長さから全台詞のフレームを決める（planDialogue）。
// 映像側はここで決まったフレームだけを見て動くので、台本を直せば、字幕・口パク・
// シーンの切り替えが新しい音声の長さに追従する。
//
// 計画（plan）は prepare() のあとにしかないので、台詞のフレームはモジュールの
// 読み込み時ではなく、描画中に関数で引く（`line('t1').from` など）。

import { useCurrentFrame } from '@celesta/react';
import { planDialogue } from '@celesta/character';
import type { DialoguePlan, PlannedDialogueLine } from '@celesta/character';
import { prepareNarration } from '@celesta/voicevox';
import type { NarratedLine, NarrationLine, VoicevoxVoice } from '@celesta/voicevox';

import { LINES } from '../script.ts';
import type { Line, SceneId } from '../script.ts';
import { FPS } from './theme.ts';

/** VOICEVOX Engine の場所。音声がキャッシュにあれば、Engine は起動していなくてよい。 */
const ENGINE = process.env.VOICEVOX_URL ?? 'http://127.0.0.1:50021';
/** 二人とも同じ読み上げ設定。スタイル（声色）は台本の行ごとに `style` で選ぶ。 */
const QUERY = { speedScale: 1.1, outputSamplingRate: 48000, prePhonemeLength: 0.05, postPhonemeLength: 0.1 };
const VOICES: Record<Line['speaker'], VoicevoxVoice> = {
  zunda: { baseUrl: ENGINE, styleId: 3, query: QUERY },
  metan: { baseUrl: ENGINE, styleId: 2, query: QUERY },
};

/** 各シーンの最初の台詞の前に置く間（フレーム）。登場や切り替えの演出に使う。 */
const LEAD: Record<SceneId, number> = {
  void: 84, layers: 26, effects: 16, path: 22, timeline: 26, voice: 18, rewind: 22, agent: 30, outro: 26,
};
/** 台本で `gap` を省略した行の、音声のあとの間（秒）。 */
const DEFAULT_GAP = 0.25;
/** 最後の台詞のあとの余韻（フレーム）。 */
const TAIL = 96;

/** 台本の 1 行に、prepareNarration が音声（`audio`）と口パク（`lipSync`）を足したもの。 */
export type VoicedLine = NarratedLine<Line & NarrationLine>;
/** 計画の中の 1 行。`from`（始まるフレーム）、`durationInFrames`（音声の長さ）、`spanInFrames`（あとの間を含む長さ）を持つ。 */
export type TimedLine = PlannedDialogueLine<VoicedLine>;

let plan: DialoguePlan<VoicedLine> | undefined;

/** 音声を用意して、全台詞のフレームを決める。エントリの prepare() から呼ぶ。 */
export async function prepareTiming(): Promise<void> {
  const lines = LINES.map((line, i) => ({
    ...line,
    // 字幕の改行（\n）は読み上げには渡さない
    spokenText: line.spokenText ?? line.text.replace(/\n/g, ''),
    // シーンの最初の行は、前置きの間を空けてから始まる。最後の行のあとは余韻を残す。
    leadIn: i === 0 || LINES[i - 1].scene !== line.scene ? LEAD[line.scene] / FPS : undefined,
    gap: i === LINES.length - 1 ? TAIL / FPS : line.gap,
    voice: { styleId: line.style },
  }));
  // 音声は ./.celesta/voices にキャッシュされ、読み上げる文やスタイルが変わった行だけ作り直される。
  const narrated = await prepareNarration(lines, { speakers: VOICES, cacheDir: './.celesta/voices' });
  plan = await planDialogue(narrated, { fps: FPS, gap: DEFAULT_GAP });
}

/** 台詞の計画。prepare() の前に呼ぶと例外にする。 */
export function dialoguePlan(): DialoguePlan<VoicedLine> {
  if (!plan) throw new Error('the dialogue plan is not ready: call prepareTiming() from prepare()');
  return plan;
}

/** 台詞 id から、その台詞のタイミング。台本に無い id は例外にする（打ち間違いの早期発見）。 */
export function line(id: string): TimedLine {
  return dialoguePlan().line(id);
}

/** 台詞の途中（音声の長さに対して 0〜1 の位置）のフレーム。「この言葉のあたりで」を指定するのに使う。 */
export function partway(id: string, fraction: number): number {
  const { from, durationInFrames } = line(id);
  return from + Math.round(durationInFrames * fraction);
}

/** シーンが始まるフレーム（最初の台詞の前置きの頭）。 */
export function sceneStart(id: SceneId): number {
  return dialoguePlan().scene(id).from;
}

/** シーンが終わる（次のシーンが始まる）フレーム。 */
export function sceneEnd(id: SceneId): number {
  const scene = dialoguePlan().scene(id);
  return scene.from + scene.durationInFrames;
}

/** 動画全体の長さ（フレーム）。 */
export function filmDuration(): number {
  return dialoguePlan().durationInFrames;
}

/**
 * シーンの `<Sequence>` の中で、動画全体のフレーム番号を返す。
 *
 * 台詞のタイミングは動画全体のフレームで決まっているので、シーンの中でも
 * 全体のフレームで比べる方が読みやすい。`useCurrentFrame()` はシーンの
 * 先頭からのフレームなので、シーンの開始フレームを足す。
 */
export function useFilmFrame(scene: SceneId): number {
  return sceneStart(scene) + useCurrentFrame();
}
