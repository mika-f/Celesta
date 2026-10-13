import { Easings, Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { random } from '@celesta/math';
import { Diamond } from '../components/Diamond';
import { Headline } from '../components/Headline';
import { Label } from '../components/Label';
import { BAR, BEAT, C } from '../constants';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

// The mock timeline editor.
const EDITOR = { x: 160, y: 360, width: 1600, height: 520, ruler: 40, labels: 200 } as const;
const TRACKS = ['title.y', 'title.opacity', 'card.x', 'card.scale', 'chart.height', 'caption.y', 'logo.rotation', 'bg.color'];
const ROW = (EDITOR.height - EDITOR.ruler) / TRACKS.length;
const COLUMN = 120;
const COLUMNS = Math.floor((EDITOR.width - EDITOR.labels) / COLUMN);

// Six keyframes per track, in seeded columns.
const KEYS = TRACKS.flatMap((_, row) => {
  const columns = Array.from({ length: COLUMNS }, (_, c) => c)
    .sort((a, b) => random(`key-${row}-${a}`) - random(`key-${row}-${b}`))
    .slice(0, 6)
    .sort((a, b) => a - b);
  return columns.map((column) => ({ row, column }));
});

const HOP = BEAT / 2; // the selection moves one keyframe per eighth note
const SLIP = BAR * 2 + BEAT * 2; // bar 7, beat 3: everything after the cursor slips
const LINE_AT = BAR * 3; // bar 9: the keyframes collapse into one line
const LINE = 'y = spring(frame)';

function keyPosition(i: number, local: number): [number, number] {
  const { row, column } = KEYS[i];
  const x = EDITOR.labels + column * COLUMN + COLUMN / 2;
  const y = EDITOR.ruler + row * ROW + ROW / 2;
  // Each keyframe the cursor has visited was nudged a little by hand.
  const visited = Math.floor(local / HOP);
  const nudge = i < visited ? (random(`nudge-${i}`) - 0.5) * 36 : 0;
  // At the slip, every keyframe after the cursor moves out of its column.
  const slip = local >= SLIP && i >= Math.floor(SLIP / HOP)
    ? interpolate(local, [SLIP, SLIP + 6], [0, 30 + random(`slip-${i}`) * 50], { ...clamp, easing: Easings.easeOutBack })
    : 0;
  return [x + nudge + slip, y];
}

// 10–18 s. Keyframes nudged one at a time; one nudge knocks the rest out of
// line; then all of them collapse into a single line of code.
export function Problem() {
  const local = useCurrentFrame();
  const { fps } = useVideoConfig();
  const headline = local >= LINE_AT ? 'Or one line.' : local >= BAR * 2 ? 'Again.' : 'Keyframes. By hand.';
  const cursor = Math.min(Math.floor(local / HOP), KEYS.length - 1);
  const panel = interpolate(local, [LINE_AT, LINE_AT + 20], [1, 0], clamp);
  const lineOpacity = interpolate(local, [LINE_AT + 18, LINE_AT + 30], [0, 1], clamp);
  const lineWidth = 80 * 0.6 * LINE.length;
  return (
    <>
      <Headline x={160} y={160} size={96}>{headline}</Headline>
      <Group x={EDITOR.x} y={EDITOR.y}>
        <Group opacity={panel}>
          <Rect width={EDITOR.width} height={EDITOR.height} fill={C.panel} stroke={C.line} strokeWidth={1} />
          <Rect y={EDITOR.ruler} width={EDITOR.width} height={1} fill={C.line} />
          <Rect x={EDITOR.labels} width={1} height={EDITOR.height} fill={C.line} />
          {Array.from({ length: COLUMNS + 1 }, (_, c) => (
            <Rect key={c} x={EDITOR.labels + c * COLUMN} y={EDITOR.ruler - 10} width={1} height={10} fill={C.muted} />
          ))}
          {Array.from({ length: COLUMNS }, (_, c) => (
            <Label key={c} x={EDITOR.labels + c * COLUMN + 6} y={8} size={14} color={C.muted}>{`${c * 15}`}</Label>
          ))}
          {TRACKS.map((name, row) => (
            <Group key={name}>
              <Rect y={EDITOR.ruler + (row + 1) * ROW} width={EDITOR.width} height={1} fill={C.line} />
              <Label x={24} y={EDITOR.ruler + row * ROW + ROW / 2} size={18} color={C.muted} ay={0.5}>{name}</Label>
            </Group>
          ))}
          {local < SLIP + 30 && (() => {
            const [x, y] = keyPosition(cursor, local);
            return <Rect x={x - 18} y={y - 18} width={36} height={36} stroke={C.accent} strokeWidth={2} />;
          })()}
        </Group>
        {KEYS.map((_, i) => {
          const [x, y] = keyPosition(i, Math.min(local, LINE_AT));
          const t = spring({ frame: local - LINE_AT, fps, delay: (i % 12) * 0.5, config: { stiffness: 120, damping: 18 } });
          const tx = (EDITOR.width - lineWidth) / 2 + (i / (KEYS.length - 1)) * lineWidth;
          const ty = 720 - EDITOR.y;
          return (
            <Diamond key={i} x={x + (tx - x) * t} y={y + (ty - y) * t} size={16 * (1 - t * 0.8)}
              color={i === cursor && local < LINE_AT ? C.accent : C.fg} opacity={1 - interpolate(t, [0.85, 1], [0, 1], clamp)} />
          );
        })}
      </Group>
      <Label x={960} y={720} size={80} weight={700} ax={0.5} ay={0.5} opacity={lineOpacity}>{LINE}</Label>
    </>
  );
}
