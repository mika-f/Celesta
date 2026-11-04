import { Audio, Composition } from '@celesta.studio/react';
import { DebugOverlay } from '@celesta.studio/debug';
import { FPS } from './components/theme';
import { FilmContext } from './v0.7.0/film-context';
import { PORTRAIT, SHORT } from './v0.7.0/layout';
import { Captions, ChapterHud, DURATION, Film, Fonts, Reel } from './v0.7.0/reel';
import { SCORE_SRC, loadScore } from './v0.7.0/score';
import { ShortFrame } from './v0.7.0/ShortFrame';

// DELTA 0.7.0 for Shorts, TikTok and Reels: film-0.7.0.tsx's cut at
// 1080×1920, on the same score and the same cuts. The open, the chapter
// pages, MORE and the outro are laid out again for 9:16; everything meant
// to be read stays inside the 'vertical-social' safe area, and the edges
// outside it carry motion (ShortFrame). The guides below show the safe area
// in the preview and in PNGs exported with --debug-guides.
//   python3 examples/delta/make-score-0.7.0.py
//   Celesta-export --react examples/delta/shorts-0.7.0.tsx examples/delta/delta-0.7.0-shorts.mp4

/** prepare(): the score's spectrum, for everything that moves with the music. */
export async function prepare() {
  await loadScore();
}

export default function Root() {
  return (
    <Composition width={SHORT.width} height={SHORT.height} fps={FPS} durationInFrames={DURATION} lang="ja-JP">
      <Fonts />
      {/* Replays and thumbnails redraw the 16:9 film. */}
      <FilmContext.Provider value={Film}>
        <Reel layout={PORTRAIT} />
      </FilmContext.Provider>
      <ShortFrame />
      <ChapterHud layout={PORTRAIT} />
      <Captions />
      <Audio src={SCORE_SRC} />
      <DebugOverlay safeArea="vertical-social" />
    </Composition>
  );
}
