// シーンの切り替わりで、緑とピンクの斜めの帯が画面を横切る。
// 帯が画面を覆っている瞬間にシーンが入れ替わるので、切れ目が見えない。

import { Easings, Rect, interpolate, useCurrentFrame } from '@celesta/react';

import { COLOR } from '../theme.ts';

/** 切り替わりの前後それぞれのフレーム数 */
const HALF = 9;

/** `at` の各フレーム（シーンの切り替わり）を中心に、帯を走らせる。 */
export function Wipes({ at }: { at: readonly number[] }) {
  const frame = useCurrentFrame();
  const cut = at.find((cutAt) => frame >= cutAt - HALF && frame < cutAt + HALF);
  if (cut === undefined) return null;
  const t = (frame - (cut - HALF)) / (HALF * 2);
  return (
    <>
      {[COLOR.zunda, COLOR.metan].map((color, i) => {
        // ピンクの帯は少し遅れて追いかける。
        const p = Easings.easeInOutCubic(Math.min(1, Math.max(0, t - i * 0.08) / 0.92));
        return <Rect key={color} x={interpolate(p, [0, 1], [-3000, 2300])} y={-500} width={2700} height={2100}
          rotation={14} fill={color} />;
      })}
    </>
  );
}
