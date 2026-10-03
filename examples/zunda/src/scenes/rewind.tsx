// 6 · フレームは関数
// 過去の 4 つのシーンを、録画ではなくその場で描き直してサムネイルに並べる
// （ui/FrameRecall.tsx）。台詞に合わせて前後にスクラブし、最後に 1 枚を
// 画面いっぱいに広げて、続きをそのまま再生してみせる。

import { Easings, Group, Rect, Text, progress, spring } from '@celesta/react';

import { DURATION, SCENE_AT, line, useFilmFrame } from '../timing.ts';
import { CANVAS, COLOR, FONT, FPS, solid } from '../theme.ts';
import { FrameRecall } from '../ui/FrameRecall.tsx';
import type { SceneDefinition } from './types.ts';

const { width: W, height: H } = CANVAS;

/** 呼び出す過去のフレーム（動画全体の番号）。 */
const RECALLED = [
  { scene: 'layers', frame: line('l6').at + 50 },
  { scene: 'effects', frame: line('e2').at + 70 },
  { scene: 'path', frame: line('p2').at + 40 },
  { scene: 'timeline', frame: line('t2').at + 40 },
] as const;

const THUMB_SCALE = 0.23;
const THUMB = { width: W * THUMB_SCALE, height: H * THUMB_SCALE };
/** 「つまり、さっきのシーンもいつでも呼び出せるのだ！」で広げるサムネイル */
const EXPANDED = 2;
const EXPAND_AT = line('r2').at + 34;
/** 「全部フレーム番号から計算されるの」のあたりから前後にスクラブする */
const SCRUB_AT = line('r1').at + 30;

function Stage() {
  const frame = useFilmFrame('rewind');
  const scrub = progress(frame, SCRUB_AT, 20);
  const expand = progress(frame, EXPAND_AT, 22, Easings.easeInOutCubic);
  return (
    <>
      <Rect width={W} height={H} fill={COLOR.navy} />
      <Perforations frame={frame} />
      {RECALLED.map((recalled, i) => {
        const appear = spring({ frame: frame - (SCENE_AT.rewind + 10 + i * 6), fps: FPS, config: { damping: 14 } });
        if (appear <= 0) return null;
        const expanding = i === EXPANDED && expand > 0;
        // 前後に揺らして、同じフレームには同じ絵が返ることを見せる。広げた 1 枚は広げ始めたときの
        // ずれで揺らしを止め、そのずれも広げる間に戻す（揺らしを残すと、再生が行ったり来たりする）。
        const wobbleAt = (f: number) => 24 * Math.sin((f - line('r1').at) / 11 + i) * scrub;
        const wobble = Math.round(expanding ? wobbleAt(EXPAND_AT) * (1 - expand) : wobbleAt(frame));
        const shownFrame = recalled.frame + wobble + (expanding ? frame - EXPAND_AT : 0);
        const home = { x: 580 + (i % 2) * (THUMB.width + 28), y: 96 + Math.floor(i / 2) * (THUMB.height + 64) };
        const t = expanding ? expand : 0;
        return (
          <Group key={recalled.scene} opacity={!expanding && expand > 0 ? 1 - expand : 1}>
            <Group x={home.x * (1 - t)} y={home.y * (1 - t)} scale={THUMB_SCALE + (1 - THUMB_SCALE) * t}
              opacity={Math.min(1, appear)}>
              <Rect x={-12} y={-12} width={W + 24} height={H + 24} cornerRadius={24} fill={COLOR.white} />
              <Group clip={{ width: W, height: H }}>
                <FrameRecall frame={shownFrame} />
              </Group>
            </Group>
            {!expanding && <Scrubber x={home.x} y={home.y + THUMB.height + 12} frame={shownFrame} opacity={Math.min(1, appear)} />}
          </Group>
        );
      })}
    </>
  );
}

/** フィルムの穴。左へ流れる。 */
function Perforations({ frame }: { frame: number }) {
  const SPACING = 80;
  const SPAN = W + 160;
  return (
    <>
      {Array.from({ length: 26 }, (_, i) => (
        <Group key={i} x={(((i * SPACING - frame * 3) % SPAN) + SPAN) % SPAN - SPACING}>
          <Rect y={24} width={36} height={22} cornerRadius={5} fill="#FFFFFF14" />
          <Rect y={H - 46} width={36} height={22} cornerRadius={5} fill="#FFFFFF14" />
        </Group>
      ))}
    </>
  );
}

/** サムネイルの下の、動画全体のどこかを示すバーとフレーム番号。 */
function Scrubber({ x, y, frame, opacity }: { x: number; y: number; frame: number; opacity: number }) {
  return (
    <Group x={x} y={y} opacity={opacity}>
      <Rect width={THUMB.width} height={6} cornerRadius={3} fill="#FFFFFF22" />
      <Rect width={THUMB.width * (frame / DURATION)} height={6} cornerRadius={3} fill={COLOR.zunda} />
      <Text y={36} anchorY="baseline" style={{ fontFamily: FONT.mono, fontSize: 18, fontWeight: 500, fill: solid('#FFFFFFAA') }}>
        {`frame ${String(frame).padStart(4, '0')}`}
      </Text>
    </Group>
  );
}

export const rewindScene: SceneDefinition = { id: 'rewind', title: 'フレームは関数', wipeIn: true, Stage };
