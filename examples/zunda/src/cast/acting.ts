// 立ち絵の演技：どの表情か、話し始めの跳ね、ゆらぎ（まばたきは立ち絵の `blink` に任せる）。
// どれもフレーム番号だけから決まる（Celesta のフレームは前後どちらからでも
// 描かれるので、前のフレームの状態を持ち越さない）。

import { cueAt } from '@celesta/react';
import { noise } from '@celesta/math';

import type { Speaker, ZundaFace } from '../../script.ts';
import { dialoguePlan, line } from '../timing.ts';

/** 声のシーンで、メタンが表情の説明をしている間（台詞 s3）にずんだもんが見せる表情の順番。 */
export const FACE_PARADE: readonly ZundaFace[] = ['amaama', 'aori', 'tsuntsun', 'namida', 'herohero', 'normal'];

/** 話者の台詞を、始まるフレームの順に並べたもの（cueAt で「いまの台詞」を引く）。 */
function cuesOf(speaker: Speaker) {
  return dialoguePlan().lines
    .filter((t) => t.line.speaker === speaker)
    .map((t) => ({ at: t.from, face: t.line.face }));
}

/**
 * `frame` での話者の表情。最後に自分が話した台詞の表情を保つ（聞いている間も
 * 表情が残る）。まだ話していなければ `initial`。
 */
export function faceAt<Face extends string>(speaker: Speaker, frame: number, initial: Face): Face {
  const parade = line('s3');
  if (speaker === 'zunda' && frame >= parade.from && frame < parade.from + parade.durationInFrames) {
    const step = Math.floor(((frame - parade.from) / parade.durationInFrames) * FACE_PARADE.length);
    return FACE_PARADE[Math.min(FACE_PARADE.length - 1, step)] as string as Face;
  }
  return (cueAt(cuesOf(speaker), frame)?.cue.face as Face | undefined) ?? initial;
}

/** 台詞の頭で小さく跳ねる高さ（px）。 */
export function hopAt(speaker: Speaker, frame: number): number {
  const HOP_FRAMES = 9;
  const speaking = cueAt(cuesOf(speaker), frame);
  return speaking && speaking.frame < HOP_FRAMES ? 18 * Math.sin((Math.PI * speaking.frame) / HOP_FRAMES) : 0;
}

/** 立ち絵が止まって見えないための、ゆっくりした上下のゆらぎ（px）。 */
export function swayAt(speaker: Speaker, frame: number): number {
  return 3 * noise(speaker === 'zunda' ? 1 : 2, frame / 40);
}
