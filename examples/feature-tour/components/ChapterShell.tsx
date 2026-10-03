import type { ReactNode } from 'react';
import { Easings, Group, Rect, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { CHAPTERS } from '../chapters';
import { Label } from './Label';
import { Swap } from './Swap';
import { C, H, W } from '../constants';
import { monoWidth, pad, progress } from '../helpers';

export function ChapterShell({ index, children }: { index: number; children: ReactNode }) {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const chapter = CHAPTERS[index];
  const onBlue = chapter.bg === C.blue;
  const accent = onBlue ? C.paper : C.blue;
  const exit = 1 - progress(f, durationInFrames - 8, 8, Easings.easeInCubic);
  const lines = chapter.en.split('\n');

  const numberIn = progress(f, 0, 14, Easings.easeOutExpo);
  const tagIn = progress(f, 4, 12, Easings.easeOutExpo);
  const captionIn = progress(f, 16, 14, Easings.easeOutExpo);
  const chipIn = progress(f, 22, 14, Easings.easeOutExpo);

  return (
    <>
      <Rect width={W} height={H} fill={chapter.bg} />
      <Group opacity={exit} y={-24 * (1 - exit)}>
        <Label x={116} y={250 + 30 * (1 - numberIn)} size={170} font="serif" weight={400} color={accent}
          ay="baseline" opacity={numberIn}>{pad(index + 1)}</Label>
        <Group x={300 - 20 * (1 - tagIn)} y={190} opacity={tagIn}>
          <Label x={0} y={0} size={22} font="mono" weight={700} ay={0.5}>{chapter.key}</Label>
          <Label x={0} y={34} size={18} font="mono" weight={400} color={onBlue ? C.paper : C.grey} ay={0.5}
            opacity={onBlue ? 0.7 : 1}>
            {`${pad(index + 1)} / ${pad(CHAPTERS.length)}`}
          </Label>
        </Group>
        {lines.map((line, i) => {
          const p = progress(f, 6 + i * 4, 16, Easings.easeOutExpo);
          return (
            <Label key={i} x={120} y={300 + i * 112 + 60 * (1 - p)} size={92} opacity={p}>{line}</Label>
          );
        })}
        <Label x={120} y={570 + 20 * (1 - captionIn)} size={28} font="ja" weight={500}
          color={onBlue ? C.paper : C.soft} lineHeight={50} opacity={captionIn}>
          {chapter.ja}
        </Label>
        <Group y={24 * (1 - chipIn)} opacity={chipIn}>
          <Swap f={f} cues={chapter.api} render={(text, p) => {
            const w = monoWidth(text, 22) + 56;
            return (
              <Group x={120} y={840} opacity={p}>
                <Rect y={-28} width={w} height={56} cornerRadius={28}
                  fill={onBlue ? C.ink : C.panel} stroke={onBlue ? undefined : C.line} strokeWidth={onBlue ? undefined : 1} />
                <Rect x={24} y={-4} width={8} height={8} cornerRadius={4} fill={onBlue ? C.paper : C.blue} />
                <Label x={44} y={0} size={22} font="mono" weight={500} color={onBlue ? C.paper : C.sky} ay={0.5}>
                  {text}
                </Label>
              </Group>
            );
          }} />
        </Group>
        {children}
      </Group>
    </>
  );
}
