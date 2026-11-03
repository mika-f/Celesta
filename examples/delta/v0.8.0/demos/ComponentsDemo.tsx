import { Group, Sequence, Text } from '@celesta.studio/react';
import { Badge, BarChart, PatternBackground, StatCard } from '@celesta.studio/components';
import type { DemoProps } from '../../components/ChapterShell';
import { CUT, STAGE } from '../../components/ChapterShell';
import { PRS } from '../prs';
import { BEAT, DURATION } from '../timeline';

// This release in numbers, drawn with @celesta.studio/components alone and the
// film's <Theme> names: a PatternBackground, three StatCards that count up,
// a BarChart of the pull requests by kind (counted from their titles), and
// a Badge for each new package, sized from its measured text. Everything
// runs on the components' own delays, from the chapter's downbeat.

/** A PR's kind, from its conventional-commit prefix; titles without one are "other". */
const kind = (title: string) => /^([a-z]+)[(:]/.exec(title)?.[1] ?? 'other';
const KINDS = Object.entries(PRS.reduce<Record<string, number>>((counts, [, title]) => {
  const k = kind(title);
  counts[k] = (counts[k] ?? 0) + 1;
  return counts;
}, {})).sort((a, b) => b[1] - a[1]).slice(0, 5);
/** Components the package exports in 0.8.0. */
const COMPONENTS = ['Badge', 'BarChart', 'Counter', 'FrameChrome', 'MediaFrame', 'Motion', 'Panel',
  'PatternBackground', 'Ranking', 'StatCard', 'TitleCard', 'TypewriterText'];
const PACKAGES = ['@celesta.studio/components', '@celesta.studio/lottie', '@celesta.studio/exporter'];

const label = { fontFamily: 'mono', fontSize: 16, fill: { type: 'solid', color: '#8E929B' } } as const;
const value = { fontFamily: 'display', fontSize: 92 } as const;

export function ComponentsDemo(_: DemoProps) {
  const card = { width: 264, height: 196, gap: 34 };
  return (
    <Group>
      <PatternBackground width={STAGE.width} height={STAGE.height} pattern="grid" spacing={40} size={1}
        background="ink" color="paper" opacity={0.06} speedX={-12} />
      <Sequence from={CUT}>
        {[
          { label: 'PULL REQUESTS', value: PRS.length },
          { label: 'COMPONENTS', value: COMPONENTS.length },
          { label: 'FRAMES IN THIS FILM', value: DURATION },
        ].map((stat, i) => (
          <StatCard key={stat.label} x={i * (card.width + card.gap)} y={12} width={card.width} height={card.height}
            align="left" fill={i === 0 ? 'orange' : 'raised'} stroke={i === 0 ? undefined : 'line'} strokeWidth={2} cornerRadius={14}
            label={stat.label} value={stat.value} delay={i * 4} durationInFrames={3 * BEAT}
            labelStyle={i === 0 ? { ...label, fill: { type: 'solid', color: '#0C0D0F' } } : label}
            valueStyle={value} color={i === 0 ? 'ink' : 'paper'} />
        ))}
        <Text x={0} y={258} fontFamily="mono" fontSize={17} fontWeight={700} color="grey">PRS BY KIND · BarChart</Text>
        <BarChart x={0} y={292} width={STAGE.width} data={KINDS.map(([id, count], i) => ({
          id, label: id, value: count, color: i === 0 ? 'orange' : 'grey',
        }))} max={KINDS[0][1]} delay={BEAT} stagger={4} durationInFrames={2 * BEAT}
          barHeight={30} cornerRadius={6} trackColor="#FFFFFF08" rowGap={14}
          labelStyle={{ fontFamily: 'mono', fontSize: 19, fontWeight: 700, fill: { type: 'solid', color: '#EFECE4' } }}
          valueStyle={{ fontFamily: 'mono', fontSize: 19, fontWeight: 700, fill: { type: 'solid', color: '#EFECE4' } }} />
        <Text x={0} y={640} fontFamily="mono" fontSize={17} fontWeight={700} color="grey">NEW PACKAGES · Badge</Text>
        {PACKAGES.map((name, i) => (
          <Sequence key={name} from={6 * BEAT + i * 4}>
            <Badge x={[0, 284, 520][i]} y={672} fill={i === 0 ? 'orange' : 'raised'} stroke={i === 0 ? undefined : 'line'}
              cornerRadius="pill" fontFamily="mono" fontSize={20} fontWeight={700} color={i === 0 ? 'ink' : 'paper'}>
              {name}
            </Badge>
          </Sequence>
        ))}
      </Sequence>
    </Group>
  );
}
