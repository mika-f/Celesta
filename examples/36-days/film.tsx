import { Assets, Audio, Composition, Font, Series } from '@celesta/react';
import { Hud } from './components/Hud';
import { FPS, H, W } from './constants';
import { SCENES, TIMING } from './timeline';

// "36 Days." A 44-second data film about how Celesta was built, drawn from
// this repository's own Git history. 120 BPM, 30 fps: one beat is 15 frames,
// one bar is 60, and every scene starts on a downbeat.
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/36-days/film.tsx 36-days.mp4
//
//   timeline.ts    the scene list (order and length in bars)
//   scenes/        one file per scene
//   components/    Label (text), Exit, Header, DotGrid, Caret, Hud (frame overlay)
//   data.ts        commit counts and crate sizes    constants.ts   canvas, timing, palette, fonts

export default function Film() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={TIMING.durationInFrames}>
      <Assets>
        <Font src="https://fonts.googleapis.com/css2?family=Archivo+Black&family=JetBrains+Mono:wght@400;700&family=Noto+Sans+JP:wght@500;700" />
      </Assets>
      <Series>
        {SCENES.map(({ name, durationInFrames, Scene }) => (
          <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
        ))}
      </Series>
      <Hud />
      <Audio src="./score.wav" />
    </Composition>
  );
}
