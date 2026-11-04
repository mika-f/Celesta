import { Audio, Composition } from '@celesta.studio/react';
import { DebugOverlay } from '@celesta.studio/debug';
import { FPS } from './components/theme';
import { PitchRiff } from './v0.8.0/demos/PitchDemo';
import { FilmContext } from './v0.8.0/film-context';
import { Captions, Chrome, DURATION, Film, Fonts, PORTRAIT, Reel } from './v0.8.0/reel';
import { SCORE_SRC, loadScore } from './v0.8.0/score';
import { loadVoices } from './v0.8.0/voices';

// DELTA 0.8.0 for Shorts, TikTok and Reels: film-0.8.0.tsx's sheet and
// camera through a 1080×1920 window. A tile fits the window's width, so the
// tiles above and below stay in view as the camera reads the sheet.
//   python3 examples/delta/make-score-0.8.0.py
//   Celesta-export --react examples/delta/shorts-0.8.0.tsx examples/delta/delta-0.8.0-shorts.mp4

/** prepare(): the score's spectrum, and the voice folder the VOICES chapter reads. */
export async function prepare() {
  await Promise.all([loadScore(), loadVoices()]);
}

export default function Root() {
  return (
    <Composition width={PORTRAIT.width} height={PORTRAIT.height} fps={FPS} durationInFrames={DURATION} lang="ja-JP" fontFallback="loaded">
      <Fonts />
      {/* The EXPORTER and STILLS thumbnails redraw the 16:9 film. */}
      <FilmContext.Provider value={Film}>
        <Reel {...PORTRAIT} />
      </FilmContext.Provider>
      <Chrome {...PORTRAIT} />
      <Captions />
      <Audio src={SCORE_SRC} />
      <PitchRiff />
      <DebugOverlay safeArea="vertical-social" />
    </Composition>
  );
}
