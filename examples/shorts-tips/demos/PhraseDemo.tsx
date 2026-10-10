import { Easings, Group, Rect, Text, progress } from '@celesta/react';
import { Circle, Polyline } from '@celesta/shapes';
import { useAccent, useStage } from '../components/DemoClock';
import { C, DEMO, FONT } from '../constants';

// lineBreak: 'phrase': the same sentence in the same maxWidth, wrapped the
// default way (left) and between 文節 (right).
// Stages (variants/phrase.json): 0 `maxWidth`, 1 `lineBreak`, 2 the sentence, 3 `</Text>`.
export const PHRASE_STAGES = 4;

// Keep these equal to the snippet in variants/phrase.json. At this size and
// width seven characters fit a line, so the default wrap leaves 「す」 alone.
const SENTENCE = '動画をコードで書けるツールです';
const MAX_WIDTH = 370;
const SIZE = 52;
const LINE = 72;

const COLUMN_Y = 120;
const COLUMN_H = LINE * 3 + 40;

function Column({ x, label, lineBreak, color, lit, mark }: {
  x: number;
  label: string;
  lineBreak: 'normal' | 'phrase';
  color: string;
  lit: number;
  mark: 'good' | 'bad';
}) {
  const enter = progress(useStage(0), 0, 12, Easings.easeOutCubic);
  const typed = Math.floor(SENTENCE.length * progress(useStage(2), 0, 20));
  const done = progress(useStage(3), 0, 14, Easings.easeOutBack);
  const labelColor = lit > 0 ? color : C.grey;
  return (
    <Group x={x} y={20 * (1 - enter)} opacity={enter}>
      <Text y={36} anchorY={0.5}
        style={{ fontFamily: FONT.mono, fontSize: 32, fontWeight: 700, fill: { type: 'solid', color: labelColor } }}>
        {`'${label}'`}
      </Text>
      <Rect x={-14} y={COLUMN_Y - 20} width={MAX_WIDTH + 28} height={COLUMN_H} cornerRadius={14}
        fill="#0D1019" stroke={lit > 0 ? `${color}80` : C.dim} strokeWidth={2} />
      {/* The maxWidth edge. */}
      <Rect x={MAX_WIDTH} y={COLUMN_Y - 8} width={3} height={COLUMN_H - 24} fill={C.grey} opacity={0.6} />
      {typed > 0 && (
        <Text y={COLUMN_Y} maxWidth={MAX_WIDTH}
          style={{
            fontFamily: FONT.ja, fontSize: SIZE, fontWeight: 700, lineHeight: LINE, lineBreak,
            fill: { type: 'solid', color: C.paper }, visibleCharacters: typed < SENTENCE.length ? typed : undefined,
          }}>
          {SENTENCE}
        </Text>
      )}
      {done > 0 && mark === 'bad' && (
        // Around the 「す」 left alone on the third line.
        <Circle x={SIZE / 2} y={COLUMN_Y + LINE * 2 + LINE / 2 - 4} anchorX={0.5} anchorY={0.5} radius={42}
          stroke={color} strokeWidth={5} scale={done} />
      )}
      <Group x={MAX_WIDTH / 2} y={COLUMN_Y + COLUMN_H + 130} scale={done * 2.6} opacity={Math.min(1, done)}>
        {mark === 'good'
          ? <Polyline points={[[-26, 0], [-6, 20], [28, -18]]} stroke={color} strokeWidth={10} />
          : <>
              <Polyline points={[[-20, -20], [20, 20]]} stroke={color} strokeWidth={10} />
              <Polyline points={[[20, -20], [-20, 20]]} stroke={color} strokeWidth={10} />
            </>}
      </Group>
    </Group>
  );
}

export function PhraseDemo() {
  const accent = useAccent();
  const phrase = useStage(1);
  return (
    <>
      <Column x={DEMO.safe.left + 14} label="normal" lineBreak="normal" color={C.bad} lit={1} mark="bad" />
      <Column x={DEMO.safe.left + 450} label="phrase" lineBreak="phrase" color={accent} lit={phrase} mark="good" />
    </>
  );
}
