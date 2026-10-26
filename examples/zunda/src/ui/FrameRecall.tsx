// 動画のどのフレームでも、その場でもう一度描く。
//
// Celesta の絵はフレーム番号の純関数なので、<FreezeFrame frame> の中に動画の
// 「世界」を置けば、録画したサムネイルではなく、本物のシーンや立ち絵の
// コンポーネントがそのフレームを描き直す。中の音は鳴らず、立ち絵のビューの ref
// も外の本物のビューに残る。
//
// 描き直す中身は WorldContext で受け取る。film.tsx の世界を直接 import すると、
// その世界の中にあるシーン自身からも import することになり循環するため。

import { createContext, useContext } from 'react';
import type { ComponentType } from 'react';
import { FreezeFrame } from '@celesta/react';

/** 動画の画面（背景・シーン・立ち絵）を描くコンポーネント。 */
export const WorldContext = createContext<ComponentType | null>(null);

/** `frame`（動画全体のフレーム番号）の画面を描く。 */
export function FrameRecall({ frame }: { frame: number }) {
  const World = useContext(WorldContext);
  if (!World) throw new Error('<FrameRecall> needs a WorldContext provider');
  return (
    <FreezeFrame frame={frame}>
      <World />
    </FreezeFrame>
  );
}
