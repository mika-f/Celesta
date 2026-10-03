// 二人の立ち絵と台詞。
//
// - <CastAssets>   … <Assets> の中に置く、キャラクターの宣言
// - <Cast>         … 立ち絵の表示（カメラの内側。寄ると一緒に大きくなる）
// - <DialogueLines> … 台詞ごとの字幕と音声と口パク（カメラの外側）
//
// 表情は、台詞（<Dialogue expression>）ではなくビュー側で決める。聞いている間も
// 最後に話したときの表情を保ち、四国めたんはポーズごとにキャラクターを差し替えるので、
// 台詞の区間の外でも表情が決まっている必要があるため。まばたきは立ち絵の `blink` に任せる。

import { createRef } from 'react';
import type { RefObject } from 'react';
import { Character, CharacterView, Dialogue, Easings, Sequence, progress, useCurrentFrame } from '@celesta/react';
import type { AssetReference, CharacterViewReference } from '@celesta/react';

import type { MetanFace, Speaker, ZundaFace } from '../../script.ts';
import { TIMED_LINES, line } from '../timing.ts';
import { subtitleFor, wrapSubtitle } from '../ui/Subtitle.tsx';
import { faceAt, hopAt, swayAt } from './acting.ts';
import { lipSyncOf } from './lipsync.ts';
import { METAN_PORTRAITS, ZUNDA_PORTRAIT } from './portraits.ts';
import { SPEAKERS } from './speakers.ts';

/**
 * 立ち絵の置き場所。PSD の左上を (x, y) に置き、`scale` で縮める。
 * `face` はそのときの顔の中心で、カメラで寄るときの目安になる。
 */
export const PLACEMENT = {
  zunda: { x: 1382, y: 106, scale: 0.56, face: { x: 1700, y: 430 } },
  metan: { x: -168, y: 234, scale: 0.66, face: { x: 250, y: 740 } },
} as const;

/** 台詞が「どの立ち絵の口を動かすか」を指すためのビュー。 */
export const VIEW: Record<Speaker, RefObject<CharacterViewReference>> = {
  zunda: createRef<CharacterViewReference>(),
  metan: createRef<CharacterViewReference>(),
};

const zunda = createRef<AssetReference>();
const metanPoses = Object.fromEntries(
  (Object.keys(METAN_PORTRAITS) as MetanFace[]).map((pose) => [pose, createRef<AssetReference>()]),
) as Record<MetanFace, RefObject<AssetReference>>;

/** キャラクターの宣言。`<Assets>` の中に置く。 */
export function CastAssets() {
  return (
    <>
      <Character ref={zunda} id="zunda" name={SPEAKERS.zunda.name}
        portrait={ZUNDA_PORTRAIT} subtitle={subtitleFor('zunda')} />
      {(Object.keys(METAN_PORTRAITS) as MetanFace[]).map((pose) => (
        <Character key={pose} ref={metanPoses[pose]} id={`metan-${pose}`} name={SPEAKERS.metan.name}
          portrait={METAN_PORTRAITS[pose]} subtitle={subtitleFor('metan')} />
      ))}
    </>
  );
}

/** 二人の立ち絵。最初の台詞の少し前に、画面の外から滑り込んでくる。 */
export function Cast() {
  const frame = useCurrentFrame();
  const zundaFace = faceAt<ZundaFace>('zunda', frame, 'normal');
  const metanPose = faceAt<MetanFace>('metan', frame, 'talk');
  // 自分の最初の台詞の `lead` フレーム前から、`offscreen` px 外から滑り込む。
  const entrance = (lineId: string, lead: number, offscreen: number) => {
    const start = line(lineId).at - lead;
    return {
      shown: frame >= start,
      dx: offscreen * (1 - progress(frame, start, 20, Easings.easeOutBack)),
    };
  };
  const zundaIn = entrance('l1', 16, 620);
  const metanIn = entrance('l2', 12, -620);
  const y = (speaker: Speaker) => PLACEMENT[speaker].y - hopAt(speaker, frame) + swayAt(speaker, frame);
  return (
    <>
      {/* めたんはポーズごとに別のキャラクター（id）なので、まばたきの間隔がポーズで変わらないよう seed をそろえる */}
      <CharacterView ref={VIEW.metan} character={metanPoses[metanPose]} expression={metanPose} blink={{ seed: 'metan' }}
        mouth="closed" scale={PLACEMENT.metan.scale} x={PLACEMENT.metan.x + metanIn.dx} y={y('metan')}
        opacity={metanIn.shown ? 1 : 0} />
      <CharacterView ref={VIEW.zunda} character={zunda} expression={zundaFace}
        mouth="closed" scale={PLACEMENT.zunda.scale} x={PLACEMENT.zunda.x + zundaIn.dx} y={y('zunda')}
        opacity={zundaIn.shown ? 1 : 0} />
    </>
  );
}

/**
 * 台詞ごとの字幕・音声・口パク。字幕は次の台詞まで（間を含めて）出しておく。
 * 立ち絵が登場する前の台詞（冒頭の暗闇）も、ビューが透明なまま口だけ動く。
 */
export function DialogueLines() {
  return (
    <>
      {TIMED_LINES.map((t) => (
        <Sequence key={t.id} from={t.at} durationInFrames={t.hold}>
          <Dialogue character={VIEW[t.speaker]} audio={t.voice} lipSync={lipSyncOf(t.id)}>
            {wrapSubtitle(t.text)}
          </Dialogue>
        </Sequence>
      ))}
    </>
  );
}
