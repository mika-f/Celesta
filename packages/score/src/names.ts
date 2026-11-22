import { DRUMS, EFFECTS, PATCHES } from './voices.js';
import type { DrumName, EffectName, PatchName } from './voices.js';

/** Every patch `program()` takes. */
export const PATCH_NAMES = Object.keys(PATCHES) as readonly PatchName[];
/** Every drum `drum()` and `drums()` take. */
export const DRUM_NAMES = Object.keys(DRUMS) as readonly DrumName[];
/** Every effect `fx()` and `ticks()` take. */
export const EFFECT_NAMES = Object.keys(EFFECTS) as readonly EffectName[];
