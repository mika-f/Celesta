// 台詞ごとの口パクのトラック。音声ファイルと読みがなから prepare() で作る。

import { loadLipSync } from '@celesta/react';
import type { LipSyncTrack } from '@celesta/react';

import { TIMED_LINES } from '../timing.ts';

const tracks = new Map<string, LipSyncTrack>();

/** 全台詞の口パクを読み込む。エントリの prepare() から呼ぶ。 */
export async function prepareLipSync(): Promise<void> {
  for (const t of TIMED_LINES) {
    // 読みがなは VOICEVOX の AquesTalk 風の表記（アクセント記号つき）だが、
    // loadLipSync は仮名の母音だけを読むので、そのまま渡してよい。
    tracks.set(t.id, await loadLipSync({ src: t.voice, text: t.kana }));
  }
}

/** 台詞の口パク。prepare() の前は undefined。 */
export function lipSyncOf(id: string): LipSyncTrack | undefined {
  return tracks.get(id);
}
