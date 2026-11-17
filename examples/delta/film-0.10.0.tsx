import { Composition, FreezeFrame } from '@celesta.studio/react';
import { FPS, H, W } from './components/theme';
import { Fonts } from './v0.10.0/fonts';
import { Captions, Chrome, POSTER_AT, Reel, Sound } from './v0.10.0/reel';
import { loadScore } from './v0.10.0/score';
import { DURATION, SCENE_LIST } from './v0.10.0/timeline';

// DELTA 0.10.0 — PATCH: what changed from Celesta v0.9.0 to v0.10.0, in 84
// seconds, as a modular rack. 0.10.0 moved what Celesta did inside itself
// out into plugins: what a layer draws, how media is read and written,
// where plugins come from. So the film is a rack: the core powers up, the
// fourteen features slide in as modules and are patched one into the next,
// and the camera goes into each in turn, cut to a 120 BPM modular techno
// track; the outro pulls back out of the core's screen and powers it down.
// Open this file in Celesta (add examples/plugins/dist in the Plugins tab
// for the two sample plugins), or export it with:
//   python3 examples/delta/make-score-0.10.0.py
//   node scripts/plugins/build.mts examples/plugins/flag examples/plugins/turntable
//   Celesta-export --react --loudness streaming --plugin-dir examples/plugins/dist \
//     examples/delta/film-0.10.0.tsx examples/delta/delta-0.10.0.mp4 \
//     --subtitles 'examples/delta/delta-0.10.0.{lang}.srt' --subtitles 'examples/delta/delta-0.10.0.{lang}.vtt'
//
//   v0.10.0/rack.tsx      the rack: sixteen slots, the core, MORE, the cables; the cameras
//   v0.10.0/module.tsx    a module's faceplate: header, LED, screen, strip, jacks; a patch cable
//   v0.10.0/open.tsx      the rack filled and patched, the title, the dive into the first module
//   v0.10.0/program.tsx   a feature patched in: headline, diff, rack map, module, caption, lower third
//   v0.10.0/chapters.tsx, demos/, More.tsx   the fourteen modules and twelve cards
//   v0.10.0/outro.tsx     the end card, the pull back, the power down
//
// Made with what is new: CONTENT PLUGINS, APPROVALS and the rack draw real
// <Flag> layers and 3D SCENES a real <Turntable>, each from its plugin's own
// process (examples/plugins); the code in every diff is highlighted by the
// registry plugin `code`, which this project locks in celesta.lock; the
// export reads and writes media through the media plugins, with the system's
// H.264 on macOS, and tags it BT.709.

/** prepare(): the score's spectrum, for the end card's meter. */
export async function prepare() {
  await loadScore();
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
      {/* The rack powered down: the film's last frame. */}
      <Composition id="poster" width={W} height={H} fps={FPS} durationInFrames={1} lang="ja-JP" fontFallback="loaded">
        <Fonts />
        <FreezeFrame frame={POSTER_AT}><Reel /></FreezeFrame>
      </Composition>
    </>
  );
}
