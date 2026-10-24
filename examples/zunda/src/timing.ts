// 台本（script.ts）と音声の長さ（voices.json）から、すべての台詞とシーンの
// フレームを決める。映像側はここで決まったフレームだけを見て動くので、台本を
// 直して make-voices.ts を実行し直せば、字幕・口パク・シーンの切り替えが
// 新しい音声の長さに追従する。

import { useCurrentFrame } from '@celesta/react';

import { LINES } from '../script.ts';
import type { Line, SceneId } from '../script.ts';
import voicesJson from '../voices.json';
import { FPS } from './theme.ts';

type VoiceInfo = { file: string; kana: string; seconds: number };
const VOICES = voicesJson as Record<string, VoiceInfo>;

/** 台本の 1 行に、再生するフレームと音声を足したもの。 */
export type TimedLine = Line & {
  /** 台詞が始まるフレーム（動画全体での番号） */
  at: number;
  /** 音声の長さ（フレーム） */
  duration: number;
  /** 次の台詞までの長さ。音声のあとの間を含む。字幕はこの間ずっと出す */
  hold: number;
  /** 音声ファイル（エントリからの相対パス） */
  voice: string;
  /** VOICEVOX が返した読み（口パク用） */
  kana: string;
};

/** シーンの並び順。台本に出てくる順と同じにする。 */
export const SCENE_ORDER: readonly SceneId[] = [
  'void', 'layers', 'effects', 'path', 'timeline', 'voice', 'rewind', 'agent', 'outro',
];

/** 各シーンの最初の台詞の前に置く間（フレーム）。登場や切り替えの演出に使う。 */
const LEAD: Record<SceneId, number> = {
  void: 84, layers: 26, effects: 16, path: 22, timeline: 26, voice: 18, rewind: 22, agent: 30, outro: 26,
};
/** 最後の台詞のあとの余韻（フレーム）。 */
const TAIL = 96;
/** 台本で `pause` を省略した行の、音声のあとの間（秒）。 */
const DEFAULT_PAUSE = 0.25;

const timed: TimedLine[] = [];
const sceneStart: Partial<Record<SceneId, number>> = {};
let cursor = 0;
for (const line of LINES) {
  if (sceneStart[line.scene] === undefined) {
    sceneStart[line.scene] = cursor;
    cursor += LEAD[line.scene];
  }
  const voice = VOICES[line.id];
  if (!voice) {
    throw new Error(`voices.json has no line "${line.id}"; run make-voices.ts`);
  }
  const duration = Math.ceil(voice.seconds * FPS);
  const hold = duration + Math.round((line.pause ?? DEFAULT_PAUSE) * FPS);
  timed.push({ ...line, at: cursor, duration, hold, voice: voice.file, kana: voice.kana });
  cursor += hold;
}

export const TIMED_LINES: readonly TimedLine[] = timed;
export const SCENE_AT = sceneStart as Record<SceneId, number>;
export const DURATION = cursor + TAIL;

const byId = new Map(timed.map((line) => [line.id, line]));

/** 台詞 id から、その台詞のタイミング。台本に無い id は例外にする（打ち間違いの早期発見）。 */
export function line(id: string): TimedLine {
  const found = byId.get(id);
  if (!found) throw new Error(`script.ts has no line "${id}"`);
  return found;
}

/** 台詞の途中（音声の長さに対して 0〜1 の位置）のフレーム。「この言葉のあたりで」を指定するのに使う。 */
export function partway(id: string, fraction: number): number {
  const { at, duration } = line(id);
  return at + Math.round(duration * fraction);
}

/** シーンが終わる（次のシーンが始まる）フレーム。 */
export function sceneEnd(id: SceneId): number {
  const next = SCENE_ORDER[SCENE_ORDER.indexOf(id) + 1];
  return next ? SCENE_AT[next] : DURATION;
}

/** `frame` の時点で最後に始まった台詞。 */
export function lineAt(frame: number): TimedLine | undefined {
  let current: TimedLine | undefined;
  for (const t of TIMED_LINES) {
    if (t.at > frame) break;
    current = t;
  }
  return current;
}

/**
 * シーンの `<Sequence>` の中で、動画全体のフレーム番号を返す。
 *
 * 台詞のタイミングは動画全体のフレームで決まっているので、シーンの中でも
 * 全体のフレームで比べる方が読みやすい。`useCurrentFrame()` はシーンの
 * 先頭からのフレームなので、シーンの開始フレームを足す。
 */
export function useFilmFrame(scene: SceneId): number {
  return SCENE_AT[scene] + useCurrentFrame();
}
