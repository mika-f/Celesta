import { defineScore } from '@celesta-plugin/score';

import { BAR, BEAT, FPS, GRID, TYPED, TYPED_END, WORDS } from './constants';
import { SCENES, sceneStart } from './timeline';

// The whole soundtrack, written against the same timeline the scenes use:
// a tick on every typed character, a reversed whoosh into every cut, a
// groove under Beat, and a chord and a pop per word in Outro. Nothing here
// is a file; prepare() renders it to .celesta/score/<hash>.wav.
export const score = defineScore({ fps: FPS, durationInFrames: SCENES.durationInFrames }, s => {
  s.tempo(GRID);
  s.master({ gain: 0.6, fadeOut: 1.2 });
  s.program('keys', 'ep', { gain: 0.5, reverb: 0.3 });
  s.program('mallet', 'marimba', { gain: 0.55, pan: -0.25, bright: 0.7 });
  s.program('bass', 'bass', { gain: 0.6 });
  s.program('pad', 'pad', { gain: 0.35, reverb: 0.5 });
  s.program('bell', 'glock', { gain: 0.35, pan: 0.3, reverb: 0.4 });

  // Type: the keys, then a ding when the command is in.
  s.ticks(TYPED, { vel: 0.55, vary: 0.6 });
  s.fx('ding', { frame: TYPED_END + 6, vel: 0.5 });
  s.chord('pad', ['D3', 'A3', 'F4'], { frame: 0, frames: 2 * BAR });

  // Into every later scene: a whoosh that lands on its first frame, and a thud there.
  for (const scene of SCENES.scenes.slice(1)) {
    s.fx('whoosh', { frame: scene.from, reverse: true, dur: 0.7, vel: 0.6 });
    s.fx('thud', { frame: scene.from, vel: 0.7 });
  }

  // Beat: two bars of groove (each line is one bar, so repeat 2), and the
  // squares bump on the same beats (useBeat(GRID)).
  const beat = sceneStart('beat');
  const from = { frame: beat };
  s.drums({ kick: 'x...x...', hat: '.x.x.x.X', clap: '....x...' }, { ...from, repeat: 2 });
  s.pattern('bass', 'x.xx.x.x', { ...from, repeat: 2, pitch: ['D2', 'D2', 'F2', 'A1', 'C2'] });
  s.pattern('mallet', 'x.x.x.xX', { ...from, repeat: 2, pitch: ['D5', 'F5', 'A5', 'C6', 'A5', 'F5', 'E5', 'D5'], vel: 0.6 });
  s.chord('keys', ['D4', 'F4', 'A4', 'C5'], { ...from, frames: BAR });
  s.chord('keys', ['Bb3', 'D4', 'F4', 'A4'], { frame: beat + BAR, frames: BAR });

  // Outro: a pop and a bell per word, then the chord rings out under the fade.
  const outro = sceneStart('outro');
  WORDS.forEach((_, i) => {
    const at = outro + (i + 1) * BEAT;
    s.fx('pop', { frame: at, vel: 0.6, pitch: i * 2 });
    s.note('bell', ['D6', 'F6', 'A6'][i], { frame: at, frames: 30 });
  });
  s.chord('pad', ['D3', 'A3', 'E4', 'F4'], { frame: outro, frames: 2 * BAR });
  s.fx('chime', { frame: outro + 4 * BEAT, vel: 0.45 });
});
