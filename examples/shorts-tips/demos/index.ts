import type { ComponentType } from 'react';
import { PHRASE_STAGES, PhraseDemo } from './PhraseDemo';
import { SPRING_STAGES, SpringDemo } from './SpringDemo';
import { TRANSITION_STAGES, TransitionDemo } from './TransitionDemo';

// Every Tip's live demo, by the ID a variant names in its `tip` property.
// A demo draws inside the lower panel (DEMO.width × DEMO.height, origin at
// its top-left) and reads time from components/DemoClock: `stages` is how
// many line numbers the variant's `stages` property must list.
export const DEMOS = {
  spring: { stages: SPRING_STAGES, Demo: SpringDemo },
  transition: { stages: TRANSITION_STAGES, Demo: TransitionDemo },
  phrase: { stages: PHRASE_STAGES, Demo: PhraseDemo },
} satisfies Record<string, { stages: number; Demo: ComponentType }>;

export type TipId = keyof typeof DEMOS;
