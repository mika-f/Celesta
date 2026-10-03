// 解説動画の字幕：画面下の帯と名前札、字幕の文字のスタイル、改行の位置。
// 文字そのものは <Dialogue> が描く（キャラクターの `subtitle` の設定で）。
// ここではその下に敷く帯と、文字の設定を作る。

import { Group, Rect, Text, progress, useCurrentFrame } from '@celesta/react';
import type { CharacterSubtitle } from '@celesta/react';

import type { Speaker } from '../../script.ts';
import { SPEAKERS } from '../cast/speakers.ts';
import { line, lineAt } from '../timing.ts';
import { COLOR, FONT, solid } from '../theme.ts';

const BAND = { x: 400, y: 866, width: 1120, height: 196 } as const;

/** `<Character subtitle>` に渡す、話者ごとの字幕の位置とスタイル。 */
export function subtitleFor(speaker: Speaker): CharacterSubtitle {
  return {
    x: BAND.x + BAND.width / 2,
    y: 962,
    anchorX: 0.5,
    anchorY: 0.5,
    maxWidth: 1060,
    style: {
      fontFamily: FONT.ja,
      fontSize: 44,
      fontWeight: 800,
      lineHeight: 60,
      align: 'center',
      fill: solid(COLOR.white),
      stroke: { paint: solid(SPEAKERS[speaker].deep), width: 9 },
    },
  };
}

/** 字幕 1 行に入る全角文字数の目安（maxWidth 1060px ÷ 44px）。 */
const LINE_WIDTH = 24;

/** 全角を 1、半角（英数字）を 0.55 と数えた、だいたいの幅。 */
const widthOf = (text: string) => [...text].reduce((sum, char) => sum + (char.charCodeAt(0) < 0x80 ? 0.55 : 1), 0);

/**
 * 長い字幕を、句読点の位置で改行する。
 *
 * 自動の折り返しは日本語の文字の間ならどこでも折るので、「フ／レーム」のように
 * 言葉の途中で切れたり、「の。」だけが次の行に残ったりする。句読点で先に
 * 改行しておくと、読みやすい位置で切れる。2 行に収まるなら 2 行の長さが
 * いちばんそろう切れ目で切る。収まらなければ文（。！？）ごとに行を分け、
 * 1 行に収まらない文だけを読点で分ける。
 */
export function wrapSubtitle(text: string): string {
  if (widthOf(text) <= LINE_WIDTH) return text;
  const clauses = text.match(/[^、。！？]+[、。！？]*/g) ?? [text];

  let best: { lines: string; imbalance: number } | undefined;
  for (let i = 1; i < clauses.length; i++) {
    const first = widthOf(clauses.slice(0, i).join(''));
    const second = widthOf(clauses.slice(i).join(''));
    if (Math.max(first, second) > LINE_WIDTH) continue;
    const imbalance = Math.abs(first - second);
    if (!best || imbalance < best.imbalance) {
      best = { lines: `${clauses.slice(0, i).join('')}\n${clauses.slice(i).join('')}`, imbalance };
    }
  }
  if (best) return best.lines;

  const sentences = text.match(/[^。！？]+[。！？]*/g) ?? [text];
  return sentences.flatMap((sentence) => (widthOf(sentence) <= LINE_WIDTH ? [sentence] : packClauses(sentence))).join('\n');
}

/** 文を、読点で切った句を詰めて行にする。1 つの句が長すぎるときは、その句だけ自動の折り返しに任せる。 */
function packClauses(sentence: string): string[] {
  const lines: string[] = [];
  for (const clause of sentence.match(/[^、]+、?/g) ?? [sentence]) {
    const last = lines[lines.length - 1];
    if (last !== undefined && widthOf(last + clause) <= LINE_WIDTH) lines[lines.length - 1] = last + clause;
    else lines.push(clause);
  }
  return lines;
}

/** 字幕の帯と、いま話している人の名前札。最初の台詞の少し前に出て、最後の台詞のあとに消える。 */
export function SubtitleBand() {
  const frame = useCurrentFrame();
  const current = lineAt(frame);
  const first = line('v1');
  const last = line('o2');
  const visible = progress(frame, first.at - 10, 12) * (1 - progress(frame, last.at + last.hold - 6, 16));
  if (!current || visible <= 0) return null;
  const { name, color } = SPEAKERS[current.speaker];
  return (
    <Group opacity={visible}>
      <Rect x={BAND.x} y={BAND.y} width={BAND.width} height={BAND.height} cornerRadius={30}
        fill="#0B0F0CC8" stroke={color} strokeWidth={4} />
      <Rect x={BAND.x + 30} y={BAND.y - 24} width={200} height={46} cornerRadius={23} fill={color} />
      <Text x={BAND.x + 130} y={BAND.y - 1} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.ja, fontSize: 24, fontWeight: 800, fill: solid('#0B0F0C') }}>
        {name}
      </Text>
    </Group>
  );
}
