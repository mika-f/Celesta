import { Counter, PatternBackground, StatCard } from '@celesta.studio/components';
import { Composition, Easings, Font, Text, Theme } from '@celesta.studio/react';

// <Counter> is a <Text> that counts to `value`, printed with formatNumber():
// grouped for `locale` ('en-US' unless given, so the machine's locale never
// changes the output), `decimals` always shown, `prefix`/`suffix`, or a
// `formatter`. <StatCard> puts one on a <Panel> with a label and a note,
// sized once for the wider end of the count so it never grows mid-count.

const colors = { bg: '#0E1319', surface: '#182030', line: '#2C3A50', ink: '#F4F1EA', dim: '#8A97A8', accent: '#FF4D1F' };
const label = { fontFamily: 'ja', fontSize: 20, fill: { type: 'solid', color: colors.dim } } as const;
const value = { fontFamily: 'ja', fontSize: 52, fontWeight: 700 } as const;
const note = { fontFamily: 'sans', fontSize: 16, fill: { type: 'solid', color: colors.dim } } as const;

export default function WithCounter() {
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="JetBrains Mono" weights={[700]} />
      <Font google="Noto Sans JP" weights={[400]} />
      <Theme fonts={{ sans: 'Inter', mono: 'JetBrains Mono', ja: ['Inter', 'Noto Sans JP'] }} colors={colors}>
        <PatternBackground background="bg" pattern="dots" spacing={24} size={2} color="ink" opacity={0.08} />

        {/* Right-anchored: the last digit and the suffix stay put as digits are added. */}
        <Text x={48} y={40} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">COUNTER · anchorX 1</Text>
        <Counter x={480} y={72} anchorX={1} value={1280000} delay={6} durationInFrames={60}
          suffix=" 件" fontFamily="ja" fontSize={72} fontWeight={700} color="ink" />
        <Text x={516} y={40} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">DOWN · DECIMALS · LOCALE</Text>
        <Counter x={516} y={72} from={100} value={-12.5} decimals={1} delay={6} durationInFrames={60}
          easing={Easings.easeInOutCubic} suffix="%" fontFamily="mono" fontSize={44} fontWeight={700} color="accent" />
        <Counter x={516} y={130} value={98765.4} decimals={2} locale="de-DE" delay={6} durationInFrames={60}
          prefix="€ " fontFamily="mono" fontSize={30} fontWeight={700} color="ink" />

        {/* Cards of one size in a row; the middle one has no note, the last uses a formatter. */}
        <StatCard x={48} y={264} width={272} height={208} align="left" fill="surface" stroke="line" strokeWidth={1}
          label="総ダウンロード" value={1280} suffix=" 件" note="2026 年 10 月" delay={20} durationInFrames={40}
          labelStyle={label} valueStyle={value} noteStyle={note} color="ink" />
        <StatCard x={344} y={264} width={272} height={208} align="center" fill="surface" stroke="line" strokeWidth={1}
          label="Retention" value={87.5} decimals={1} suffix="%" delay={28} durationInFrames={40}
          labelStyle={label} valueStyle={value} color="ink" />
        <StatCard x={640} y={264} width={272} height={208} align="right" fill="accent"
          label="Watch time" value={5430} formatter={(s) => `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, '0')}m`}
          note="per viewer" delay={36} durationInFrames={40}
          labelStyle={{ ...label, fill: { type: 'solid', color: colors.ink } }} valueStyle={value}
          noteStyle={{ ...note, fill: { type: 'solid', color: colors.ink } }} color="ink" />
      </Theme>
    </Composition>
  );
}
