import { Group, Rect, Text, useCurrentFrame } from '@celesta/react';
import { useCodePoint } from '@celesta/code';
import { CodePanel, codeStyle } from '../components/CodePanel';
import { Headline } from '../components/Headline';
import { KeyCap } from '../components/KeyCap';
import { Label } from '../components/Label';
import { BEAT, C, FONT, H, W } from '../constants';
import { S } from '../shared/cues';
import { EDITS, SAVE_FRAMES, TITLE, savedAt, typedAt } from '../shared/live-edit';

const FONT_SIZE = 26;
const PREVIEW = { x: 980, y: 200, width: 860, scale: 860 / W } as const;

function Caret({ source, line, column, visible }: { source: string; line: number; column: number; visible: boolean }) {
  const point = useCodePoint(source, { line, column }, codeStyle(FONT_SIZE));
  return visible ? <Rect x={point.x} y={point.y} width={2} height={point.lineHeight} fill={C.accent} /> : null;
}

// 34–44 s. Edits typed in the editor reach the preview only when saved, on
// the frame the key goes down.
export function LiveEdit() {
  const local = useCurrentFrame();
  const { source, caret } = typedAt(local);
  const props = savedAt(local);
  const typing = EDITS.some((e) => local >= e.at && local < e.end);
  const pressed = SAVE_FRAMES.some((f) => local >= f && local < f + 6);
  const flash = SAVE_FRAMES.some((f) => local >= f && local < f + 4);
  const headline = local >= BEAT * 16 ? 'Frame-exact.' : local >= SAVE_FRAMES[1] ? 'It reloads.' : local >= SAVE_FRAMES[0] ? 'Save.' : '';
  return (
    <>
      {headline && <Headline x={80} y={96} size={80}>{headline}</Headline>}
      <CodePanel x={80} y={200} width={860} height={560} source={source} fontSize={FONT_SIZE} title="title.tsx">
        <Caret source={source} line={caret.line} column={caret.column} visible={typing || local % 30 < 15} />
      </CodePanel>

      <Group x={PREVIEW.x} y={PREVIEW.y} scale={PREVIEW.scale}>
        <Text x={TITLE.x} y={TITLE.y} anchorX={TITLE.anchorX} anchorY={0.5}
          style={{ fontFamily: FONT.display, fontWeight: 700, fontSize: props.fontSize, fill: { type: 'solid', color: props.fill } }}>
          Hello, Celesta.
        </Text>
      </Group>
      <Rect x={PREVIEW.x} y={PREVIEW.y} width={PREVIEW.width} height={H * PREVIEW.scale}
        stroke={flash ? C.accent : C.line} strokeWidth={flash ? 2 : 1} />
      <Label x={PREVIEW.x} y={PREVIEW.y + H * PREVIEW.scale + 16} size={20} color={C.muted}>
        {`preview · frame ${S.liveEdit + local}`}
      </Label>

      <KeyCap x={PREVIEW.x} y={760} pressed={pressed} />
    </>
  );
}
