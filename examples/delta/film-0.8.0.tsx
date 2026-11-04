import { Audio, Composition, FreezeFrame } from '@celesta.studio/react';
import { FPS } from './components/theme';
import { PitchRiff } from './v0.8.0/demos/PitchDemo';
import { FilmContext } from './v0.8.0/film-context';
import { Captions, Chrome, DURATION, Film, Fonts, LANDSCAPE, POSTER_AT, Reel } from './v0.8.0/reel';
import { SCORE_SRC, loadScore } from './v0.8.0/score';
import { loadVoices } from './v0.8.0/voices';

// DELTA 0.8.0 — PROOF: what changed from Celesta v0.7.0 to v0.8.0, in a
// minute and a half. Every feature is a frame printed on one proof sheet;
// the camera reads the sheet tile by tile to a 90 BPM lo-fi score, tipping
// and turning the paper in 3D between them, and ends on the whole sheet.
// Open this file in Celesta, or export it with:
//   python3 examples/delta/make-score-0.8.0.py
//   Celesta-export --react examples/delta/film-0.8.0.tsx examples/delta/delta-0.8.0.mp4 \
//     --subtitles 'examples/delta/delta-0.8.0.{lang}.srt' --subtitles 'examples/delta/delta-0.8.0.{lang}.vtt'
// shorts-0.8.0.tsx is the same sheet and camera through a 9:16 window.
//
//   v0.8.0/sheet.ts    the sheet's layout and the camera's path
//   v0.8.0/tiles.tsx   what is printed: the header, the feature tiles, the MORE cards
//   v0.8.0/open.tsx    the cover page: the PRs on a tipped plane, the 0.8.0 stamp
//   v0.8.0/chapters.tsx, demos/, More.tsx   the twelve features and eight cards
//
// Made with 0.8.0: the camera is a rotationX / rotation about the screen's
// middle (originX / originY) with a perspective, the tiles are Panels on a
// PatternBackground in a <Theme> of the DELTA fonts and colors, the corners
// are a FrameChrome that names the chapter with useScene(), the 0.8.0 stamp
// is outlined three times in one layer, Japanese falls back to the loaded
// Noto Sans JP (fontFallback="loaded"), the PITCH chapter's riff is one file
// through <Audio pitch>, the VOICES chapter reads a real voice folder, and
// the loader cat is a Lottie file.

/** prepare(): the score's spectrum, and the voice folder the VOICES chapter reads. */
export async function prepare() {
  await Promise.all([loadScore(), loadVoices()]);
}

export default function Root() {
  return (
    <>
      <Composition id="film" width={LANDSCAPE.width} height={LANDSCAPE.height} fps={FPS} durationInFrames={DURATION}
        lang="ja-JP" fontFallback="loaded">
        <Fonts />
        <FilmContext.Provider value={Film}>
          <Reel {...LANDSCAPE} />
        </FilmContext.Provider>
        <Chrome {...LANDSCAPE} />
        <Captions />
        <Audio src={SCORE_SRC} />
        <PitchRiff />
      </Composition>
      {/* The whole sheet: the film's last frame. */}
      <Composition id="poster" width={LANDSCAPE.width} height={LANDSCAPE.height} fps={FPS} durationInFrames={1}
        lang="ja-JP" fontFallback="loaded">
        <Fonts />
        <FilmContext.Provider value={Film}>
          <FreezeFrame frame={POSTER_AT}><Film /></FreezeFrame>
        </FilmContext.Provider>
      </Composition>
    </>
  );
}
