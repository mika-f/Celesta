// 「動画はコードで書くのだ」— ずんだもんと四国めたんが Celesta を紹介する解説動画。
//
// 真っ黒な <Composition> から始まり、二人が話すたびに、この動画自身に
// レイヤーが一枚ずつ増えていく。このファイルは全体の組み立てだけを行い、
// 中身は src/ 以下に分けてある（構成は README.md）。
//
//   node examples/zunda/prepare-assets.ts   # 立ち絵・映像素材
//   python3 examples/zunda/make-score.py    # BGM
//   Celesta-export --react examples/zunda/film.tsx zunda.mp4
//
// 音声は prepare() が VOICEVOX Engine で作る（初回だけ Engine が必要。README.md）。

import { Assets, Audio, Composition, Font, Sequence, useCurrentFrame } from '@celesta/react';
import { Camera } from '@celesta/layout';

import { LINES } from './script.ts';
import { cameraAt } from './src/camera.ts';
import { Cast, CastAssets, DialogueLines } from './src/cast/Cast.tsx';
import { SPEAKERS } from './src/cast/speakers.ts';
import { MONO_FONT_URL, japaneseFontUrls } from './src/fonts.ts';
import { SCENES, SCENE_STRINGS, SceneOverlay, SceneStage, chapters, prepareScenes, wipeCuts } from './src/scenes/index.tsx';
import { filmDuration, prepareTiming, sceneStart } from './src/timing.ts';
import { ASSET, CANVAS, FPS } from './src/theme.ts';
import { WorldContext } from './src/ui/FrameRecall.tsx';
import { Wipes } from './src/ui/Wipe.tsx';

const JAPANESE_FONTS = japaneseFontUrls([
  ...LINES.map((line) => line.text),
  ...Object.values(SPEAKERS).map((speaker) => speaker.name),
  ...SCENES.map((scene) => scene.title ?? ''),
  ...SCENE_STRINGS,
]);

export async function prepare() {
  await prepareTiming();
  await prepareScenes();
}

/**
 * 字幕より奥に映るもの。<FrameRecall> がこれを好きなフレームで描き直す（字幕は
 * いまの字幕と重なるので入れない）。奥から：カメラの内側（背景・シーン・立ち絵）→
 * 画面に固定の表示 → ワイプ。
 */
function Film() {
  const camera = cameraAt(useCurrentFrame());
  return (
    <>
      <Camera x={camera.x} y={camera.y} zoom={camera.zoom} shake={camera.shake} shakeFrequency={6}>
        <SceneStage />
        <Cast />
      </Camera>
      <SceneOverlay />
      <Wipes at={wipeCuts()} />
    </>
  );
}

export default function Root() {
  return (
    <Composition width={CANVAS.width} height={CANVAS.height} fps={FPS} durationInFrames={filmDuration()}
      chapters={chapters()} lang="ja-JP">
      <Assets>
        {JAPANESE_FONTS.map((src) => <Font key={src} src={src} />)}
        <Font src={MONO_FONT_URL} />
        <CastAssets />
      </Assets>

      <WorldContext.Provider value={Film}>
        <Film />
      </WorldContext.Provider>
      <DialogueLines />

      {/* BGM は二人が登場してから。台詞の間は自動で下げ（ducking）、最後は消えていく */}
      <Sequence from={sceneStart('layers')}>
        <Audio src={ASSET.score} volume={0.55} fadeIn={0.4} fadeOut={1.5} duck={{ under: 'voice', to: 0.55 }} />
      </Sequence>
    </Composition>
  );
}
