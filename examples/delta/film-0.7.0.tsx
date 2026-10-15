import { Audio, Composition, FreezeFrame } from '@celesta/react';
import { FPS, H, W } from './components/theme';
import { FilmContext } from './v0.7.0/film-context';
import { LANDSCAPE } from './v0.7.0/layout';
import { Captions, ChapterHud, DURATION, Film, Fonts, POSTER_AT } from './v0.7.0/reel';
import { SCORE_SRC, loadScore } from './v0.7.0/score';

// DELTA 0.7.0: what changed from Celesta v0.6.0 to v0.7.0, in a minute and a
// half. Twelve features of four bars and eight one-bar cards, cut to a
// 180 BPM drum & bass score; every chapter is a release-notes page with a
// live demo of the feature drawn by the feature.
// Open this file in Celesta, or export it with:
//   python3 examples/delta/make-score-0.7.0.py
//   Celesta-export --react examples/delta/film-0.7.0.tsx examples/delta/delta-0.7.0.mp4 \
//     --subtitles 'examples/delta/delta-0.7.0.{lang}.srt' --subtitles 'examples/delta/delta-0.7.0.{lang}.vtt'
// (Japanese and English captions, as SRT and WebVTT)
// shorts-0.7.0.tsx is the same cut at 9:16.
//
//   components/   the DELTA look, shared by every release film: theme, chapter page, diff panel, shaders
//   v0.7.0/       this release: the cut (reel.tsx), the open, the chapters (chapters.tsx lists them,
//                 demos/ draws them), MORE, the outro
//
// The film is cut with the features it shows: every cut is a different
// transition (two of them defineTransition()), the open whips the PR log
// under a <MotionBlur> and flashes the chapters through a text mask, the
// card is glitched by a shader array, the HUD and the outro's ring read the
// score's spectrum, the fonts are loaded by name, the captions go out as
// SRT and WebVTT files in Japanese and English, and the entry holds a second
// composition: the poster.

/** prepare(): the score's spectrum, for everything that moves with the music. */
export async function prepare() {
  await loadScore();
}

export default function Root() {
  return (
    <>
      <Composition id="film" width={W} height={H} fps={FPS} durationInFrames={DURATION} lang="ja-JP">
        <Fonts />
        <FilmContext.Provider value={Film}>
          <Film />
        </FilmContext.Provider>
        <ChapterHud layout={LANDSCAPE} />
        <Captions />
        <Audio src={SCORE_SRC} />
      </Composition>
      {/* A still of the outro: one frame of the film, past this composition's end. */}
      <Composition id="poster" width={W} height={H} fps={FPS} durationInFrames={1} lang="ja-JP">
        <Fonts />
        <FilmContext.Provider value={Film}>
          <FreezeFrame frame={POSTER_AT}><Film /></FreezeFrame>
        </FilmContext.Provider>
      </Composition>
    </>
  );
}
