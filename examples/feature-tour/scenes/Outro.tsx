import type { ReactNode } from 'react';
import { Easings, Group, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { CHAPTERS } from '../chapters';
import { Label } from '../components/Label';
import { BEAT, C, H, W } from '../constants';
import { clamp, hash, pad, progress } from '../helpers';
import { TAGLINE } from './Open';

// ── Outro ─────────────────────────────────────────────────────────────────

export function Outro() {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const flash = 1 - progress(f, 0, 14, Easings.easeOutCubic);
  const hit = progress(f, 0, 22, Easings.easeOutExpo);
  const rule = progress(f, 16, 20, Easings.easeInOutExpo);
  const ja = progress(f, 26, 16, Easings.easeOutExpo);
  const url = progress(f, 36, 16, Easings.easeOutExpo);
  const fadeOut = 1 - progress(f, durationInFrames - 24, 24, Easings.easeInCubic);
  const beat = Math.floor(f / BEAT);

  // A faint sheet of frames behind the logo; a few light up on each beat.
  const cols = 16;
  const rows = 9;
  const cell = W / cols;
  const sheet: ReactNode[] = [];
  const drift = f * 0.4;
  for (let r = 0; r < rows + 1; r++) {
    for (let c = 0; c < cols; c++) {
      const lit = hash(r * cols + c, beat) > 0.96 && (r < 2 || c < 3 || c > 12);
      sheet.push(
        <Rect key={`${r}-${c}`} x={c * cell + 6} y={r * cell + 6 - drift} width={cell - 12} height={cell - 12}
          cornerRadius={4} fill={lit ? '#3D5BFF26' : undefined} stroke="#FFFFFF0A" strokeWidth={1} />,
      );
    }
  }

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group opacity={fadeOut}>
        {sheet}
        <Label x={W / 2} y={440 + 20 * (1 - hit)} size={230} ax={0.5} ay={0.5} scale={1.2 - 0.2 * hit}
          opacity={clamp(hit * 2)}>Celesta</Label>
        <Rect x={W / 2} y={590} anchorX={0.5} width={Math.max(1, 600 * rule)} height={6} fill={C.blue} />
        <Label x={W / 2} y={670 + 16 * (1 - ja)} size={44} font="ja" weight={700} ax={0.5} ay={0.5} opacity={ja}>
          {TAGLINE}
        </Label>
        <Label x={W / 2} y={745 + 16 * (1 - url)} size={22} font="mono" weight={400} color={C.grey} ax={0.5} ay={0.5}
          opacity={url}>github.com/mika-f/celesta</Label>
        {CHAPTERS.map((chapter, i) => {
          const on = progress(f, 44 + i * 6, 8, Easings.easeOutExpo);
          const x = W / 2 + (i - (CHAPTERS.length - 1) / 2) * 150;
          return (
            <Group key={chapter.key} x={x} y={900} opacity={0.25 + 0.75 * on}>
              <Label x={0} y={0} size={40} font="serif" weight={400} color={on > 0.5 ? C.blue : C.grey} ax={0.5} ay="baseline">
                {pad(i + 1)}
              </Label>
              <Label x={0} y={30} size={12} font="mono" weight={700} color={C.soft} ax={0.5} ay={0.5}>{chapter.key}</Label>
            </Group>
          );
        })}
      </Group>
      <Rect width={W} height={H} fill={C.blue} opacity={flash} />
    </>
  );
}
