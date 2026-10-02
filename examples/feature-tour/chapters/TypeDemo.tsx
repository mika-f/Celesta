import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { Label } from '../components/Label';
import { BEAT, C, DW, DX, FONT } from '../constants';
import { clamp, progress } from '../helpers';

// ── 04 · Type: one card, eight faces ──────────────────────────────────────

type Specimen = {
  font: keyof typeof FONT;
  weight: number;
  glyph: string;
  sample: string;
  meta: string;
  color?: string;
  stroke?: { color: string; width: number };
};

export const SPECIMENS: Specimen[] = [
  { font: 'display', weight: 800, glyph: 'Aa', sample: 'Every frame, typeset.', meta: 'Unbounded · 800' },
  { font: 'serif', weight: 400, glyph: 'Aa', sample: 'Every frame, typeset.', meta: 'Instrument Serif · 400' },
  { font: 'ja', weight: 900, glyph: '字', sample: '動画を、コードで書く。', meta: 'Noto Sans JP · 900' },
  { font: 'jaDisplay', weight: 400, glyph: '字', sample: '動画を、コードで書く。', meta: 'Dela Gothic One · 400' },
  { font: 'dot', weight: 400, glyph: '字', sample: '動画を、コードで書く。', meta: 'DotGothic16 · 400' },
  { font: 'mono', weight: 700, glyph: '{ }', sample: 'const f = useCurrentFrame();', meta: 'JetBrains Mono · 700' },
  { font: 'ja', weight: 900, glyph: '縁', sample: '縁取りも、スタイルひとつで。', meta: 'stroke · 10 px',
    color: C.ink, stroke: { color: C.sky, width: 10 } },
  { font: 'display', weight: 800, glyph: 'Aa', sample: 'Load anything with <Font>.', meta: 'fill + stroke',
    color: C.blue, stroke: { color: C.paper, width: 6 } },
];

export function TypeDemo() {
  const f = useCurrentFrame();
  const k = clamp(Math.floor(f / BEAT), 0, SPECIMENS.length - 1);
  const local = f - k * BEAT;
  const s = SPECIMENS[k];
  const punch = 1 + 0.08 * (1 - progress(local, 0, 8, Easings.easeOutExpo));
  const enter = progress(f, 2, 16, Easings.easeOutExpo);
  const top = 250;
  const h = 580;
  const listX = DX + DW - 250;
  return (
    <Group y={40 * (1 - enter)} opacity={enter}>
      <Rect x={DX} y={top} width={DW} height={h} cornerRadius={14} fill={C.panel} stroke={C.line} strokeWidth={1} />
      <Rect x={listX - 30} y={top + 30} width={1} height={h - 60} fill={C.line} />
      <Label x={(DX + listX - 30) / 2} y={top + 230} size={220} font={s.font} weight={s.weight} ax={0.5} ay={0.5}
        color={s.color ?? C.paper} stroke={s.stroke} scale={punch}>{s.glyph}</Label>
      <Label x={DX + 40} y={top + 440} size={s.font === 'mono' ? 22 : 30} font={s.font} weight={s.weight}
        color={C.paper} ay={0.5} opacity={progress(local, 1, 6)}>{s.sample}</Label>
      <Label x={DX + 40} y={top + 510} size={16} font="mono" weight={400} color={C.grey} ay={0.5}>{s.meta}</Label>
      <Label x={listX - 60} y={top + 510} size={16} font="mono" weight={700} color={C.blue} ax={1} ay={0.5}>
        {`${k + 1} / ${SPECIMENS.length}`}
      </Label>
      {SPECIMENS.map((spec, i) => (
        <Group key={i} x={listX} y={top + 70 + i * 62}>
          {i === k && <Rect x={-14} y={-5} width={10} height={10} fill={C.blue} />}
          <Label x={0} y={0} size={15} font="mono" weight={i === k ? 700 : 400} color={i === k ? C.paper : C.grey} ay={0.5}>
            {spec.meta.split(' · ')[0]}
          </Label>
        </Group>
      ))}
    </Group>
  );
}
