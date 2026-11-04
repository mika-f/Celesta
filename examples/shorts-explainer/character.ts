import { createRef } from 'react';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterViewReference, PsdCharacterPortrait, PsdExpression } from '@celesta.studio/character';
import type { Face, SpeakerId } from './script';

// The two portraits: official 東北ずん子・ずんだもんプロジェクト PSDs, shrunk
// into assets/ by prepare-assets.ts. Both files name their face layers the
// same way (目 / 眉 / クチ / 頬, one closed and one open mouth per face), so
// one table builds every expression; only the folder prefixes differ.
//
// Each PSD is saved with the whole body shown and one variant shown in each
// face folder, so the portraits start from that saved pose: an expression
// hides the face folders it changes and shows its own variant in each.

type PsdNames = {
  src: string;
  // Layers or folders the saved pose shows that the portrait leaves out.
  hide?: string[];
  face: string; // the folder holding 目 / 眉 / クチ / 頬
  browShadow: boolean; // ずん子 draws a shadow under each brow
  mouth: Record<'normal' | 'smile' | 'sad' | 'angry', [closed: string, open: string]>;
  eyes: { open: string[]; half: string[]; closed: string[]; happy: string[] };
};

const ZUNKO: PsdNames = {
  src: './assets/zunko.psd',
  face: 'グループ 1',
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
  // The きりたん砲 behind her would crowd the bust shot.
  hide: ['きりたん/砲'],
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
      // The saved brow, cheek, and mouth (and, for shut eyes, the open eyes)
      // give way to this face's own.
      hide: [
        at('眉'), ...(names.browShadow ? [at('眉影')] : []), at('頬'), at('クチ'),
        ...(shut ? [at('目')] : []),
      ],
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
    ...(names.hide ? { hide: names.hide } : {}),
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
