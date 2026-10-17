// 4 · 動画とタイムライン
// トラックにクリップが落ちてきて、上のモニターで実写の映像が流れ始める。
// モニターの <Video> と下帯は本物。トラックの表示は説明のための絵だが、クリップの位置と
// 再生位置は同じ秒数から計算するので、モニターに映るものはトラックの表示と一致する。

import { Easings, Group, Rect, Sequence, Text, Video, interpolate, progress, spring, useCurrentFrame } from '@celesta/react';
import type { ReactNode } from 'react';

import { SCENE_AT, line, partway, useFilmFrame } from '../timing.ts';
import { ASSET, CANVAS, CLAMP, COLOR, FONT, FPS, alpha, solid } from '../theme.ts';
import { CodePanel } from '../ui/CodePanel.tsx';
import type { SceneDefinition } from './types.ts';

const { width: W, height: H } = CANVAS;

/** 「JSON のタイムラインも使えるわ」の途中で、クリップがトラックに落ちる */
const DROP_AT = partway('t1', 0.55);
/** 落ちて少し跳ねてから再生が始まる（トラックの 0 秒） */
const PLAY_AT = DROP_AT + 8;
/** トラックの目盛りの長さ（秒） */
const TRACK_SECONDS = 8;
/** トラック上の秒が、動画全体の何フレーム目か */
const frameAtSecond = (seconds: number) => PLAY_AT + Math.round(seconds * FPS);

const MONITOR = { x: 700, y: 64, width: 800, height: 450 } as const;
const TRACKS = { x: 700, y: 548, width: 800, labelWidth: 60 } as const;
/** 映像素材（field.mp4）の幅。モニターの幅に縮めて映す。 */
const CLIP_WIDTH = 1280;

/** トラックの行。`from`・`to` はトラック上の秒。V1 はコードパネルの JSON と同じ 6 秒。 */
const ROWS = [
  { name: 'V1', label: 'field.mp4', color: COLOR.zunda, from: 0, to: 6, drops: true },
  { name: 'A1', label: 'voices/t1.wav · t2.wav', color: COLOR.metan, from: 0.2, to: 7.4 },
  { name: 'A2', label: 'score.wav', color: '#8C7CF0', from: 0, to: 8 },
  { name: 'T1', label: 'LowerThird', color: '#F2A93B', from: 1.6, to: 4.8 },
] as const;
const [VIDEO_ROW, , , TITLE_ROW] = ROWS;

function Stage() {
  const frame = useFilmFrame('timeline');
  return (
    <>
      <Rect width={W} height={H} fill="#12161B" />
      {Array.from({ length: 30 }, (_, i) => <Rect key={i} x={i * 64} width={1} height={H} fill="#FFFFFF08" />)}
      <Monitor playing={frame >= frameAtSecond(VIDEO_ROW.from) && frame < frameAtSecond(VIDEO_ROW.to)} />
      <Tracks frame={frame} />
    </>
  );
}

function Monitor({ playing }: { playing: boolean }) {
  return (
    <Group x={MONITOR.x} y={MONITOR.y}>
      <Rect x={-10} y={-10} width={MONITOR.width + 20} height={MONITOR.height + 20} cornerRadius={20}
        fill="#000000" stroke="#FFFFFF22" strokeWidth={2} />
      <Group clip={{ width: MONITOR.width, height: MONITOR.height, cornerRadius: 12 }}>
        <Rect width={MONITOR.width} height={MONITOR.height} fill="#0A0C0F" />
        {!playing && (
          <Text x={MONITOR.width / 2} y={MONITOR.height / 2} anchorX={0.5} anchorY={0.5}
            style={{ fontFamily: FONT.mono, fontSize: 24, fill: solid('#FFFFFF55') }}>
            no clip
          </Text>
        )}
        {/* トラックの V1・T1 と同じ区間だけ映す。シーンの Sequence の中なので、開始はシーンの先頭からのフレームで書く */}
        <ClipSequence row={VIDEO_ROW}>
          <Video src={ASSET.clip} startFrom={2} scale={MONITOR.width / CLIP_WIDTH} />
        </ClipSequence>
        <ClipSequence row={TITLE_ROW}>
          <LowerThird />
        </ClipSequence>
      </Group>
    </Group>
  );
}

/** トラックの行と同じ区間だけ、子を表示する。 */
function ClipSequence({ row, children }: { row: { from: number; to: number }; children: ReactNode }) {
  const from = frameAtSecond(row.from);
  return (
    <Sequence from={from - SCENE_AT.timeline} durationInFrames={frameAtSecond(row.to) - from}>
      {children}
    </Sequence>
  );
}

