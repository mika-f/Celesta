import { Easings, Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { CodeCard, SYNTAX, codeCardHeight } from '../components/CodeCard';
import type { CodeLine } from '../components/CodeCard';
import { Label } from '../components/Label';
import { BODY, Panel } from '../components/Panel';
import { C } from '../constants';
import { lineFrom } from '../voice';

const TITLE = 'React で書く';
const NOTE = '部品を組み合わせて画面に';
export const TEXT = [TITLE, NOTE, '四角', '文字'];

const CODE: CodeLine[] = [
  [['<Composition', SYNTAX.tag], [' width', SYNTAX.attr], ['={1080}', SYNTAX.value], [' height', SYNTAX.attr], ['={1920}', SYNTAX.value], ['>', SYNTAX.tag]],
  [['  <Rect', SYNTAX.tag], [' fill', SYNTAX.attr], ['="#FFC93C"', SYNTAX.value], [' />', SYNTAX.tag]],
  [['  <Text', SYNTAX.tag], ['>', SYNTAX.tag], ['Hello!', SYNTAX.plain], ['</Text>', SYNTAX.tag]],
  [['</Composition>', SYNTAX.tag]],
];
const CHARS = CODE.flat().reduce((n, [text]) => n + [...text].length, 0);

// "Celesta writes video in React": the code types itself, and the picture
// it describes builds up beside it. On "parts", the two parts light up.
export function ReactScene() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const parts = lineFrom('react', 'react-3');
  const typed = Math.round(interpolate(frame, [4, 64], [0, CHARS], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }));
  const done = (line: number) => typed >= CODE.slice(0, line + 1).flat().reduce((n, [t]) => n + [...t].length, 0);
  const rectIn = spring({ frame: frame - 34, fps, config: { damping: 12 } });
  const textIn = spring({ frame: frame - 50, fps, config: { damping: 12 } });
  const lit = frame >= parts;
  const pulse = lit ? 1 + 0.06 * Math.max(0, Math.sin(((frame - parts) / fps) * Math.PI * 3)) : 1;
  const cardH = codeCardHeight(CODE.length, 28);
  // The preview phone, at 1/6 of the composition's size.
  const px = BODY.x + BODY.w - 180;
  const py = BODY.y + cardH + 24;
  const pw = 180;
  const ph = 320;
  return (
    <Panel index="01" title={TITLE} accent={C.blue}>
      <CodeCard x={BODY.x} y={BODY.y} width={BODY.w} file="film.tsx" lines={CODE} size={28}
        typed={typed} highlight={lit ? [1, 2] : []} />
      <Group opacity={interpolate(frame, [20, 28], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}>
        <Rect x={px} y={py} width={pw} height={ph} cornerRadius={24} fill={C.ink} />
        <Rect x={px + 8} y={py + 8} width={pw - 16} height={ph - 16} cornerRadius={18} fill={C.paper} />
        {done(1) && (
          <Rect x={px + pw / 2} y={py + ph / 2 - 30} anchorX={0.5} anchorY={0.5} width={130 * rectIn * pulse} height={130 * rectIn * pulse}
            cornerRadius={16} fill={C.yellow} />
        )}
        {done(2) && (
          <Label x={px + pw / 2} y={py + ph / 2 + 80} ax={0.5} ay={0.5} size={36} weight={900}
            scale={textIn * pulse}>Hello!</Label>
        )}
      </Group>
      <Group opacity={lit ? interpolate(frame, [parts, parts + 8], [0, 1], { extrapolateRight: 'clamp' }) : 0}>
        <Chip x={BODY.x} y={py + 40} color={C.yellow} label="<Rect>" note="四角" grow={spring({ frame: frame - parts, fps })} />
        <Chip x={BODY.x} y={py + 150} color={C.pink} label="<Text>" note="文字" grow={spring({ frame: frame - parts - 6, fps })} />
        <Label x={BODY.x} y={py + 262} ay={0.5} size={34} weight={800} color={C.soft}
          opacity={interpolate(frame, [parts + 14, parts + 22], [0, 1], { easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}>
          {NOTE}
        </Label>
      </Group>
    </Panel>
  );
}

function Chip({ x, y, color, label, note, grow }: { x: number; y: number; color: string; label: string; note: string; grow: number }) {
  return (
    <Group x={x + 40 * (1 - grow)} y={y} opacity={Math.min(1, grow * 1.5)}>
      <Rect width={250} height={86} cornerRadius={43} fill={color} stroke={C.ink} strokeWidth={5} />
      <Label x={125} y={43} ax={0.5} ay={0.5} size={36} font="mono" weight={800}>{label}</Label>
      <Label x={280} y={43} ay={0.5} size={48} weight={900}>{note}</Label>
    </Group>
  );
}
