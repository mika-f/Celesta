import { Assets, Audio, Composition, Font, Sequence, frameKeyframes } from '@celesta/react';
import type { ReactElement } from 'react';
import { Character, DialogueSeries } from '@celesta/character';
import type { CharacterSubtitle } from '@celesta/character';
import { CAST } from './character';
import { Backdrop } from './components/Backdrop';
import { PanelSurface } from './components/Panel';
import { NameTag, Stage } from './components/Stage';
import { C, FONT, FPS, H, SUB, W } from './constants';
import { FONTS } from './fonts';
import { FrameScene } from './scenes/FrameScene';
import { Hook } from './scenes/Hook';
import { Outro } from './scenes/Outro';
import { PsdScene } from './scenes/PsdScene';
import { ReactScene } from './scenes/ReactScene';
import { SyncScene } from './scenes/SyncScene';
import { VoiceScene } from './scenes/VoiceScene';
import type { SceneId } from './script';
import { plan, prepareVoices } from './voice';

// "1分解説": 東北きりたん explains Celesta to 東北ずん子 in a vertical short.
// 1080×1920, 30 fps; the length follows the voices (about 45 s).
// Open this file in Celesta, or export it with:
//   Celesta-export --react examples/shorts-explainer/film.tsx shorts-explainer.mp4
//
//   script.ts          the script: speaker, subtitle, reading, face per line
//   make-voices.ts     VOICEVOX Engine → voices/<line>.wav + voices/<line>.json (AudioQuery)
//   voice.ts           prepare(): times the script from the WAVs, lip sync from the queries
//   character.ts       the two PSD portraits: expressions, mouths, blinking
//   prepare-assets.ts  fetches and shrinks the PSDs into assets/
//   scenes/            the explainer panel of each scene    components/  shared parts
//
// Layout: the explainer panel fills the top half, the subtitle sits in the
// middle, the portraits stand in the bottom half. Text stays clear of the
// Shorts / TikTok buttons on the right and the captions at the bottom.

export async function prepare() {
  await prepareVoices();
}

const SCENES: Record<SceneId, () => ReactElement> = {
  hook: Hook, react: ReactScene, frame: FrameScene, voice: VoiceScene, psd: PsdScene, sync: SyncScene, outro: Outro,
};

const SUBTITLE: CharacterSubtitle = {
  x: SUB.x, y: SUB.y, anchorX: 0.5, anchorY: 0, maxWidth: SUB.maxWidth,
  style: {
    fontFamily: FONT.ja, fontWeight: 900, fontSize: SUB.size, lineHeight: SUB.size * 1.3, align: 'center',
    lineBreak: 'phrase',
    fill: { type: 'solid', color: '#FFFFFF' },
    stroke: { paint: { type: 'solid', color: C.ink }, width: 14 },
  },
};

export default function Root() {
  const end = plan.durationInFrames;
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={end} lang="ja-JP">
      <Assets>
        {FONTS.map((src) => <Font key={src} src={src} />)}
        {Object.values(CAST).map((cast) => (
          <Character key={cast.name} ref={cast.ref} name={cast.name} displayName={cast.displayName}
            portrait={cast.portrait} subtitle={SUBTITLE} />
        ))}
      </Assets>

      <Backdrop />
      <Stage />
      <PanelSurface accent={C.yellow} />
      {plan.scenes.map((scene) => {
        const Scene = SCENES[scene.id as SceneId];
        return <Sequence key={scene.id} from={scene.from} durationInFrames={scene.durationInFrames}><Scene /></Sequence>;
      })}

      <NameTag />
      <DialogueSeries plan={plan} views={{ kiritan: CAST.kiritan.view, zunko: CAST.zunko.view }} holdThroughGap />

      {/* The score sits under the voices and fades out over the last second. */}
      <Audio src="./score.wav" volume={frameKeyframes([
        { frame: 0, value: 0.32 },
        { frame: end - FPS, value: 0.32 },
        { frame: end, value: 0, easing: 'ease-in' },
      ], { fps: FPS })} />
    </Composition>
  );
}
