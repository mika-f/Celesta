// 動画のどのフレームでも、その場でもう一度描く。
//
// Celesta の絵はフレーム番号の純関数なので、子の <Sequence> の 0 フレームが
// 「いま」に来るように開始をずらせば、子は好きなフレームを描く。録画した
// サムネイルではなく、本物のシーンのコンポーネントがそのフレームを描き直す。
//
// 描き直す中身（動画の「世界」）は WorldContext で受け取る。シーンの一覧を
// 直接 import すると、そのシーン自身からも import することになり循環するため。

import { createContext, useContext } from 'react';
import type { ComponentType } from 'react';
import { Sequence, useCurrentFrame } from '@celesta/react';

/** 動画の背景・シーン・画面固定の表示をまとめて描くコンポーネント。 */
export const WorldContext = createContext<ComponentType | null>(null);

/**
 * `frame`（動画全体のフレーム番号）の世界を描く。立ち絵と音声は含めない
 * （立ち絵のビューは 1 つの ref で 1 か所にしか置けず、音声は二重に鳴るため）。
 */
export function FrameRecall({ frame }: { frame: number }) {
  const World = useContext(WorldContext);
  const now = useCurrentFrame();
  if (!World) throw new Error('<FrameRecall> needs a WorldContext provider');
  return (
    <Sequence from={now - frame}>
      <World />
    </Sequence>
  );
}
