import { createRef } from 'react';
import type { AssetReference } from '@celesta/react';
import type { CharacterViewReference, PsdCharacterPortrait, PsdExpression } from '@celesta/character';
import type { SpeakerId } from './script';

// The two portraits: official 東北ずん子・ずんだもんプロジェクト PSDs, shrunk
// into assets/ by prepare-assets.ts. Both files name their face layers the
// same way (目 / 眉 / クチ / 頬, one closed and one open mouth per face), so
// one table builds every expression; only the folder prefixes differ.

export type Face = 'normal' | 'smile' | 'happy' | 'smug' | 'puzzled';

type PsdNames = {
  src: string;
  // Layers every expression shares: hair, body, the face outline.
  body: string[];
  face: string; // the folder holding 目 / 眉 / クチ / 頬
  browShadow: boolean; // ずん子 draws a shadow under each brow
  mouth: Record<'normal' | 'smile' | 'sad' | 'angry', [closed: string, open: string]>;
  eyes: { open: string[]; half: string[]; closed: string[]; happy: string[] };
};

const ZUNKO: PsdNames = {
  src: './assets/zunko.psd',
  face: 'グループ 1',
  body: [
    '後髪', '後ろの横毛', 'たすきひらひら', 'たすき左', 'たすき右', '帯りぼん', '帯りぼんひらひら', '振袖', '首',
    '耳L', '耳R', '輪郭', '鼻', '腕', '足', '下半身', '上半身', '胸当て', '横髪L', '横髪R', '前髪', 'あほげ',
  ].map((name) => `グループ 1/${name}`),
  browShadow: true,
  mouth: {
    normal: ['クチ通常', '通常開'], smile: ['クチ笑', '笑開'], sad: ['クチ哀', '哀開'], angry: ['クチ怒', '怒開'],
  },
  eyes: {
    open: ['開き/レイヤー 48 のコピー', '開き/白目', '開き/影', '開き/目', '瞳'],
    half: ['半/レイヤー 102', '半/半白目', '半/半影', '半/半目'],
    closed: ['閉じ', 'レイヤー 103'],
    happy: ['閉じ笑い', 'レイヤー 104'],
  },
};

const KIRITAN: PsdNames = {
  src: './assets/kiritan.psd',
  face: 'きりたん/頭',
  body: [
    // '砲' (the きりたん砲 behind her) is left out: it would crowd the bust shot.
    'レイヤー 1', '体/ランドセル', '体/帯左', '体/帯右', '体/帯りぼん',
    '体/左腕/肘下 のコピー 2/手 のコピー/手ぱー のコピー 2', '体/左腕/肘下 のコピー 2/レイヤー 30 のコピー', '体/左腕/肘上 のコピー 2',
    '体/右腕/肘下 のコピー/手/手ぱー のコピー', '体/右腕/肘下 のコピー/レイヤー 30 のコピー 3', '体/右腕/肘上 のコピー',
    '体/左足', '体/右足', '体/首', '体/上半身', '体/レイヤー 2',
    '頭/後髪', '頭/ついんて左', '頭/ついんて右', '頭/包丁右', '頭/包丁左', '頭/耳L', '頭/耳R', '頭/横髪L', '頭/横髪R',
    '頭/輪郭', '頭/鼻', '頭/前髪', '頭/あほげ',
  ].map((name) => `きりたん/${name}`),
  browShadow: false,
  mouth: {
    normal: ['クチ通常', 'クチ通常開き'], smile: ['クチ笑', 'クチ笑開き'], sad: ['クチ哀', 'クチ哀開き'], angry: ['クチ怒', 'クチ怒開き'],
  },
  eyes: {
    open: ['開き/レイヤー 48 のコピー', '開き/白目', '開き/瞳', '開き/目'],
    half: ['半/レイヤー 102', '半/半白目', '半/瞳 のコピー', '半/半目'],
    closed: ['閉じ'],
    happy: ['閉じ笑い'],
  },
};

// Brows, cheeks and mouth for each expression.
const FACES: Record<Face, { brow: '通常' | '笑' | '哀' | '怒'; cheek: '薄' | '濃'; mouth: keyof PsdNames['mouth']; eyes?: 'happy' }> = {
  normal: { brow: '通常', cheek: '薄', mouth: 'normal' },
  smile: { brow: '笑', cheek: '濃', mouth: 'smile' },
  happy: { brow: '笑', cheek: '濃', mouth: 'smile', eyes: 'happy' },
  smug: { brow: '怒', cheek: '薄', mouth: 'smile' },
  puzzled: { brow: '哀', cheek: '薄', mouth: 'normal' },
};

function portrait(names: PsdNames): PsdCharacterPortrait {
  const at = (path: string) => `${names.face}/${path}`;
  const eyes = (list: string[]) => list.map((name) => at(`目/${name}`));
  const expressions: Record<string, PsdExpression> = {};
  for (const [face, { brow, cheek, mouth, eyes: shut }] of Object.entries(FACES)) {
    const [closed, open] = names.mouth[mouth];
    expressions[face] = {
      layers: [
        at(`眉/眉${brow}`),
        ...(names.browShadow ? [at(`眉影/眉影${brow}`)] : []),
        at(`頬/${cheek}`),
        ...(shut ? eyes(names.eyes.happy) : []),
      ],
      // Two mouth layers per face: open for a / e / o, closed for i / u and silence.
      lipSync: {
        a: at(`クチ/${open}`), e: at(`クチ/${open}`), o: at(`クチ/${open}`),
        i: at(`クチ/${closed}`), u: at(`クチ/${closed}`), closed: at(`クチ/${closed}`),
      },
      ...(shut ? { blink: false as const } : {}),
    };
  }
  return {
    type: 'psd',
    src: names.src,
    layers: names.body,
    defaultExpression: 'normal',
    expressions,
    blink: { open: eyes(names.eyes.open), closed: eyes(names.eyes.closed), half: eyes(names.eyes.half) },
  };
}

export const CAST = {
  kiritan: {
    ref: createRef<AssetReference>(),
    view: createRef<CharacterViewReference>(),
    name: 'kiritan',
    displayName: '東北きりたん',
    color: '#C2410C', // name tag, timeline bars
    accent: '#FF9A6B', // emphasized words in the subtitle, bright against the ink outline
    portrait: portrait(KIRITAN),
  },
  zunko: {
    ref: createRef<AssetReference>(),
    view: createRef<CharacterViewReference>(),
    name: 'zunko',
    displayName: '東北ずん子',
    color: '#2F9E44',
    accent: '#8BE38A',
    portrait: portrait(ZUNKO),
  },
} satisfies Record<SpeakerId, unknown>;
