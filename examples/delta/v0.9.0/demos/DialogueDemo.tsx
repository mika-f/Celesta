import { createRef } from 'react';
import { Group, Image, Rect, Sequence, Text } from '@celesta/react';
import { Character, CharacterView, DialogueSeries } from '@celesta/character';
import type { CharacterViewReference } from '@celesta/character';
import type { AssetReference } from '@celesta/react';
import { C, FONT, solid } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { VOICE_DELAY } from '../timeline';
import { DIALOGUE_EMPHASIS, DIALOGUE_PAGES, SHIZUKU, dialoguePlan, voices } from '../voices';
import { mono } from './common';

// DIALOGUE: voice-2 as a real dialogue line. The subtitle shows `text`, a
// page at a time (subtitlePages, #453) with "同じ名前" emphasized; the mouth
// follows `spokenText`, the kana the recording says (#452), through
// waveform lip sync. The voice is in the duck group "voice" by default, so
// the score dips under it (#466). On the multiviewer the demo draws the
// portrait without the line: one DialogueSeries plays the voice.

const shizuku = createRef<AssetReference>();
const view = createRef<CharacterViewReference>();
const PORTRAIT = { x: 70, y: 70, scale: 0.95 };
const SUBTITLE = { x: 250, y: 500 };
const SUBTITLE_STYLE = {
  fontFamily: FONT.sans, fontSize: 34, fontWeight: 700, align: 'center', lineBreak: 'phrase',
  fill: solid(C.paper), stroke: [{ paint: solid(C.ink), width: 8 }],
} as const;

/** The character: declared once, in the film's <Assets>. */
export function Shizuku() {
  return (
    <Character ref={shizuku} name={SHIZUKU.name}
      portrait={{
        defaultExpression: 'normal',
        expressions: { normal: SHIZUKU.normal, smile: SHIZUKU.smile },
        lipSync: { a: SHIZUKU.mouth('a'), i: SHIZUKU.mouth('i'), u: SHIZUKU.mouth('u'), e: SHIZUKU.mouth('e'), o: SHIZUKU.mouth('o'), closed: SHIZUKU.mouth('closed') },
      }}
      subtitle={{
        x: SUBTITLE.x, y: SUBTITLE.y, anchorX: 0.5, anchorY: 0.5, maxWidth: 440, style: SUBTITLE_STYLE,
        emphasisStyles: { key: { color: C.orange } },
      }} />
  );
}

export function DialogueDemo({ frame, live }: DemoProps) {
  const plan = dialoguePlan();
  const planned = plan.lines[0];
  const pages = planned.subtitlePages ?? [];
  const local = frame - VOICE_DELAY;
  const page = pages.findIndex((p) => local >= p.from && local < p.end);
  const line = voices().dialogue.line;
  const rows: [string, string][] = [['text', line.text], ['spokenText', line.spokenText ?? '']];
  const emphasis = DIALOGUE_EMPHASIS[0];
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill="#1B2430" />
      <Rect y={430} width={500} height={STAGE.height - 430} fill="#0C0D0F" opacity={0.35} />
      {live ? (
        <>
          <CharacterView ref={view} character={shizuku} x={PORTRAIT.x} y={PORTRAIT.y} scale={PORTRAIT.scale} mouth="closed" />
          {/* The pages go to the Japanese subtitle file, beside the chapter's caption. */}
          <Sequence from={VOICE_DELAY - planned.from} lang="ja">
            <DialogueSeries plan={plan} views={{ shizuku: view }} />
          </Sequence>
        </>
      ) : (
        <Group x={PORTRAIT.x} y={PORTRAIT.y} scale={PORTRAIT.scale}>
          <Image src={SHIZUKU.normal} width={360} height={400} />
          <Image src={SHIZUKU.mouth('closed')} width={360} height={400} />
        </Group>
      )}
      {!live ? (
        <Text x={SUBTITLE.x} y={SUBTITLE.y} anchorX={0.5} anchorY={0.5} maxWidth={440} style={SUBTITLE_STYLE}>{DIALOGUE_PAGES[0]}</Text>
      ) : null}

      <Group x={540} y={60}>
        {rows.map(([key, value], i) => (
          <Group key={key} y={i * 96}>
            <Text style={mono(17, i === 0 ? C.orange : C.blue, 700)}>{key}</Text>
            <Text y={28} maxWidth={420} style={{ fontFamily: FONT.sans, fontSize: 21, fontWeight: 500, lineHeight: 30, lineBreak: 'phrase', fill: solid(C.paper) }}>{value}</Text>
          </Group>
        ))}
        <Group y={204}>
          <Text style={mono(17, C.grey, 700)}>subtitlePages</Text>
          {DIALOGUE_PAGES.map((text, i) => {
            const p = pages[i];
            const on = i === page;
            return (
              <Group key={i} y={32 + i * 56}>
                <Rect width={420} height={46} cornerRadius={4} fill={on ? 'orange' : 'raised'} stroke={on ? 'orange' : 'line'} strokeWidth={2} />
                <Text x={14} y={23} anchorY={0.5} style={mono(15, on ? C.ink : C.dim, 700)}>{p ? `${p.from}+` : ''}</Text>
                <Text x={74} y={23} anchorY={0.5} style={{ fontFamily: FONT.sans, fontSize: 21, fontWeight: 700, fill: solid(on ? C.ink : C.paper) }}>{text}</Text>
              </Group>
            );
          })}
        </Group>
        <Text y={380} style={mono(16, C.grey)}>{`emphasis: [{ start: ${emphasis.start}, end: ${emphasis.end}, style: '${emphasis.style}' }]`}</Text>
        <Text y={410} style={mono(16, C.dim)}>{'duckGroup: "voice" (the default)'}</Text>
      </Group>
    </Group>
  );
}
