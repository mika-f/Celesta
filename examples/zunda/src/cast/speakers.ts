// 話者ごとの名前と色。字幕の縁、名前札、ワイプの帯で共通に使う。

import type { Speaker } from '../../script.ts';
import { COLOR } from '../theme.ts';

export const SPEAKERS: Record<Speaker, { name: string; color: string; deep: string }> = {
  zunda: { name: 'ずんだもん', color: COLOR.zunda, deep: COLOR.zundaDeep },
  metan: { name: '四国めたん', color: COLOR.metan, deep: COLOR.metanDeep },
};
