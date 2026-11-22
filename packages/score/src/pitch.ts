/** A MIDI note number (60 is middle C, C4) or a name such as `'C4'`, `'F#3'`, `'Bb2'`. */
export type Pitch = number | string;

const STEPS: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };

/** The MIDI note number of a pitch. Throws on anything that is not one. */
export function parsePitch(pitch: Pitch, where: string): number {
  if (typeof pitch === 'number') {
    if (!Number.isFinite(pitch) || pitch < 0 || pitch > 127) {
      throw new Error(`${where}: pitch ${pitch} is not a MIDI note number from 0 to 127`);
    }
    return pitch;
  }
  const match = typeof pitch === 'string' ? /^([A-Ga-g])([#b♯♭]*)(-?\d+)$/.exec(pitch.trim()) : null;
  if (match === null) {
    throw new Error(`${where}: ${JSON.stringify(pitch)} is not a pitch; use a MIDI number or a name such as 'C4', 'F#3', 'Bb2'`);
  }
  let midi = STEPS[match[1].toLowerCase()] + 12 * (Number(match[3]) + 1);
  for (const accidental of match[2]) midi += accidental === '#' || accidental === '♯' ? 1 : -1;
  if (midi < 0 || midi > 127) {
    throw new Error(`${where}: ${pitch} is outside MIDI's range (C-1 to G9)`);
  }
  return midi;
}
