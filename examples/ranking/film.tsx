import * as fs from 'node:fs';

import {
  Composition,
  Easings,
  Font,
  Rect,
  Text,
  defineProjectProperties,
  getProjectProperty,
  progress,
  useCountUp,
  useCurrentFrame,
  useProjectProperty,
} from '@celesta/react';

// A ranking card template. The title, colors and data file come from
// project properties, so one source renders every variant:
//
//   celesta-exporter --react examples/ranking/film.tsx spring.mp4 \
//     --props-file examples/ranking/variants/spring.json
defineProjectProperties({
  title: { type: 'string', label: 'Title', defaultValue: 'Top Picks' },
  subtitle: { type: 'string', label: 'Subtitle', defaultValue: 'Sample data' },
  accent: { type: 'color', label: 'Accent', defaultValue: '#4f8cff' },
  theme: { type: 'select', label: 'Theme', defaultValue: 'dark', options: ['dark', 'light'] },
  data: { type: 'path', label: 'Data', defaultValue: './data/sample.json' },
});

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const ROW_START = 24;
const ROW_STAGGER = 8;
const HOLD = 75;
const MAX_ROWS = 6;
const FONT = 'Noto Sans JP';
const FONT_SRC = 'https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700';

interface Row {
  name: string;
  score: number;
}

let rows: Row[] = [];

// `getProjectProperty` sees the same values the render does, so the data
// file read here and the duration computed from it always match the variant.
export async function prepare(): Promise<void> {
  const file = getProjectProperty<string>('data');
  // A `path` property may also be an http(s) URL, which is fetched instead.
  const text = /^https?:\/\//i.test(file) ? await (await fetch(file)).text() : await fs.promises.readFile(file, 'utf8');
  const parsed: unknown = JSON.parse(text);
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error(`${file} must be a non-empty array of { name, score } rows`);
  }
  rows = parsed
    .map((row: Row) => ({ name: String(row.name), score: Number(row.score) }))
    .sort((a, b) => b.score - a.score)
    .slice(0, MAX_ROWS);
  // Bar widths are relative to the top score.
  if (rows.some((row) => !Number.isFinite(row.score) || row.score < 0) || rows[0].score === 0) {
    throw new Error(`${file}: every score must be a non-negative number, and the top score above 0`);
  }
}

const THEMES = {
  dark: { background: '#11131a', panel: '#1b1e28', text: '#f5f6fa', muted: '#8a90a6' },
  light: { background: '#f4f1ea', panel: '#ffffff', text: '#1d1f27', muted: '#6d6a63' },
} as const;

function useTheme() {
  return THEMES[useProjectProperty<'dark' | 'light'>('theme')];
}

function Header() {
  const frame = useCurrentFrame();
  const theme = useTheme();
  const accent = useProjectProperty<string>('accent');
  const enter = progress(frame, 0, 18, Easings.easeOutCubic);
  return (
    <>
      <Rect x={96} y={88} width={12} height={92} fill={accent} opacity={enter} />
      <Text
        x={132 - 24 * (1 - enter)}
        y={84}
        opacity={enter}
        style={{ fontFamily: FONT, fontSize: 64, fontWeight: 700, fill: { type: 'solid', color: theme.text } }}
      >
        {useProjectProperty<string>('title')}
      </Text>
      <Text
        x={134}
        y={166}
        opacity={enter}
        style={{ fontFamily: FONT, fontSize: 26, fill: { type: 'solid', color: theme.muted } }}
      >
        {useProjectProperty<string>('subtitle')}
      </Text>
    </>
  );
}

function RankRow({ row, index, top }: { row: Row; index: number; top: number }) {
  const frame = useCurrentFrame();
  const theme = useTheme();
  const accent = useProjectProperty<string>('accent');
  const delay = ROW_START + index * ROW_STAGGER;
  const enter = progress(frame, delay, 16, Easings.easeOutCubic);
  const grow = progress(frame, delay + 4, 30, Easings.easeOutQuart);
  const score = useCountUp(row.score, { delay: delay + 4, durationInFrames: 30 });
  const y = 228 + index * 76 + 20 * (1 - enter);
  const barWidth = 520 * (row.score / top) * grow;
  return (
    <>
      <Rect x={96} y={y} width={1088} height={64} cornerRadius={12} fill={theme.panel} opacity={enter} />
      <Text
        x={124}
        y={y + 32}
        anchorY={0.5}
        opacity={enter}
        style={{ fontFamily: FONT, fontSize: 30, fontWeight: 700, fill: { type: 'solid', color: accent } }}
      >
        {String(index + 1).padStart(2, '0')}
      </Text>
      <Text
        x={196}
        y={y + 32}
        anchorY={0.5}
        opacity={enter}
        style={{ fontFamily: FONT, fontSize: 30, fill: { type: 'solid', color: theme.text } }}
      >
        {row.name}
      </Text>
      <Rect x={540} y={y + 24} width={Math.max(barWidth, 1)} height={16} cornerRadius={8} fill={accent} opacity={enter} />
      <Text
        x={1156}
        y={y + 32}
        anchorY={0.5}
        anchorX={1}
        opacity={enter}
        style={{ fontFamily: FONT, fontSize: 30, fontWeight: 700, fill: { type: 'solid', color: theme.text } }}
      >
        {score.toLocaleString('en-US')}
      </Text>
    </>
  );
}

function Card() {
  const theme = useTheme();
  const top = rows[0].score;
  return (
    <>
      <Rect x={0} y={0} width={WIDTH} height={HEIGHT} fill={theme.background} />
      <Header />
      {rows.map((row, index) => (
        <RankRow key={row.name} row={row} index={index} top={top} />
      ))}
    </>
  );
}

export default function Ranking() {
  // Fewer rows finish sooner; the hold after the last row stays the same.
  const durationInFrames = ROW_START + rows.length * ROW_STAGGER + 30 + HOLD;
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={durationInFrames}>
      <Font src={FONT_SRC} />
      <Card />
    </Composition>
  );
}
