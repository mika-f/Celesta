// BGM の音量。最初のシーンは無音で、二人の登場とともに入り、台詞の間は控えめに、
// 最後の台詞のあとで少し上げてから消える。

import type { Animatable } from '@celesta/react';

import { DURATION, SCENE_AT, line } from './timing.ts';
import { FPS } from './theme.ts';

/** BGM を流し始めるフレーム。 */
export const SCORE_FROM = SCENE_AT.layers;

const UNDER_VOICES = 0.3;
const ALONE = 0.55;

/**
 * `<Audio volume>` に渡すキーフレーム。時刻は `<Audio>` を置いた <Sequence> の
 * 先頭（SCORE_FROM）からの秒なので、動画全体のフレームから引いて秒にする。
 */
export function scoreVolume(): Animatable<number> {
  const key = (frame: number, value: number) => ({ time: { value: frame - SCORE_FROM, timescale: FPS }, value });
  const lastWords = line('o2').at + line('o2').duration;
  const end = DURATION - 1;
  return {
    type: 'keyframes',
    keyframes: [
      key(SCORE_FROM, 0),
      key(SCORE_FROM + 12, UNDER_VOICES),
      key(lastWords, UNDER_VOICES),
      key(lastWords + 15, ALONE),
      key(end - 45, ALONE),
      key(end, 0),
    ],
  };
}
