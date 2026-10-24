// 立ち絵の PSD のどのレイヤーを、どの表情・口の形で見せるか。
//
// レイヤーのパスは PSD の中の名前そのまま（フォルダ名と `/` でつなぐ）。一覧は
//   node skills/celesta/scripts/inspect.mjs --psd-layers examples/zunda/assets/zundamon.psd
// で確かめられる。

import type { PsdCharacterLipSync, PsdCharacterPortrait, PsdExpression } from '@celesta/react';

import type { MetanFace, ZundaFace } from '../../script.ts';
import { ASSET } from '../theme.ts';

/** 1 つの表情：目以外の顔のレイヤー、目（開・閉）、口パクの口。 */
type PsdFace = {
  /** 声のシーンの表で見せる名前 */
  label: string;
  /** 目以外に見せるレイヤー（眉、涙、「？」など） */
  layers: string[];
  /** まばたきで切り替える目。表情ごとに目のレイヤーが違うので、表情ごとに持つ */
  eyes: { open: string[]; closed: string[] };
  mouth: PsdCharacterLipSync;
};

/**
 * 表情の表から PSD 立ち絵を作る。`shared` はどの表情でも見せるレイヤー（体など）、
 * `defaultExpression` は表情を指定しないときの表情。
 * まばたきは Celesta に任せる（表情ごとの `blink` に目のレイヤーを渡すと、
 * フレーム番号から決まる間隔で目を閉じる）。
 */
function psdPortrait<Face extends string>(
  src: string,
  shared: string[],
  faces: Record<Face, PsdFace>,
  defaultExpression: NoInfer<Face>,
): PsdCharacterPortrait {
  const expressions: Record<string, PsdExpression> = {};
  for (const [name, face] of Object.entries<PsdFace>(faces)) {
    expressions[name] = { layers: [...face.layers, ...face.eyes.open], lipSync: face.mouth, blink: face.eyes };
  }
  return { type: 'psd', src, layers: shared, expressions, defaultExpression };
}

// ── ずんだもん（公式 PSD）────────────────────────────────────────────────
// 表情フォルダごとに目（開・閉）と、あいうえお・ん の口がある。名前は
// 「〜 のコピー 3」のように表情ごとに番号が違うので、表情ごとに書き出す。

function zundaFace(
  label: string,
  folder: string,
  eyes: { open: string; closed: string },
  mouthFolder: string,
  mouths: { closed: string; suffix: string; a?: string },
  extra: string[] = [],
): PsdFace {
  const inFolder = (name: string) => `${folder}/${name}`;
  const mouth = (name: string) => inFolder(`${mouthFolder}/${name}`);
  return {
    label,
    layers: extra.map(inFolder),
    eyes: { open: [inFolder(eyes.open)], closed: [inFolder(eyes.closed)] },
    mouth: {
      a: mouth(mouths.a ?? `あ${mouths.suffix}`),
      i: mouth(`い${mouths.suffix}`),
      u: mouth(`う${mouths.suffix}`),
      e: mouth(`え${mouths.suffix}`),
      o: mouth(`お${mouths.suffix}`),
      closed: mouth(mouths.closed),
    },
  };
}

export const ZUNDA_FACES: Record<ZundaFace, PsdFace> = {
  normal: zundaFace('ノーマル', 'ノーマル', { open: '開', closed: '閉じ2' }, '口',
    { closed: 'ん のコピー', suffix: '' }),
  aori: zundaFace('煽り', '煽り', { open: '開け目 のコピー', closed: '2 のコピー' }, '口 のコピー',
    { closed: 'ん のコピー 2', suffix: ' のコピー' }),
  tsuntsun: zundaFace('つんつん', 'つんつん', { open: '開け目 のコピー 2', closed: '2 のコピー 2' }, '口 のコピー 3',
    { closed: 'ん のコピー 4', suffix: ' のコピー 3' }),
  amaama: zundaFace('あまあま', 'あまあま', { open: '開け目 のコピー 3', closed: '2 のコピー 3' }, '口 のコピー 4',
    { closed: 'ん のコピー 5', suffix: ' のコピー 4' }),
  namida: zundaFace('涙目', '顔涙目 のコピー', { open: '開け目 のコピー 7', closed: 'とじ2' }, '口 のコピー 9',
    { closed: 'とじ のコピー 2', suffix: ' のコピー 9', a: 'あけ のコピー 2' }, ['まゆ のコピー 3']),
  herohero: zundaFace('へろへろ', '顔へろへろ のコピー', { open: '開け目 のコピー 6', closed: 'とじ2' }, '口 のコピー 8',
    { closed: 'ん のコピー 8', suffix: ' のコピー 8', a: 'あ のコピー 7' }, ['赤', 'まゆ のコピー 2']),
};

/** ずんだもんは 1 つの PSD に全部の表情が入っているので、立ち絵は 1 つ。 */
export const ZUNDA_PORTRAIT = psdPortrait(ASSET.zunda, ['ベース'], ZUNDA_FACES, 'normal');

// ── 四国めたん（公式 SD 立ち絵）─────────────────────────────────────────
// ポーズごとに別の PSD なので、ポーズごとに立ち絵を作って差し替える。
// 口は開・閉の 2 枚だけなので、あいうえお はすべて「開」に割り当てる。

function twoStateMouth(open: string, closed: string): PsdCharacterLipSync {
  return { a: open, i: open, u: open, e: open, o: open, closed };
}

const METAN_FACES: Record<MetanFace, PsdFace> = {
  talk: { label: 'おしゃべり', layers: [], eyes: { open: ['レイヤー 112 のコピー'], closed: ['せん 3'] },
    mouth: twoStateMouth('2 のコピー', '1 のコピー') },
  happy: { label: 'にっこり', layers: [], eyes: { open: ['目あけ'], closed: ['目とじ'] },
    mouth: twoStateMouth('口あけ', '口とじ') },
  worried: { label: 'しょんぼり', layers: ['涙/レイヤー 17'], eyes: { open: ['目開け'], closed: ['目閉じ'] },
    mouth: twoStateMouth('口/開け', '口/閉じ') },
  what: { label: 'はてな', layers: ['？'], eyes: { open: ['目開け'], closed: ['目閉じ'] },
    mouth: twoStateMouth('口/あけ', '口/とじ') },
};

export const METAN_PORTRAITS = Object.fromEntries(
  (Object.keys(METAN_FACES) as MetanFace[]).map((pose) => [
    pose,
    psdPortrait(ASSET.metan[pose], [], { [pose]: METAN_FACES[pose] }, pose),
  ]),
) as Record<MetanFace, PsdCharacterPortrait>;
