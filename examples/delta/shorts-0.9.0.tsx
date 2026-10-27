import { Composition } from '@celesta/react';
import { DebugOverlay } from '@celesta/debug';
import { FPS } from './components/theme';
import { Fonts } from './v0.9.0/fonts';
import { PORTRAIT } from './v0.9.0/layout';
import { Captions, Reel, Sound } from './v0.9.0/reel';
import { loadScore } from './v0.9.0/score';
import { ShortFrame } from './v0.9.0/ShortFrame';
import { DURATION, SCENE_LIST } from './v0.9.0/timeline';
import { loadVoices } from './v0.9.0/voices';

// DELTA 0.9.0 for Shorts, TikTok and Reels: film-0.9.0.tsx's broadcast at
// 1080×1920, on the same score, the same cuts and the same mix. The wall
// is laid out again with 9:16 PVW and PGM monitors, so the dive into PGM
// fills the portrait frame; the programs, MORE and the end card are
// stacked for 9:16, with everything meant to be read inside the
// 'vertical-social' safe area, and the edges outside it carry the
// broadcast (ShortFrame). The guides show the safe area in the preview and
// in PNGs exported with --debug-guides.
//   python3 examples/delta/make-score-0.9.0.py
//   Celesta-export --react --loudness streaming examples/delta/shorts-0.9.0.tsx examples/delta/delta-0.9.0-shorts.mp4

/** prepare(): the score's spectrum, and the voice folder the voice chapters play. */
export async function prepare() {
  await Promise.all([loadScore(), loadVoices()]);
}

export default function Root() {
  return (
    <Composition width={PORTRAIT.width} height={PORTRAIT.height} fps={FPS} durationInFrames={DURATION}
      lang="ja-JP" fontFallback="loaded" chapters={SCENE_LIST}>
      <Fonts />
      <Reel layout={PORTRAIT} />
      <ShortFrame />
      <Captions />
      <Sound />
      <DebugOverlay safeArea="vertical-social" />
    </Composition>
  );
}
