// Integer hashing shared by the random and noise functions. Every value they
// return is a pure function of the seed, so a frame renders the same in
// preview and in export.

/** A seed: a number (index) or a string such as `` `star-${i}-x` ``. */
export type Seed = number | string;

/** 2^32, to turn a 32-bit hash into `[0, 1)`. */
export const UINT32_RANGE = 4294967296;

/** Hashes a string seed to a 32-bit integer (FNV-1a). */
function hashString(seed: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < seed.length; i += 1) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Mixes a 32-bit integer into a well-distributed one (the murmur3 finalizer). */
export function mix32(value: number): number {
  let h = value >>> 0;
  h = Math.imul(h ^ (h >>> 16), 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return (h ^ (h >>> 16)) >>> 0;
}

export function seedToInt(seed: Seed, caller: string): number {
  if (typeof seed === 'string') {
    return hashString(seed);
  }
  if (!Number.isFinite(seed)) {
    throw new Error(`${caller}() requires a finite number or a string seed`);
  }
  // Keep fractional seeds distinct: 0.5 and 1.5 must not collide with 0 and 1.
  return Number.isInteger(seed) ? seed | 0 : hashString(String(seed));
}

/** An independent `[0, 1)` value for each `index` of the stream `base`. */
export function streamValue(base: number, index: number): number {
  return mix32(base ^ mix32(index)) / UINT32_RANGE;
}

export function requireFinite(caller: string, values: Record<string, number>): void {
  for (const [name, value] of Object.entries(values)) {
    if (!Number.isFinite(value)) {
      throw new Error(`${caller}() requires a finite ${name}`);
    }
  }
}
