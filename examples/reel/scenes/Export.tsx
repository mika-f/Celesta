import type { Run } from '../components/Mono';
import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { Mono, runLength } from '../components/Mono';
import { Tag } from '../components/Tag';
import { BEAT, C, DURATION, H, W } from '../constants';
import { clamp, progress } from '../helpers';
import { monoAdvance } from '../metrics';

const SPECS = ['1920 × 1080', '30 fps', 'H.264 + AAC', 'GPU · yuv420p'];
const COMMAND: Run[] = [
  { text: '$ ', color: C.accent, weight: 700 },
  { text: 'Celesta-export', color: C.ink, weight: 700 },
  { text: ' --react ', color: C.grey },
  { text: 'reel.tsx reel.mp4', color: C.ink },
];

export function Export() {
  const f = useCurrentFrame();
  const p = Easings.easeInOutCubic(clamp((f - 14) / 82));
  const pct = Math.round(p * 100);
  const done = pct === 100;
  const inAt = progress(f, 0, 14, Easings.easeOutExpo);
  const typed = Math.floor(clamp((f - 2) * 3.5, 0, runLength(COMMAND)));
  return (
    <>
      <Rect width={W} height={H} fill={C.paper} />
      <Tag x={120} y={150} n="05" name="EXPORT" color={C.ink} opacity={inAt} />
      <Label x={120} y={200 + 30 * (1 - inAt)} size={112} lineHeight={116} color={C.ink} opacity={inAt}>
        {'What you preview\nis what you ship.'}
      </Label>
      <Mono runs={COMMAND} x={124} y={480} size={28} visible={typed} />
      {typed < runLength(COMMAND) || Math.floor(f / 8) % 2 === 0 ? (
        <Rect x={124 + typed * 28 * monoAdvance + 2} y={464} width={3} height={32} fill={C.accent} />
      ) : null}
      {SPECS.map((spec, i) => {
        const on = progress(f, 20 + i * BEAT, 10, Easings.easeOutExpo);
        return (
          <Group key={spec} x={120} y={620 + i * 44} opacity={on}>
            <Rect y={-4} width={8} height={8} fill={i === SPECS.length - 1 ? C.accent : C.ink} />
            <Label x={24 + 20 * (1 - on)} y={0} size={26} font="mono" weight={400} color={C.ink} ay={0.5}>{spec}</Label>
          </Group>
        );
      })}
      <Label x={W - 330} y={690} size={440} color={done ? C.accent : C.ink} ax={1} ay={0.5}
        scale={done ? 1 + 0.06 * (1 - progress(f, 96, 10, Easings.easeOutExpo)) : 1}>
        {String(pct)}
      </Label>
      <Label x={W - 120} y={560} size={160} weight={500} color={C.ink} ax={1} ay={0.5}>%</Label>
      <Rect x={120} y={910} width={W - 240} height={4} fill={C.ink} opacity={0.15} />
      <Rect x={120} y={910} width={Math.max(1, (W - 240) * p)} height={4} fill={done ? C.accent : C.ink} />
      <Label x={120} y={950} size={20} font="mono" weight={400} color={C.ink} ay={0.5}>
        {`frame ${String(Math.round(p * DURATION)).padStart(3, '0')} / ${DURATION}`}
      </Label>
      <Label x={W - 120} y={950} size={20} font="mono" weight={700} color={done ? C.accent : C.ink} ax={1} ay={0.5}>
        {done ? 'DONE → reel.mp4' : 'RENDERING…'}
      </Label>
    </>
  );
}
