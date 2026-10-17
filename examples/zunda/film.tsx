// 「動画はコードで書くのだ」— ずんだもんと四国めたんが Celesta を紹介する解説動画。
//
// 真っ黒な <Composition> から始まり、二人が話すたびに、この動画自身に
// レイヤーが一枚ずつ増えていく。このファイルは全体の組み立てだけを行い、
// 中身は src/ 以下に分けてある（構成は README.md）。
//
//   node examples/zunda/prepare-assets.ts   # 立ち絵・映像素材
//   node examples/zunda/make-voices.ts      # 音声（VOICEVOX Engine が必要）
//   python3 examples/zunda/make-score.py    # BGM
//   Celesta-export --react examples/zunda/film.tsx zunda.mp4

import { Assets, Audio, Camera, Composition, Font, Sequence, useCurrentFrame } from '@celesta/react';

import { LINES } from './script.ts';
import { cameraAt } from './src/camera.ts';
import { Cast, CastAssets, DialogueLines } from './src/cast/Cast.tsx';
import { prepareLipSync } from './src/cast/lipsync.ts';
import { SPEAKERS } from './src/cast/speakers.ts';
import { MONO_FONT_URL, japaneseFontsUrl } from './src/fonts.ts';
import { SCENES, SCENE_STRINGS, SceneOverlay, SceneStage, WIPE_CUTS, prepareScenes } from './src/scenes/index.tsx';
import { SCORE_FROM, scoreVolume } from './src/score.ts';
import { DURATION } from './src/timing.ts';
import { ASSET, CANVAS, FPS } from './src/theme.ts';
import { WorldContext } from './src/ui/FrameRecall.tsx';
import { SubtitleBand } from './src/ui/Subtitle.tsx';
import { Wipes } from './src/ui/Wipe.tsx';

const JAPANESE_FONTS = japaneseFontsUrl([
  ...LINES.map((line) => line.text),
  ...Object.values(SPEAKERS).map((speaker) => speaker.name),
  ...SCENES.map((scene) => scene.title ?? ''),
  ...SCENE_STRINGS,
]);

/** 背景と各シーン。カメラの内側と外側をまとめて、<FrameRecall> がどのフレームでも描き直せる単位。 */
function World() {
  return (
    <>
      <SceneStage />
      <SceneOverlay />
    </>
  );
}

export async function prepare() {
  await prepareLipSync();
  await prepareScenes();
}

export default function Root() {
  const camera = cameraAt(useCurrentFrame());
  return (
    <Composition width={CANVAS.width} height={CANVAS.height} fps={FPS} durationInFrames={DURATION}>
      <Assets>
        <Font src={JAPANESE_FONTS} />
        <Font src={MONO_FONT_URL} />
        <CastAssets />
      </Assets>

      <WorldContext.Provider value={World}>
        {/* 奥から：カメラの内側（背景・シーン・立ち絵）→ 画面に固定の表示 → ワイプ → 字幕 */}
        <Camera x={camera.x} y={camera.y} zoom={camera.zoom} shake={camera.shake} shakeFrequency={6}>
          <SceneStage />
          <Cast />
        </Camera>
        <SceneOverlay />
      </WorldContext.Provider>
      <Wipes at={WIPE_CUTS} />
      <SubtitleBand />
      <DialogueLines />

      <Sequence from={SCORE_FROM}>
        <Audio src={ASSET.score} volume={scoreVolume()} />
      </Sequence>
    </Composition>
  );
}
