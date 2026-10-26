// 7 · AI エージェント
// ターミナルに、この動画を作ったエージェントの作業が流れる（説明のための絵）。
// 台本の行数やフレーム数は、実際の値を計算して出す。最後に「出番なし」の判子。

import { Easings, Group, Rect, Text, progress, spring, useTextMetrics } from '@celesta/react';
import type { TextStyle } from '@celesta/react';

import { LINES } from '../../script.ts';
import { dialoguePlan, filmDuration, line, partway, sceneEnd, sceneStart, useFilmFrame } from '../timing.ts';
import { CANVAS, COLOR, FONT, FPS, solid } from '../theme.ts';
import type { SceneDefinition } from './types.ts';

const { width: W, height: H } = CANVAS;

/** ターミナルの 1 行。等幅の `command` に、和文の `note` が続く（和文は等幅フォントに無いので別の Text にする）。 */
type TerminalLine = { at: number; command: string; note?: string; color: string };

function terminal(): TerminalLine[] {
  return [
    { at: sceneStart('agent') + 6, command: '$ claude', color: COLOR.white },
    { at: line('a2').from + 6, command: '> ', note: 'ずんだもんとめたんの解説動画を作って', color: COLOR.white },
    { at: partway('a2', 0.45), command: '● Read skills/celesta/SKILL.md', color: COLOR.string },
    { at: partway('a2', 0.6), command: '● Write script.ts', note: `  台本 ${LINES.length} 行`, color: COLOR.string },
    { at: partway('a2', 0.75), command: '● Write src/timing.ts', note: `  VOICEVOX で ${LINES.length} 本`, color: COLOR.string },
    { at: partway('a2', 0.9), command: '● Write film.tsx', note: `  ${dialoguePlan().scenes.length} シーン`, color: COLOR.string },
    { at: line('a3').from, command: '● Run inspect.mjs film.tsx --every 30', color: COLOR.string },
    { at: partway('a3', 0.6), command: `  ok  ${filmDuration()} frames, 0 errors`, color: COLOR.mute },
    { at: line('a4').from, command: '● Run celesta-exporter --react film.tsx', color: COLOR.string },
  ];
}
/** ターミナルに出る和文（フォントの文字の絞り込み用）。数字は ASCII なので含めなくてよい。 */
const TERMINAL_STRINGS = ['ずんだもんとめたんの解説動画を作って', '台本 行', 'VOICEVOX で 本', 'シーン'];

/** 書き出しの進捗バーが動く区間（「ぜんぶ Claude が書いたコードなの」の間）。 */
function exportSpan() {
  const { from, durationInFrames } = line('a4');
  return { from: from + 14, to: from + durationInFrames - 6 };
}

const FONT_SIZE = 24;
const ROW = 50;
/** 1 フレームに打ち込む文字数 */
const TYPING_SPEED = 2.2;

function Stage() {
  const frame = useFilmFrame('agent');
  const rows = terminal();
  const span = exportSpan();
  const total = filmDuration();
  const exported = progress(frame, span.from, span.to - span.from, Easings.easeInOutSine);
  return (
    <>
      <Rect width={W} height={H} fill="#07090D" />
      <Group x={560} y={60}>
        <TerminalWindow />
        {rows.filter((row) => frame >= row.at).map((row, i) => (
          <TerminalRow key={i} row={row} typed={Math.floor((frame - row.at) * TYPING_SPEED)} y={100 + i * ROW} />
        ))}
        {frame >= span.from && (
          <Group x={32} y={100 + rows.length * ROW - 26}>
            <Rect width={620} height={20} cornerRadius={10} fill="#FFFFFF14" />
            <Rect width={620 * exported} height={20} cornerRadius={10} fill={COLOR.zunda} />
            <Text x={640} y={18} anchorY="baseline" style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(COLOR.mute) }}>
              {`${Math.round(exported * total)}/${total}`}
            </Text>
          </Group>
        )}
        {frame >= span.to && (
          <Text x={32} y={100 + (rows.length + 1) * ROW} anchorY="baseline"
            style={{ fontFamily: FONT.mono, fontSize: FONT_SIZE, fontWeight: 700, fill: solid(COLOR.zunda) }}>
            export complete: zunda.mp4
          </Text>
        )}
      </Group>
      <Stamp frame={frame} />
    </>
  );
}

function TerminalWindow() {
  return (
    <>
      <Rect width={940} height={770} cornerRadius={20} fill="#0D1117" stroke="#FFFFFF22" strokeWidth={2}
        shadow={{ color: '#000000AA', blur: 40, offsetX: 0, offsetY: 18 }} />
      <Rect width={940} height={48} cornerRadius={20} fill="#161B22" />
      {['#FF5F57', '#FEBC2E', '#28C840'].map((color, i) => (
        <Rect key={color} x={24 + i * 24} y={18} width={13} height={13} cornerRadius={7} fill={color} />
      ))}
      <Text x={470} y={31} anchorX={0.5} anchorY="baseline" style={{ fontFamily: FONT.mono, fontSize: 16, fill: solid(COLOR.mute) }}>
        ~/celesta/examples/zunda
      </Text>
    </>
  );
}

/** `typed` 文字目まで打ち込まれた 1 行。等幅部分のあとに和文が続く。 */
function TerminalRow({ row, typed, y }: { row: TerminalLine; typed: number; y: number }) {
  const style: TextStyle = { fontFamily: FONT.mono, fontSize: FONT_SIZE, fill: solid(row.color) };
  // 和文は、打ち終えたコマンドの幅のぶん右に置く
  const commandWidth = useTextMetrics(row.command, style).width;
  const command = row.command.slice(0, typed);
  const note = row.note?.slice(0, Math.max(0, typed - row.command.length)) ?? '';
  return (
    <Group x={32} y={y}>
      <Text anchorY="baseline" style={style}>{command}</Text>
      {note && (
        <Text x={commandWidth} anchorY="baseline"
          style={{ fontFamily: FONT.ja, fontSize: FONT_SIZE, fontWeight: 500, fill: solid(COLOR.white) }}>
          {note}
        </Text>
      )}
    </Group>
  );
}

/** 「ボクの出番が奪われたのだ！」で押される判子。 */
function Stamp({ frame }: { frame: number }) {
  const stamp = spring({ frame: frame - line('a5').from, fps: FPS, config: { damping: 8 } });
  if (stamp <= 0 || frame >= sceneEnd('agent')) return null;
  return (
    <Group x={1030} y={430} rotation={-8} scale={0.4 + 0.6 * stamp} opacity={Math.min(1, stamp)}>
      <Rect width={560} height={150} cornerRadius={20} anchorX={0.5} anchorY={0.5}
        stroke={COLOR.metan} strokeWidth={10} fill="#FFFFFFEE" />
      <Text anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.title, fontSize: 84, fill: solid(COLOR.metan) }}>出番なし</Text>
    </Group>
  );
}

export const agentScene: SceneDefinition = {
  id: 'agent',
  title: 'AI エージェント',
  wipeIn: true,
  Stage,
  strings: ['出番なし', ...TERMINAL_STRINGS],
};
