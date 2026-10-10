import { Group, Rect, interpolate, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { CodeCard, SYNTAX } from '../components/CodeCard';
import type { CodeLine } from '../components/CodeCard';
import { Label } from '../components/Label';
import { ScaleAbout } from '../components/Panel';
import { C, SAFE, SAFE_CX, SAFE_W } from '../constants';
import { lineFrom } from '../voice';

export const TEXT = ['この動画、', 'ぜんぶ', 'コード。', '編集ソフト', 'いりません'];

const STICKER = { x: SAFE.right - 170, y: SAFE.top + 610 } as const;

const CODE: CodeLine[] = [
  [['export default function ', SYNTAX.tag], ['Short', SYNTAX.fn], ['() {', SYNTAX.plain]],
  [['  return ', SYNTAX.tag], ['<Composition', SYNTAX.tag], [' height', SYNTAX.attr], ['={1920}', SYNTAX.value], ['>', SYNTAX.tag]],
];

// The first second: the claim is on screen from frame 0, each word slamming
// in on its own beat. Then the "no editor" sticker for the reply.
export function Hook() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const reply = lineFrom('hook', 'hook-2');
  // Every word is on screen from frame 0, a little too big, and punches in to
  // size one after the other.
  const word = (delay: number, from = 1.2) => spring({ frame, fps, delay, from, to: 1, config: { damping: 11, stiffness: 260 } });
  const w2 = word(3);
  const w3 = word(6, 1.08); // the widest word: kept inside the safe area while it is enlarged
  const sticker = spring({ frame: frame - reply - 4, fps, config: { damping: 9, stiffness: 220 } });
  const shake = frame >= reply && frame < reply + 8 ? Math.sin((frame - reply) * 2.4) * 10 : 0;
  const settle = word(0);
  return (
    <Group>
      <Group x={shake}>
        <Label x={SAFE_CX} y={SAFE.top + 70} ax={0.5} ay={0.5} size={96} font="display" weight={400}
          scale={settle}>この動画、</Label>
        <Label x={SAFE_CX} y={SAFE.top + 245} ax={0.5} ay={0.5} size={168} font="display" weight={400} color={C.pink}
          stroke={{ color: C.ink, width: 10 }} scale={w2} rotation={-4}>ぜんぶ</Label>
        <Label x={SAFE_CX} y={SAFE.top + 432} ax={0.5} ay={0.5} size={190} font="display" weight={400}
          scale={w3}>コード。</Label>
      </Group>
      <Group opacity={interpolate(frame, [10, 18], [0, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}>
        <CodeCard x={SAFE.left} y={SAFE.top + 560} width={SAFE_W} file="film.tsx" lines={CODE} size={27}
          typed={Math.max(0, (frame - 10) * 3)} />
      </Group>
      {frame >= reply && (
        // Stuck on the editor card's top-right corner, clear of the headline.
        <ScaleAbout x={STICKER.x} y={STICKER.y} scale={sticker} rotation={8 - 4 * sticker}>
          <Rect x={STICKER.x - 160} y={STICKER.y - 75} width={320} height={150} cornerRadius={26} fill={C.yellow}
            stroke={C.ink} strokeWidth={6} />
          <Label x={STICKER.x} y={STICKER.y - 25} ax={0.5} ay={0.5} size={42} weight={900}>編集ソフト</Label>
          <Label x={STICKER.x} y={STICKER.y + 30} ax={0.5} ay={0.5} size={42} weight={900} color={C.pink}>いりません</Label>
        </ScaleAbout>
      )}
    </Group>
  );
}
