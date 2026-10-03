// 立ち絵の演技：どの表情か、話し始めの跳ね、ゆらぎ（まばたきは立ち絵の `blink` に任せる）。
// どれもフレーム番号だけから決まる（Celesta のフレームは前後どちらからでも
// 描かれるので、前のフレームの状態を持ち越さない）。

import { noise } from '@celesta/react';

import type { Speaker, ZundaFace } from '../../script.ts';
import { TIMED_LINES, line } from '../timing.ts';

/** 声のシーンで、メタンが表情の説明をしている間（台詞 s3）にずんだもんが見せる表情の順番。 */
export const FACE_PARADE: readonly ZundaFace[] = ['amaama', 'aori', 'tsuntsun', 'namida', 'herohero', 'normal'];

/**
 * `frame` での話者の表情。最後に自分が話した台詞の表情を保つ（聞いている間も
 * 表情が残る）。まだ話していなければ `initial`。
 */
export function faceAt<Face extends string>(speaker: Speaker, frame: number, initial: Face): Face {
  const parade = line('s3');
  if (speaker === 'zunda' && frame >= parade.at && frame < parade.at + parade.duration) {
    const step = Math.floor(((frame - parade.at) / parade.duration) * FACE_PARADE.length);
    return FACE_PARADE[Math.min(FACE_PARADE.length - 1, step)] as string as Face;
  }
  let face = initial;
  for (const t of TIMED_LINES) {
    if (t.at > frame) break;
    if (t.speaker === speaker) face = t.face as Face;
  }
  return face;
}

/** 台詞の頭で小さく跳ねる高さ（px）。 */
export function hopAt(speaker: Speaker, frame: number): number {
  const HOP_FRAMES = 9;
  const speaking = TIMED_LINES.find((t) => t.speaker === speaker && frame >= t.at && frame < t.at + HOP_FRAMES);
  return speaking ? 18 * Math.sin((Math.PI * (frame - speaking.at)) / HOP_FRAMES) : 0;
}

/** 立ち絵が止まって見えないための、ゆっくりした上下のゆらぎ（px）。 */
export function swayAt(speaker: Speaker, frame: number): number {
  return 3 * noise(speaker === 'zunda' ? 1 : 2, frame / 40);
}
