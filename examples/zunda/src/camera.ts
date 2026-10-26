// 動画全体のカメラワーク。立ち絵と背景（カメラの内側）にだけ効き、字幕や
// コードパネルは画面に固定のまま。
//
// ショットを時刻順に並べ、ショットの間を easeInOutCubic でなめらかにつなぐ。

import { Easings, cueAt, interpolate } from '@celesta/react';

import { PLACEMENT } from './cast/Cast.tsx';
import { voiceCues } from './scenes/voice.tsx';
import { line } from './timing.ts';
import { CANVAS, CLAMP } from './theme.ts';

/** `at` フレームで、画面の中心に (x, y) を `zoom` 倍で映す。 */
type Shot = { at: number; x: number; y: number; zoom: number };

const WIDE = { x: CANVAS.width / 2, y: CANVAS.height / 2, zoom: 1 };
// ずんだもんの顔を右寄りに大きく映し、左を説明パネルのために空ける。
const MOUTH_CLOSE_UP = { x: PLACEMENT.zunda.face.x - 200, y: PLACEMENT.zunda.face.y + 40, zoom: 2 };
const FACE_MEDIUM = { x: PLACEMENT.zunda.face.x - 270, y: PLACEMENT.zunda.face.y + 130, zoom: 1.5 };
const ANGRY_CLOSE_UP = { x: PLACEMENT.zunda.face.x - 180, y: PLACEMENT.zunda.face.y + 40, zoom: 2.15 };

function shots(): Shot[] {
  const { zoomIn, zoomOut } = voiceCues();
  return [
    { at: 0, ...WIDE },
    { at: zoomIn, ...WIDE },
    { at: zoomIn + 22, ...MOUTH_CLOSE_UP },
    // メタンが表情の話を始めたら、表情が見える程度まで引く
    { at: line('s3').from + 6, ...MOUTH_CLOSE_UP },
    { at: line('s3').from + 30, ...FACE_MEDIUM },
    // 「ボクの顔で遊ばないでほしいのだ！」で詰め寄る
    { at: line('s4').from, ...FACE_MEDIUM },
    { at: line('s4').from + 10, ...ANGRY_CLOSE_UP },
    { at: zoomOut, ...ANGRY_CLOSE_UP },
    { at: zoomOut + 20, ...WIDE },
  ];
}

/** 画面の揺れ（px）。怒っている台詞と、判子が押された瞬間に揺らす。 */
function shakeAt(frame: number): number {
  const angry = line('s4');
  const stamped = line('a5');
  return (frame >= angry.from && frame < angry.from + angry.durationInFrames ? 7 : 0)
    + (frame >= stamped.from && frame < stamped.from + 24 ? 12 : 0);
}

/** `<Camera>` に渡す値。いまのショットから次のショットへ寄っていく。 */
export function cameraAt(frame: number): { x: number; y: number; zoom: number; shake: number } {
  const active = cueAt(shots(), frame);
  if (!active) return { ...WIDE, shake: 0 };
  const { cue: from, next: to } = active;
  const t = to ? interpolate(frame, [from.at, to.at], [0, 1], { ...CLAMP, easing: Easings.easeInOutCubic }) : 0;
  const mix = (a: number, b: number) => a + (b - a) * t;
  const target = to ?? from;
  return { x: mix(from.x, target.x), y: mix(from.y, target.y), zoom: mix(from.zoom, target.zoom), shake: shakeAt(frame) };
}
