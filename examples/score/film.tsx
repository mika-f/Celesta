import { Composition, Scenes } from '@celesta.studio/react';
import { ScoreAudio } from '@celesta-plugin/score';

import { score } from './score';
import { FPS, HEIGHT, WIDTH } from './constants';
import { SCENES } from './timeline';

// "Score": twelve seconds whose every sound is written as data in score.ts
// and synthesized by @celesta-plugin/score: no audio files. The ticks
// follow typewriterFrames(), the whooshes land on defineScenes() starts,
// and the music sits on the beat grid useBeat() reads.
//   Celesta-export --react examples/score/film.tsx score.mp4 --plugin-dir plugins/dist

export async function prepare() {
  await score.prepare();
}

export default function Film() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={SCENES.durationInFrames}>
      <Scenes scenes={SCENES} />
      <ScoreAudio score={score} />
    </Composition>
  );
}
