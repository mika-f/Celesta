// 5 · 声と口パク
// カメラがずんだもんの口元に寄り、左のパネルに「いまの口の形」「読みがな」
// 「音声の波形」「表示している PSD のレイヤー」を出す。メタンが表情の話を
// している間は、ずんだもんの表情を順に切り替えて、パネルに表情の一覧を出す。
// カメラの動きは camera.ts、表情の順番は cast/acting.ts。

import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

import { Easings, Group, Rect, Text, buildEnvelope, decodeWav, progress } from '@celesta/react';
import type { MouthShape } from '@celesta/react';

import type { ZundaFace } from '../../script.ts';
import { FACE_PARADE, faceAt } from '../cast/acting.ts';
import { lipSyncOf } from '../cast/lipsync.ts';
import { ZUNDA_FACES } from '../cast/portraits.ts';
import { line, useFilmFrame } from '../timing.ts';
import type { TimedLine } from '../timing.ts';
import { CANVAS, COLOR, FONT, FPS, solid } from '../theme.ts';
import type { SceneDefinition } from './types.ts';

const { width: W, height: H } = CANVAS;

/** カメラが寄る・引くフレーム（camera.ts と、パネルの出入りで使う）。 */
export const VOICE_CUE = {
  /** 「口、ちゃんと動いてるの気づいてた？」の言い終わりで寄る */
  zoomIn: line('s1').at + line('s1').duration - 26,
  /** 「ボクの顔で遊ばないでほしいのだ！」のあとで引く */
  zoomOut: line('s4').at + line('s4').hold - 8,
} as const;

/** 口の形を表す仮名 */
const SHAPE_KANA: Record<MouthShape, string> = { a: 'あ', i: 'い', u: 'う', e: 'え', o: 'お', closed: 'ん' };
const SHAPES: readonly MouthShape[] = ['a', 'i', 'u', 'e', 'o', 'closed'];

/** 波形を出す台詞（ずんだもんの台詞）と、その音量の包絡線。prepare() で読む。 */
const WAVEFORM_LINES = ['s2', 's4'];
const envelopes = new Map<string, Float32Array>();

async function prepare() {
  for (const id of WAVEFORM_LINES) {
    // Celesta はバンドル時に import.meta.dirname をエントリ（film.tsx）のフォルダに
    // 置き換えるので、どのモジュールからでもエントリ基準のパスで読める。
    const bytes = new Uint8Array(await readFile(join(import.meta.dirname, line(id).voice)));
    envelopes.set(id, buildEnvelope(decodeWav(bytes), FPS));
  }
}

function Stage() {
  const frame = useFilmFrame('voice');
  const COLUMNS = 14;
  const SPACING = 170;
  return (
    <>
      {/* カメラが寄っても端が見えないよう、画面より広く敷く */}
      <Rect x={-400} y={-300} width={W + 800} height={H + 600} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: H + 600 },
        stops: [{ offset: 0, color: '#F2FBE6' }, { offset: 1, color: '#BFE39A' }],
      }} />
      {/* 横に流れる水玉。行ごとに半分ずらす */}
      {Array.from({ length: COLUMNS * 7 }, (_, i) => {
        const row = Math.floor(i / COLUMNS);
        const x = (i % COLUMNS) * SPACING - SPACING + (row % 2) * (SPACING / 2) + ((frame * 0.6) % SPACING) - SPACING / 2;
        return <Rect key={i} x={x} y={row * SPACING + 40} width={26} height={26} cornerRadius={13} fill="#7CC24233" />;
      })}
    </>
  );
}

function Overlay() {
  const frame = useFilmFrame('voice');
  const shown = progress(frame, VOICE_CUE.zoomIn + 10, 16, Easings.easeOutCubic)
    * (1 - progress(frame, VOICE_CUE.zoomOut - 4, 14));
  if (shown <= 0) return null;
  const current = ['s2', 's3', 's4'].map(line).reduce((latest, t) => (t.at <= frame ? t : latest));
  return (
    <Group x={80 - 40 * (1 - shown)} y={110} opacity={shown}>
      <Rect width={660} height={700} cornerRadius={26} fill="#10241AEE" stroke="#FFFFFF22" strokeWidth={2}
        shadow={{ color: '#0000004D', blur: 24, offsetX: 0, offsetY: 12 }} />
      {current.id === 's3' ? <FaceList frame={frame} /> : <MouthMeter frame={frame} line={current} />}
    </Group>
  );
}

const panelTitle = (text: string) => (
  <Text x={36} y={64} anchorY="baseline"
    style={{ fontFamily: FONT.mono, fontSize: 24, fontWeight: 700, fill: solid(COLOR.string) }}>
    {text}
  </Text>
);

