// 冒頭から「エフェクト」のシーンまで続く、育っていく背景。
// 真っ黒 → Rect 一枚 → グラデーション → 写真 → 写真がぼける、の順に、台詞に
// 合わせて重なっていく。2 つのシーンにまたがるので、シーンとは別に描く。

import { Camera, Easings, Group, Image, Rect, interpolate, progress, useCurrentFrame } from '@celesta/react';

import { SCENE_AT, line, partway } from '../timing.ts';
import { ASSET, CANVAS, CLAMP, COLOR } from '../theme.ts';

const { width: W, height: H } = CANVAS;

/** 背景が変わるフレーム。コードパネルや中央のラベルも同じ瞬間に出す。 */
export const BACKDROP_CUE = {
  /** 「Rect を一枚置けば背景になるわ」 */
  rect: line('l4').at - 2,
  /** 「グラデーションも指定できるの」 */
  gradient: partway('l4', 0.55),
  /** 「Image の fit を cover にすれば」 */
  photo: partway('l6', 0.4),
  /** 「blur でぼかし」 */
  blur: partway('e2', 0.56),
  /** 背景を描き終えるフレーム（次の「Path で描く」のシーンから別の背景） */
  end: SCENE_AT.path,
} as const;

export const BLUR_RADIUS = 14;

/** 写真のぼかしの半径（px）。コードパネルにも同じ数を出す。 */
export function blurAt(frame: number): number {
  return interpolate(frame, [BACKDROP_CUE.blur, BACKDROP_CUE.blur + 45], [0, BLUR_RADIUS],
    { ...CLAMP, easing: Easings.easeInOutSine });
}

/** 動画の先頭から置く。`useCurrentFrame()` がそのまま動画全体のフレームになる。 */
export function GrowingBackground() {
  const frame = useCurrentFrame();
  const rect = progress(frame, BACKDROP_CUE.rect, 18, Easings.easeOutExpo);
  const gradient = progress(frame, BACKDROP_CUE.gradient, 20, Easings.easeInOutSine);
  const photo = progress(frame, BACKDROP_CUE.photo, 24, Easings.easeOutCubic);
  // 写真はゆっくり寄り続ける（Ken Burns）。
  const zoom = 1.04 + 0.1 * progress(frame, BACKDROP_CUE.photo, BACKDROP_CUE.end - BACKDROP_CUE.photo);
  return (
    <>
      <Rect width={W} height={H} fill={COLOR.ink} />
      {rect > 0 && (
        // 左から右へ塗られていく
        <Group clip={{ width: W * rect, height: H }}>
          <Rect width={W} height={H} fill={COLOR.leaf} />
        </Group>
      )}
      {gradient > 0 && (
        <Rect width={W} height={H} opacity={gradient} fill={{
          type: 'linear', start: { x: 0, y: 0 }, end: { x: 600, y: H },
          stops: [{ offset: 0, color: '#C3EC78' }, { offset: 1, color: '#1F5A1A' }],
        }} />
      )}
      {photo > 0 && (
        <>
          <Group opacity={photo} blur={blurAt(frame)}>
            <Camera zoom={zoom} x={1000} y={520}>
              {/* ぼかしたときに端が透けないよう、画面より少し大きく敷く */}
              <Image src={ASSET.photo} x={-40} y={-24} width={W + 80} height={H + 48} fit="cover" />
            </Camera>
          </Group>
          {/* 字幕が読めるよう、下側を暗くする */}
          <Rect width={W} height={H} opacity={photo} fill={{
            type: 'linear', start: { x: 0, y: 420 }, end: { x: 0, y: H },
            stops: [{ offset: 0, color: '#00000000' }, { offset: 1, color: '#000000A0' }],
          }} />
        </>
      )}
    </>
  );
}
