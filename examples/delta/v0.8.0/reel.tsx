import { Group, Rect, Subtitles, progress, useCurrentFrame } from '@celesta.studio/react';
import { FrameChrome } from '@celesta.studio/components';
import { C } from '../components/theme';
import { CAPTIONS, CAPTION_LANGUAGES } from './captions';
import { CHAPTERS } from './chapters';
import { CARDS } from './More';
import { Fonts } from './fonts';
import { Open } from './open';
import { TILE, shotAt } from './sheet';
import { DeltaTheme } from './theme';
import { Sheet } from './tiles';
import { DURATION, FEATURES, MORE_CARDS, OPEN_LENGTH, SCENE_LIST, TITLES } from './timeline';

// The release as a proof sheet the camera reads. Every feature is a
// 1920×1080 frame printed on one sheet of paper; the camera holds on each
// for its two bars and hops to the next, tipped back and turned with the
// sheet's rotationX / rotation about the middle of the screen (originX /
// originY) and a perspective. The film (16:9) and the short (9:16) are the
// same sheet and the same camera through a different window.

if (CHAPTERS.length !== FEATURES || CARDS.length !== MORE_CARDS || CHAPTERS.some((chapter, i) => chapter.title !== TITLES[i])) {
  throw new Error('v0.8.0/timeline.ts does not match chapters.tsx and More.tsx');
}

export { DURATION, Fonts };
/** The whole sheet, laid flat at the end: the poster. */
export const POSTER_AT = DURATION - 1;

export interface Viewport { width: number; height: number; portrait: boolean }

/** The sheet through the camera, for a window of `width`×`height`. */
export function Reel({ width, height }: Viewport) {
  const frame = useCurrentFrame();
  const shot = shotAt(frame);
  // A zoom of 1 fits a tile to the window's width.
  const zoom = shot.zoom * (width / TILE.width);
  const reach = 1 + shot.tilt / 25;
  const visible = (x: number, y: number, w: number, h: number) =>
    Math.abs(x + w / 2 - shot.x) < (width / zoom) * reach / 2 + w / 2
    && Math.abs(y + h / 2 - shot.y) < (height / zoom) * reach * 1.4 / 2 + h / 2;
  return (
    <DeltaTheme>
      {/* The desk the sheet lies on. */}
      <Rect width={width} height={height} fill="#1B1C20" />
      <Group originX={width / 2} originY={height / 2} perspective={1.6 * Math.max(width, height)}
        rotationX={shot.tilt} rotation={shot.turn}>
        <Group x={width / 2 - shot.x * zoom} y={height / 2 - shot.y * zoom} scale={zoom}>
          <Sheet visible={visible} />
        </Group>
      </Group>
      {frame < OPEN_LENGTH ? <Open width={width} height={height} /> : null}
    </DeltaTheme>
  );
}

/** Corner marks, timecode, the chapter by name (useScene) and progress, over the camera (#423). */
export function Chrome({ width, height, portrait }: Viewport) {
  const frame = useCurrentFrame();
  return (
    <Group opacity={progress(frame, OPEN_LENGTH / 2, 20)}>
      <FrameChrome margin={portrait ? 72 : 44} corners color={C.paper} blendMode="difference"
        fontFamily={['JetBrains Mono', 'Noto Sans JP']} fontSize={portrait ? 22 : 18}
        title="CELESTA 0.8.0" subtitle="/ PROOF" timecode timecodeHours={false}
        chapters scenes={SCENE_LIST} formatChapter={(scene) => (scene.index === 0 || scene.index > FEATURES + 1
          ? scene.name : `${scene.index <= FEATURES ? `Δ${String(scene.index).padStart(2, '0')}` : '+'}  ${scene.name}`)}
        caption={`${width}×${height} · 30 FPS · 90 BPM`} progress />
    </Group>
  );
}

// Every language at once; --subtitles 'delta.{lang}.srt' writes one file each.
export function Captions() {
  return (
    <>
      {CAPTION_LANGUAGES.map((lang) => <Subtitles key={lang} lang={lang} cues={CAPTIONS[lang]} />)}
    </>
  );
}

/** The film at 16:9, for <FreezeFrame>s of "the film" (the EXPORTER and STILLS thumbnails). */
export function Film() {
  return <Reel width={1920} height={1080} portrait={false} />;
}

export const LANDSCAPE: Viewport = { width: 1920, height: 1080, portrait: false };
export const PORTRAIT: Viewport = { width: 1080, height: 1920, portrait: true };
