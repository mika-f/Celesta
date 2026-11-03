import { BarChart, PatternBackground, Ranking } from '@celesta.studio/components';
import { Composition, Easings, Font, Text, Theme } from '@celesta.studio/react';

// <BarChart> draws one row per item: a label column, a bar from 0, and the
// value printed with formatNumber() (as <Counter> prints it). Each row starts
// `stagger` frames after the one above; its bar grows and its value counts
// from the same progress, so the number always matches the bar.
// <Ranking> adds ranks and needs `order` spelled out: sorted by value, or
// kept as given. Rows are keyed by `id`.

const colors = { bg: '#0E1319', surface: '#182030', line: '#2C3A50', ink: '#F4F1EA', dim: '#8A97A8', accent: '#FF4D1F', sky: '#5BC0FF' };

const tools = [
  { id: 'remotion', label: 'Remotion', note: 're-bundle + Chromium', value: 9.4, color: '#5BC0FFBF' },
  { id: 'fframes', label: 'fframes', note: 'cargo build --release', value: 41.2, color: '#B48CFFBF' },
  { id: 'celesta', label: 'Celesta', note: 're-bundle', value: 1.3, color: 'accent', textColor: 'accent' },
];

const languages = [
  { id: 'ts', label: 'TypeScript', value: 18240 },
  { id: 'rs', label: 'Rust', value: 30512 },
  { id: 'wgsl', label: 'WGSL', value: 1460 },
  { id: 'md', label: 'Markdown', value: 9733 },
  { id: 'empty', label: 'COBOL', value: 0 },
];

export default function WithBarChart() {
  return (
    <Composition width={960} height={540} fps={30} durationInFrames={120}>
      <Font google="Inter" weights={[400, 700]} />
      <Font google="JetBrains Mono" weights={[700]} />
      <Theme fonts={{ sans: 'Inter', mono: 'JetBrains Mono' }} colors={colors}>
        <PatternBackground background="bg" pattern="dots" spacing={24} size={2} color="ink" opacity={0.08} />

        {/* Values ride the bar ends; per-item colors; a track behind each bar. */}
        <Text x={48} y={40} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">BARCHART · valuePosition "end"</Text>
        <BarChart x={48} y={72} width={864} data={tools} max={45} decimals={1} suffix=" s"
          delay={6} stagger={10} durationInFrames={40} fadeInFrames={12} easing={Easings.easeOutCubic}
          barHeight={40} cornerRadius={8} trackColor="#FFFFFF08" rowGap={20}
          labelStyle={{ fontFamily: 'sans', fontSize: 24, fontWeight: 700, fill: { type: 'solid', color: colors.ink } }}
          noteStyle={{ fontFamily: 'sans', fontSize: 15, fill: { type: 'solid', color: colors.dim } }}
          valueStyle={{ fontFamily: 'mono', fontSize: 28, fontWeight: 700, fill: { type: 'solid', color: colors.ink } }} />

        {/* Sorted descending, ranked, values in a right-aligned column; 0 draws no bar. */}
        <Text x={48} y={292} fontFamily="mono" fontSize={14} fontWeight={700} color="dim">RANKING · order "descending" · valuePosition "right"</Text>
        <Ranking x={48} y={324} width={864} data={languages} order="descending" valuePosition="right"
          formatRank={(rank) => String(rank).padStart(2, '0')} suffix=" lines"
          delay={40} stagger={6} durationInFrames={36} barHeight={14} cornerRadius={7} rowGap={10} barColor="sky"
          rankStyle={{ fontFamily: 'mono', fontSize: 20, fontWeight: 700, fill: { type: 'solid', color: colors.accent } }}
          labelStyle={{ fontFamily: 'sans', fontSize: 20, fontWeight: 700, fill: { type: 'solid', color: colors.ink } }}
          valueStyle={{ fontFamily: 'mono', fontSize: 20, fontWeight: 700, fill: { type: 'solid', color: colors.ink } }} />
      </Theme>
    </Composition>
  );
}
