import { Easings, Group, Rect, useCurrentFrame } from '@celesta/react';
import { CHAPTERS } from '../chapters';
import { Label } from '../components/Label';
import { C, H, W } from '../constants';
import { pad, progress } from '../helpers';

// ── Index: the table of contents ──────────────────────────────────────────

const INDEX_TOP = 250;
const ROW = 68;

export function Index() {
  const f = useCurrentFrame();
  const scan = f >= 60 && f < 96 ? Math.floor((f - 60) / 4) : f >= 96 ? 0 : -1;
  const grow = progress(f, 100, 16, Easings.easeInOutExpo);
  const head = progress(f, 0, 14, Easings.easeOutExpo);

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group opacity={head}>
        <Label x={120} y={170} size={22} font="mono" weight={700} ay={0.5}>INDEX</Label>
        <Label x={220} y={170} size={22} font="ja" weight={500} color={C.grey} ay={0.5}>機能一覧</Label>
        <Label x={W - 120} y={170} size={20} font="mono" weight={400} color={C.grey} ax={1} ay={0.5}>
          {`${pad(CHAPTERS.length)} FEATURES / 1 FILE`}
        </Label>
      </Group>
      {scan >= 0 && scan < CHAPTERS.length && (
        <Rect x={120} y={INDEX_TOP + scan * ROW + 4} width={W - 240} height={ROW - 8} fill={C.blue} />
      )}
      {CHAPTERS.map((chapter, i) => {
        const y = INDEX_TOP + i * ROW;
        const p = progress(f, 6 + i * 4, 14, Easings.easeOutExpo);
        const lit = i === scan;
        const out = f >= 100 && i !== 0 ? 1 - progress(f, 100, 6) : 1;
        return (
          <Group key={chapter.key} opacity={p * out}>
            <Rect x={120} y={y} width={Math.max(1, (W - 240) * p)} height={1} fill={C.line} />
            <Label x={140 + 40 * (1 - p)} y={y + ROW / 2 + 14} size={44} font="serif" weight={400}
              color={lit ? C.paper : C.blue} ay="baseline">{pad(i + 1)}</Label>
            <Label x={250 + 40 * (1 - p)} y={y + ROW / 2} size={30} ay={0.5}>{chapter.key}</Label>
            <Label x={840 + 40 * (1 - p)} y={y + ROW / 2} size={24} font="ja" weight={500}
              color={lit ? C.paper : C.soft} ay={0.5}>{chapter.jaShort}</Label>
            <Label x={W - 140} y={y + ROW / 2} size={20} font="mono" weight={400} color={lit ? C.paper : C.grey}
              ax={1} ay={0.5}>{INDEX_API[i]}</Label>
          </Group>
        );
      })}
      {/* The first row opens up into chapter 01. */}
      {f >= 100 && (
        <Rect x={120 * (1 - grow)} y={(INDEX_TOP + 4) * (1 - grow)} width={W - 240 * (1 - grow)}
          height={ROW - 8 + (H - ROW + 8) * grow} fill={C.blue} />
      )}
    </>
  );
}

const INDEX_API = [
  '<Composition>', 'interpolate · spring', '<Grid> · <Stack>', '<Font>', '.celesta.json',
  '<Dialogue>', 'useCurrentFrame()', 'Celesta-export', 'skills/celesta',
];