/** いまの口の形・読みがな・波形・PSD のレイヤー名。 */
function MouthMeter({ frame, line: spoken }: { frame: number; line: TimedLine }) {
  const local = Math.max(0, frame - spoken.at);
  const speaking = local < spoken.duration;
  const shape: MouthShape = speaking ? lipSyncOf(spoken.id)?.mouthAtFrame(local, FPS) ?? 'closed' : 'closed';
  const face = ZUNDA_FACES[spoken.face as ZundaFace];
  const envelope = envelopes.get(spoken.id);
  const BARS = 72;
  return (
    <>
      {panelTitle('loadLipSync({ src, text })')}
      <Text x={36} y={102} anchorY="baseline"
        style={{ fontFamily: FONT.ja, fontSize: 22, fontWeight: 500, fill: solid('#FFFFFFAA') }}>
        {`text: ${spoken.kana.replace(/['/_]/g, '')}`}
      </Text>
      {/* いまの口の形 */}
      <Group x={330} y={290}>
        <Rect width={250} height={250} cornerRadius={125} anchorX={0.5} anchorY={0.5} fill={COLOR.zunda}
          glow={{ color: '#B8FF5A88', blur: 20 }} />
        <Text anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.ja, fontSize: 150, fontWeight: 800, fill: solid('#0B2A06') }}>
          {SHAPE_KANA[shape]}
        </Text>
      </Group>
      <Group x={60} y={470}>
        {SHAPES.map((s, i) => (
          <Group key={s} x={i * 92}>
            <Rect width={76} height={56} cornerRadius={12} fill={s === shape ? COLOR.zunda : '#FFFFFF14'} />
            <Text x={38} y={28} anchorX={0.5} anchorY={0.5}
              style={{ fontFamily: FONT.ja, fontSize: 30, fontWeight: 800, fill: solid(s === shape ? '#0B2A06' : '#FFFFFF88') }}>
              {SHAPE_KANA[s]}
            </Text>
          </Group>
        ))}
      </Group>
      {/* WAV から読んだ実際の波形。再生した部分を緑にする */}
      <Group x={40} y={560}>
        {envelope && Array.from({ length: BARS }, (_, i) => {
          const level = Math.min(1, envelope[Math.floor((i / BARS) * envelope.length)] * 1.6);
          const played = (i / BARS) * spoken.duration <= local;
          return <Rect key={i} x={i * 8} y={40 - 36 * level} width={5} height={Math.max(2, 72 * level)} cornerRadius={2}
            fill={played ? COLOR.zunda : '#FFFFFF33'} />;
        })}
      </Group>
      <Text x={36} y={672} anchorY="baseline"
        style={{ fontFamily: FONT.ja, fontSize: 20, fontWeight: 500, fill: solid('#FFFFFF99') }}>
        {`PSD レイヤー: ${face.mouth[shape] ?? ''}`}
      </Text>
    </>
  );
}

/** 表情の一覧。いま見せている表情を光らせ、その目のレイヤー名を出す。 */
function FaceList({ frame }: { frame: number }) {
  const current = faceAt<ZundaFace>('zunda', frame, 'normal');
  return (
    <>
      {panelTitle("expressions: { normal, smile, … }")}
      {FACE_PARADE.map((face, i) => {
        const on = face === current;
        const { label, eyes } = ZUNDA_FACES[face];
        return (
          <Group key={face} x={40} y={110 + i * 78}>
            <Rect width={580} height={64} cornerRadius={14} fill={on ? COLOR.zunda : '#FFFFFF10'} />
            <Text x={24} y={42} anchorY="baseline"
              style={{ fontFamily: FONT.ja, fontSize: 30, fontWeight: 800, fill: solid(on ? '#0B2A06' : '#FFFFFFAA') }}>
              {label}
            </Text>
            <Text x={556} y={42} anchorX={1} anchorY="baseline"
              style={{ fontFamily: FONT.ja, fontSize: 18, fontWeight: 500, fill: solid(on ? '#0B2A06' : '#FFFFFF55') }}>
              {eyes.open[0]}
            </Text>
          </Group>
        );
      })}
    </>
  );
}

export const voiceScene: SceneDefinition = {
  id: 'voice',
  title: '声と口パク',
  wipeIn: true,
  Stage,
  Overlay,
  prepare,
  // パネルに出す仮名とレイヤー名
  strings: [
    'text: PSD レイヤー: ', ...Object.values(SHAPE_KANA), ...WAVEFORM_LINES.map((id) => line(id).kana),
    ...Object.values(ZUNDA_FACES).flatMap((face) => [face.label, ...face.eyes.open, ...Object.values(face.mouth)]),
  ],
};
