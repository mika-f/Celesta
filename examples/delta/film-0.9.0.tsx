import { Composition, FreezeFrame } from '@celesta/react';
import { FPS, H, W } from './components/theme';
import { Fonts } from './v0.9.0/fonts';
import { Captions, Chrome, POSTER_AT, Reel, Sound } from './v0.9.0/reel';
import { loadScore } from './v0.9.0/score';
import { DURATION, SCENE_LIST } from './v0.9.0/timeline';
import { loadVoices } from './v0.9.0/voices';

// DELTA 0.9.0 — ON AIR: what changed from Celesta v0.8.0 to main, in 91
// seconds, as a broadcast. A control room's multiviewer lights up, the
// twelve features are its sources, and the camera dives into the program
// monitor; each feature goes to air with a lower third and its caption lit
// word by word, cut to the next by a stinger on the downbeat of a 100 BPM
// newsroom theme; the outro pulls back out to the wall and goes off air.
// Open this file in Celesta, or export it with:
//   python3 examples/delta/make-score-0.9.0.py
//   Celesta-export --react --loudness streaming examples/delta/film-0.9.0.tsx examples/delta/delta-0.9.0.mp4 \
//     --subtitles 'examples/delta/delta-0.9.0.{lang}.srt' --subtitles 'examples/delta/delta-0.9.0.{lang}.vtt'
//
//   v0.9.0/multiview.tsx  the wall: PVW, PGM, the twelve sources, the ticker; the dive
//   v0.9.0/open.tsx       the monitors light, the countdown, TAKE, the dive into PGM
//   v0.9.0/program.tsx    a feature on air: headline, diff, monitor, caption, lower third
//   v0.9.0/chapters.tsx, demos/, More.tsx   the twelve features and twelve cards
//   v0.9.0/outro.tsx      the end card, the pull back, OFF AIR
//   v0.9.0/mix.ts, voices.ts   the bed's fades and ducking, the three voice lines
//
// Made with what is new: every chapter's name is a LowerThird and its
// caption a CaptionTrack lit word by word; the score fades in and out with
// fadeIn / fadeOut and ducks 12 dB under three real voice lines with duck,
// one of them a dialogue line with spokenText and subtitle pages; the
// shapes take Theme color names; the preview lists the chapters below.

/** prepare(): the score's spectrum, and the voice folder the voice chapters play. */
export async function prepare() {
  await Promise.all([loadScore(), loadVoices()]);
}

export default function Root() {
  return (
    <>
      <Composition id="film" width={W} height={H} fps={FPS} durationInFrames={DURATION}
        lang="ja-JP" fontFallback="loaded" chapters={SCENE_LIST}>
        <Fonts />
        <Reel />
        <Chrome />
        <Captions />
        <Sound />
      </Composition>
      {/* The wall off air: the film's last frame. */}
      <Composition id="poster" width={W} height={H} fps={FPS} durationInFrames={1} lang="ja-JP" fontFallback="loaded">
        <Fonts />
        <FreezeFrame frame={POSTER_AT}><Reel /></FreezeFrame>
      </Composition>
    </>
  );
}
