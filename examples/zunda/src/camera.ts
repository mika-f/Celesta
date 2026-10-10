// 動画全体のカメラワーク。立ち絵と背景（カメラの内側）にだけ効き、字幕や
// コードパネルは画面に固定のまま。
//
// ショットを時刻順に並べ、ショットの間を easeInOutCubic でなめらかにつなぐ。

import { Easings } from '@celesta/react';

import { PLACEMENT } from './cast/Cast.tsx';
import { VOICE_CUE } from './scenes/voice.tsx';
import { line } from './timing.ts';
import { CANVAS } from './theme.ts';

/** `at` フレームで、画面の中心に (x, y) を `zoom` 倍で映す。 */
type Shot = { at: number; x: number; y: number; zoom: number };

const WIDE = { x: CANVAS.width / 2, y: CANVAS.height / 2, zoom: 1 };
// ずんだもんの顔を右寄りに大きく映し、左を説明パネルのために空ける。
const MOUTH_CLOSE_UP = { x: PLACEMENT.zunda.face.x - 200, y: PLACEMENT.zunda.face.y + 40, zoom: 2 };
const FACE_MEDIUM = { x: PLACEMENT.zunda.face.x - 270, y: PLACEMENT.zunda.face.y + 130, zoom: 1.5 };
const ANGRY_CLOSE_UP = { x: PLACEMENT.zunda.face.x - 180, y: PLACEMENT.zunda.face.y + 40, zoom: 2.15 };

const SHOTS: Shot[] = [
  { at: 0, ...WIDE },
  { at: VOICE_CUE.zoomIn, ...WIDE },
  { at: VOICE_CUE.zoomIn + 22, ...MOUTH_CLOSE_UP },
  // メタンが表情の話を始めたら、表情が見える程度まで引く
  { at: line('s3').at + 6, ...MOUTH_CLOSE_UP },
  { at: line('s3').at + 30, ...FACE_MEDIUM },
  // 「ボクの顔で遊ばないでほしいのだ！」で詰め寄る
  { at: line('s4').at, ...FACE_MEDIUM },
  { at: line('s4').at + 10, ...ANGRY_CLOSE_UP },
  { at: VOICE_CUE.zoomOut, ...ANGRY_CLOSE_UP },
  { at: VOICE_CUE.zoomOut + 20, ...WIDE },
];

/** 画面の揺れ（px）。怒っている台詞と、判子が押された瞬間に揺らす。 */
function shakeAt(frame: number): number {
  const angry = line('s4');
  const stamped = line('a5');
  return (frame >= angry.at && frame < angry.at + angry.duration ? 7 : 0)
    + (frame >= stamped.at && frame < stamped.at + 24 ? 12 : 0);
}

/** `<Camera>` に渡す値。 */
export function cameraAt(frame: number): { x: number; y: number; zoom: number; shake: number } {
  let i = 0;
  while (i < SHOTS.length - 1 && SHOTS[i + 1].at <= frame) i++;
  const from = SHOTS[i];
  const to = SHOTS[i + 1] ?? from;
  const t = to === from ? 0 : Easings.easeInOutCubic(Math.min(1, (frame - from.at) / (to.at - from.at)));
  const mix = (a: number, b: number) => a + (b - a) * t;
  return { x: mix(from.x, to.x), y: mix(from.y, to.y), zoom: mix(from.zoom, to.zoom), shake: shakeAt(frame) };
}
