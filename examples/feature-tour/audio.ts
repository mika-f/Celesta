import { CHAPTER_AT } from './chapters';
import { FPS } from './constants';
import { VOICE_AT, voiceSeconds } from './voice';

// The score ducks under the voice line. Called from Root, after prepare()
// has measured the recording.
export function scoreVolume() {
  const at = (CHAPTER_AT[5] + VOICE_AT) / FPS;
  const key = (seconds: number, value: number, easing?: 'ease-in' | 'ease-out') =>
    ({ time: { value: Math.round(seconds * 100), timescale: 100 }, value, easing });
  return {
    type: 'keyframes' as const,
    keyframes: [key(at - 0.4, 1), key(at, 0.3, 'ease-out'), key(at + voiceSeconds, 0.3), key(at + voiceSeconds + 0.8, 1, 'ease-in')],
  };
}
