import type { TextStyle } from '@celesta.studio/react';
import { FONT, HEADER, SAFE_W } from './constants';

// The three places that set a variant's own words: how each is set, how wide
// it may wrap, and how many lines fit there. Header, Hook and Outro draw with
// these, and prepare() (plan.ts) checks each variant's text against them.
type Slot = { style: TextStyle; maxWidth: number; maxLines: number };

const heading = { fontFamily: FONT.ja, fontWeight: 900, lineBreak: 'phrase', lang: 'ja-JP' } as const;

export const SLOTS = {
  // Two lines end above the code band at y = 384.
  title: { style: { ...heading, fontSize: HEADER.titleSize, lineHeight: HEADER.titleLine }, maxWidth: SAFE_W, maxLines: 2 },
  // Two lines fit the hook's band (HOOK_BAND, 300 px).
  hook: { style: { ...heading, fontSize: 88, lineHeight: 112, align: 'center' }, maxWidth: SAFE_W - 40, maxLines: 2 },
  // Four lines stay clear of the sign at the bottom of the closing card.
  closing: { style: { ...heading, fontSize: 76, lineHeight: 104, align: 'center' }, maxWidth: SAFE_W - 40, maxLines: 4 },
} satisfies Record<string, Slot>;

export type SlotName = keyof typeof SLOTS;
