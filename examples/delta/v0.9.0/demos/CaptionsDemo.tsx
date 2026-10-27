import { Group, Rect, Text } from '@celesta/react';
import { CaptionTrack, PatternBackground } from '@celesta/components';
import { LayoutBounds } from '@celesta/layout';
import { graphemes, phrases } from '@celesta/text';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { VOICE_DELAY } from '../timeline';
import { voiceLevel, voices } from '../voices';
import { Bars, Lane, mono } from './common';

// CAPTIONS: voice-1 plays (film-0.9.0.tsx places it), and a real
// <CaptionTrack> lights its transcript a phrase at a time on a 9:16 feed
// (wordUnit="phrase": BudouX phrases, frames shared out by length). The
// chips are phrases() of the same text, lit by the same rule; under them,
// the voice's level, and the English a word at a time, three to a page.

export const CAPTIONS_EN = 'Hello. Let me load a voice folder.';
const PHONE = { x: 56, y: 22, width: 290, height: 519 };
const RIGHT = 400;

/** Which of `words` is lit at `frame`, sharing `[from, from + length)` out by cluster count. */
function litWord(words: readonly string[], from: number, length: number, frame: number) {
  const sizes = words.map((w) => graphemes(w).length);
  const total = sizes.reduce((a, b) => a + b, 0);
  let start = from;
  for (let i = 0; i < words.length; i++) {
    const end = start + (length * sizes[i]) / total;
    if (frame < end) return frame >= from ? i : -1;
    start = end;
  }
  return words.length;
}

export function CaptionsDemo({ frame }: DemoProps) {
  const voice = voices().captions;
  const text = voice.line.text;
  const length = voice.durationInFrames;
  const parts = phrases(text);
  const lit = litWord(parts, VOICE_DELAY, length, frame);
  const cue = [{ from: VOICE_DELAY, durationInFrames: length + 10, text }];
  // The voice's level over its frames, revealed up to the playhead.
  const hops = 56;
  const levels = Array.from({ length: hops }, (_, i) => {
    const f = voice.from + (i / hops) * length;
    return f - voice.from <= frame - VOICE_DELAY ? Math.min(1, voiceLevel(voice, f) * 4) : 0;
  });
  let cx = 0;
  let cy = 0;
  const chips = parts.map((part, i) => {
    const width = graphemes(part).length * 26 + 26;
    if (cx + width > STAGE.width - RIGHT - 40) { cx = 0; cy += 58; }
    const chip = { part, i, x: cx, y: cy, width };
    cx += width + 10;
    return chip;
  });
  return (
    <Group>
      <Group x={PHONE.x} y={PHONE.y} clip={{ width: PHONE.width, height: PHONE.height, cornerRadius: 18 }}>
        <PatternBackground width={PHONE.width} height={PHONE.height} pattern="stripes" spacing={36} size={14} angle={-30}
          background="#1A1C22" color="paper" opacity={0.05} speedX={20} />
        <LayoutBounds width={PHONE.width} height={PHONE.height}>
          <CaptionTrack cues={cue} subtitles={false} wordUnit="phrase" position="middle" margin={10}
            fontFamily="sans" fontSize={28} lineHeight={1.45} maxLines={3} color="paper" style={{ fontWeight: 700 }}
            background={null} padding={0}
            highlight={{ color: C.ink, background: C.orange, scale: 1.1, upcomingColor: '#EFECE466', cornerRadius: 3 }} />
        </LayoutBounds>
      </Group>
      <Rect x={PHONE.x - 3} y={PHONE.y - 3} width={PHONE.width + 6} height={PHONE.height + 6} cornerRadius={20} stroke="line" strokeWidth={3} />
      <Text x={PHONE.x + PHONE.width / 2} y={PHONE.y + PHONE.height - 26} anchorX={0.5} style={mono(15, C.dim)}>9:16 · wordUnit="phrase"</Text>

      <Group x={RIGHT} y={52}>
        <Text style={mono(18, C.grey)}>phrases(text)</Text>
        <Group y={38}>
          {chips.map(({ part, i, x, y, width }) => (
            <Group key={i} x={x} y={y}>
              <Rect width={width} height={46} cornerRadius={4} fill={i === lit ? 'orange' : i < lit ? 'raised' : 'panel'}
                stroke={i === lit ? 'orange' : 'line'} strokeWidth={2} />
              <Text x={width / 2} y={24} anchorX={0.5} anchorY={0.5}
                style={mono(24, i === lit ? C.ink : i < lit ? C.paper : C.grey, 700)}>{part}</Text>
            </Group>
          ))}
        </Group>
      </Group>
      <Group x={RIGHT} y={290}>
        <Lane x={0} y={0} width={STAGE.width - RIGHT - 40} label={'voice-1.wav  duckGroup="voice"'} />
        <Bars x={0} y={10} width={STAGE.width - RIGHT - 40} height={70} values={levels} color={C.orange} />
      </Group>
      <Group x={RIGHT} y={418}>
        <Text y={-12} anchorY={1} style={mono(15, C.dim)}>{"wordUnit=\"word\"  paging={{ maxWords: 3 }}"}</Text>
        <LayoutBounds width={STAGE.width - RIGHT - 40} height={110}>
          <CaptionTrack cues={[{ from: VOICE_DELAY, durationInFrames: length + 10, text: CAPTIONS_EN }]} subtitles={false}
            position="top" align="left" margin={0} paging={{ maxWords: 3 }} maxLines={1}
            fontFamily="sans" fontSize={44} color="paper" style={{ fontWeight: 700 }} background={null} padding={0}
            highlight={{ color: C.ink, background: C.paper, upcomingColor: '#EFECE455', cornerRadius: 3 }} />
        </LayoutBounds>
      </Group>
    </Group>
  );
}