function Tracks({ frame }: { frame: number }) {
  const drop = spring({ frame: frame - DROP_AT, fps: FPS, config: { damping: 13 } });
  const laneWidth = TRACKS.width - TRACKS.labelWidth - 10;
  /** トラック上の秒の x 座標（レーンの左端から） */
  const xAt = (seconds: number) => laneWidth * (seconds / TRACK_SECONDS);
  const head = interpolate(frame, [PLAY_AT, frameAtSecond(TRACK_SECONDS)], [0, TRACK_SECONDS], CLAMP);
  return (
    <Group x={TRACKS.x} y={TRACKS.y}>
      <Rect x={-10} y={-10} width={TRACKS.width + 20} height={290} cornerRadius={16}
        fill="#1A1F26" stroke="#FFFFFF1A" strokeWidth={2} />
      {/* 目盛り */}
      {Array.from({ length: TRACK_SECONDS + 1 }, (_, i) => (
        <Group key={i} x={TRACKS.labelWidth + xAt(i)}>
          <Rect width={2} height={10} fill="#FFFFFF40" />
          <Text x={6} y={10} anchorY="baseline" style={{ fontFamily: FONT.mono, fontSize: 13, fill: solid('#FFFFFF60') }}>
            {`00:0${i}`}
          </Text>
        </Group>
      ))}
      {ROWS.map((row, i) => {
        const dropping = 'drops' in row;
        return (
          <Group key={row.name} y={28 + i * 62}>
            <Text x={8} y={34} anchorY="baseline"
              style={{ fontFamily: FONT.mono, fontSize: 18, fontWeight: 700, fill: solid(COLOR.mute) }}>
              {row.name}
            </Text>
            <Rect x={TRACKS.labelWidth} width={laneWidth} height={50} cornerRadius={8} fill="#FFFFFF08" />
            {(!dropping || frame >= DROP_AT) && (
              <Group x={TRACKS.labelWidth + xAt(row.from)} y={dropping ? (1 - drop) * -260 : 0}>
                <Rect width={xAt(row.to) - xAt(row.from)} height={50} cornerRadius={8}
                  fill={alpha(row.color, 0.85)} stroke="#FFFFFF55" strokeWidth={2} />
                <Text x={14} y={32} anchorY="baseline"
                  style={{ fontFamily: FONT.mono, fontSize: 17, fontWeight: 700, fill: solid('#0B0F0C') }}>
                  {row.label}
                </Text>
              </Group>
            )}
          </Group>
        );
      })}
      {frame >= PLAY_AT && <Rect x={TRACKS.labelWidth + xAt(head)} y={14} width={3} height={262} fill="#FF5A5A" />}
    </Group>
  );
}

/**
 * 映像に重ねる下帯。props だけで見た目が決まる React コンポーネントにしておけば、
 * registerComponent() で JSON のタイムラインからも使える。
 */
function LowerThird() {
  const reveal = progress(useCurrentFrame(), 10, 18, Easings.easeOutExpo);
  return (
    <Group x={24} y={MONITOR.height - 82} opacity={reveal}>
      <Group clip={{ width: 420 * reveal, height: 58 }}>
        <Rect width={420} height={58} cornerRadius={8} fill="#0B0F0CCC" />
        <Rect width={8} height={58} fill={COLOR.zunda} />
        <Text x={24} y={38} anchorY="baseline"
          style={{ fontFamily: FONT.ja, fontSize: 26, fontWeight: 800, fill: solid(COLOR.white) }}>
          どこかの畑（イメージ）
        </Text>
      </Group>
    </Group>
  );
}

function Overlay() {
  const frame = useFilmFrame('timeline');
  return (
    <CodePanel frame={frame} title="project.celesta.json" width={620} maxRows={10} steps={[{ at: line('t1').at + 6, lines: [
      '{ "id": "broll",',
      '  "range": {',
      '    "start": { "value": 2, "timescale": 1 },',
      '    "duration": { "value": 6, "timescale": 1 }',
      '  },',
      '  "content": {',
      '    "type": "video", "asset": "field"',
      '  } }',
    ] }]} />
  );
}

export const timelineScene: SceneDefinition = {
  id: 'timeline',
  title: '動画とタイムライン',
  wipeIn: true,
  Stage,
  Overlay,
  strings: ['どこかの畑（イメージ）'],
};
