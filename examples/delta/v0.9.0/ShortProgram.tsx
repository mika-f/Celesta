import { Group, Sequence, Text, useCurrentFrame } from '@celesta/react';
import { LowerThird } from '@celesta/components';
import { Backdrop } from '../components/Backdrop';
import { DiffPanel } from '../components/DiffPanel';
import { C, FONT, ease, solid } from '../components/theme';
import { PORTRAIT } from './layout';
import { Caption, Monitor, STAGE } from './program';
import type { Chapter } from './program';
import { CHAPTER_LENGTH } from './timeline';

// A program for the 9:16 short. The same parts as Program, stacked: the
// lower third at the top of the safe area (placement="top-left" with
// safeArea="vertical-social"), the headline, the program monitor at the
// frame's width, the caption lit word by word, and the diff. Everything to
// be read is inside the safe area; the monitor's picture reaches past its
// right edge, as pictures may. The rundown is the film's alone.
const { width: WIDTH, height: HEIGHT, safe } = PORTRAIT;
const LEFT = safe.left + 24;
const INNER = WIDTH - safe.left - safe.right - 48;
const SCALE = 0.96;
const MONITOR_Y = 626;
const CAPTION_Y = MONITOR_Y + STAGE.height * SCALE + 24;
const CAPTION = { fontSize: 30, maxLines: 3 };
const DIFF_Y = CAPTION_Y + Math.ceil(CAPTION.fontSize * 1.45 * CAPTION.maxLines) + 14;

export function ShortProgram({ chapter, index, live = true }: { chapter: Chapter; index: number; live?: boolean }) {
  const frame = useCurrentFrame();
  const head = ease(frame, 4, 20);
  const code = ease(frame, 12, 16);
  const typed = Math.max(0, Math.floor((frame - 16) * 2.6));
  return (
    <Group>
      <Backdrop width={WIDTH} height={HEIGHT} />
      <Sequence from={6} durationInFrames={CHAPTER_LENGTH - 14}>
        <LowerThird title={chapter.title} subtitle={chapter.merged.map(([n, label]) => `#${n} ${label}`).join('  ·  ')}
          placement="top-left" safeArea="vertical-social" margin={24} maxWidth={INNER}
          fontFamily="display" color="paper" subtitleColor="grey"
          titleSize={112} minTitleSize={72} maxTitleLines={1} titleLineHeight={1.0}
          titleStyle={{ letterSpacing: { em: 0.01 } }}
          subtitleSize={20} minSubtitleSize={16} maxSubtitleLines={2} subtitleStyle={{ fontFamily: FONT.mono, fontWeight: 700 }}
          background="raised" accent="orange" accentWidth={10} cornerRadius={4}
          padding={{ top: 18, right: 30, bottom: 20, left: 30 }} gap={6} />
      </Sequence>
      <Text x={LEFT} y={440 + 24 * (1 - head)} opacity={head} maxWidth={INNER}
        style={{ fontFamily: FONT.sans, fontSize: 50, fontWeight: 700, lineHeight: 60, fill: solid(C.paper) }}>
        {chapter.headline}
      </Text>
      <Monitor index={index} chapter={chapter} frame={frame} live={live} x={safe.left} y={MONITOR_Y} scale={SCALE} meta={false} />
      <Caption text={chapter.caption} x={LEFT} y={CAPTION_Y} width={INNER} {...CAPTION} />
      <Group opacity={code} y={16 * (1 - code)}>
        <DiffPanel x={LEFT} y={DIFF_Y} width={INNER} file={chapter.file} code={chapter.code} language={chapter.language} typed={typed} />
      </Group>
    </Group>
  );
}
