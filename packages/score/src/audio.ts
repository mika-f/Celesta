import * as React from 'react';

import { Audio } from '@celesta.studio/react';
import type { AudioProps } from '@celesta.studio/react';

import type { Score } from './define.js';

export interface ScoreAudioProps extends Omit<AudioProps, 'src'> {
  /** A score from `defineScore()`, prepared in the entry's `prepare()`. */
  score: Score;
}

/**
 * Plays a prepared score: an `<Audio>` of its file, taking every other
 * `<Audio>` prop (`volume`, `duckGroup`, `duck`, `fadeOut`, ...). Place it at
 * the composition's top level, so the score's frame 0 is the composition's.
 */
export function ScoreAudio({ score, ...audio }: ScoreAudioProps): React.ReactElement {
  return React.createElement(Audio, { ...audio, src: score.src });
}
