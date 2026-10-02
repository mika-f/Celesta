import type { ReactNode } from 'react';
import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { C, CHAPTER_COUNT, DURATION, DW, DX } from '../constants';
import { clamp, hash, pad, progress, timecode } from '../helpers';

// ── 08 · Export: frames become a file ─────────────────────────────────────

const MOSAIC_COLS = 14;
const MOSAIC_ROWS = 8;

export function ExportDemo() {
  const f = useCurrentFrame();
  const p = Easings.easeInOutCubic(clamp((f - 10) / 84));
  const pct = Math.round(p * 100);
  const done = pct === 100;
  const total = MOSAIC_COLS * MOSAIC_ROWS;
  const filled = p * total;
  const tw = 50;
  const th = 28;
  const g = 6;
  const mx = DX + (DW - (MOSAIC_COLS * (tw + g) - g)) / 2;
  const my = 250;
  const flash = done ? 1 - progress(f, 95, 12) : 0;
  const tiles: ReactNode[] = [];
  for (let i = 0; i < total; i++) {
    const c = i % MOSAIC_COLS;
    const r = Math.floor(i / MOSAIC_COLS);
    const x = mx + c * (tw + g);
    const y = my + r * (th + g);
    const on = i < filled;
    const head = i === Math.floor(filled) && !done;
    // Each tile stands for a frame of this film; its marks hint at which chapter.
    const chapter = Math.floor((i / total) * (CHAPTER_COUNT + 2));
    tiles.push(
      <Group key={i}>
        <Rect x={x} y={y} width={tw} height={th} fill={head ? C.paper : on ? C.ink : undefined}
          stroke={on || head ? undefined : '#FFFFFF40'} strokeWidth={on || head ? undefined : 1} />
        {on && (
          <>
            <Rect x={x + 6} y={y + 7} width={8 + 20 * hash(chapter, 1)} height={4} fill={C.paper} opacity={0.8} />
            <Rect x={x + 6} y={y + 15} width={12 + 20 * hash(i, 2)} height={3} fill={chapter % 3 === 0 ? C.blue : C.grey} />
          </>
        )}
      </Group>,
    );
  }
  return (
    <>
      {tiles}
      <Rect x={mx - 12} y={my - 12} width={MOSAIC_COLS * (tw + g) - g + 24} height={MOSAIC_ROWS * (th + g) - g + 24}
        fill={C.paper} opacity={flash * 0.9} />
      <Label x={DX + DW} y={690} size={150} ax={1} ay="baseline">{`${pct}%`}</Label>
      <Label x={DX} y={690} size={18} font="mono" weight={700} ay="baseline">{done ? 'DONE' : 'RENDERING…'}</Label>
      <Label x={DX} y={660} size={16} font="mono" weight={400} ay="baseline" opacity={0.7}>
        {`frame ${pad(Math.round(p * DURATION), 4)} / ${DURATION}`}
      </Label>
      <Rect x={DX} y={730} width={DW} height={6} fill="#FFFFFF40" />
      <Rect x={DX} y={730} width={Math.max(1, DW * p)} height={6} fill={C.paper} />
      {['1920 × 1080', '30 fps', 'H.264 + AAC', 'GPU'].map((spec, i) => {
        const on = progress(f, 20 + i * 8, 10, Easings.easeOutExpo);
        return (
          <Group key={spec} x={DX + i * 200} y={790} opacity={on}>
            <Rect y={-4} width={8} height={8} fill={C.paper} />
            <Label x={20 + 16 * (1 - on)} y={0} size={18} font="mono" weight={400} ay={0.5}>{spec}</Label>
          </Group>
        );
      })}
      <Group opacity={progress(f, 96, 10, Easings.easeOutExpo)} x={DX} y={850 + 20 * (1 - progress(f, 96, 10, Easings.easeOutExpo))}>
        <Rect y={-26} width={DW} height={52} cornerRadius={26} fill={C.ink} />
        <Rect x={24} y={-5} width={10} height={10} cornerRadius={5} fill={C.paper} />
        <Label x={46} y={0} size={18} font="mono" weight={700} ay={0.5}>feature-tour.mp4</Label>
        <Label x={DW - 24} y={0} size={18} font="mono" weight={400} color={C.soft} ax={1} ay={0.5}>
          {`${timecode(DURATION)} · ${DURATION} frames`}
        </Label>
      </Group>
    </>
  );
}
