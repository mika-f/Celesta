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
  useCurrentFrame,
} from '@celesta.studio/react';
import { Counter } from '@celesta.studio/components';
import { TextBox } from '@celesta.studio/text';
import { useProjectProperty } from '@celesta.studio/project';

// A ranking card. The title, subtitle, colors, format and data file come from
// project properties, so one source renders every variant:
//
// - props.json holds the values this project renders with. Change the title,
//   accent color, theme (dark / light) or format (16:9 / 9:16) there.
// - data.json is the ranking: an array of { "name", "score" } rows. The top
//   six by score are shown, and the video gets shorter with fewer rows.
// - variants/*.json are alternative sets of values, rendered with
//   `--props-file variants/spring.json` instead of props.json.
//
// The colors of the two themes, every layout number, and the motion timings
// are in the constants block below.

// ---- Constants --------------------------------------------------------------

const FPS = 30;
// Timing, in frames. The header fades in over HEADER_IN. Rows start entering
// at ROW_START, one every ROW_STAGGER, each fading in over ROW_IN; BAR_DELAY
// after a row starts, its bar grows and its score counts up over BAR_GROW,
// both through BAR_EASING. The finished card holds for HOLD before the end.
const HEADER_IN = 18;
const ROW_START = 24;
const ROW_STAGGER = 8;
const ROW_IN = 16;
const BAR_DELAY = 4;
const BAR_GROW = 30;
const BAR_EASING = Easings.easeOutQuart;
const HOLD = 75;
const MAX_ROWS = 6;

const FONT = 'Noto Sans JP';
const FONT_SRC = 'https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700';

const THEMES = {
  dark: { background: '#11131a', panel: '#1b1e28', text: '#f5f6fa', muted: '#8a90a6' },
  light: { background: '#f4f1ea', panel: '#ffffff', text: '#1d1f27', muted: '#6d6a63' },
} as const;

// Layout, in pixels. Each format sets the canvas and the numbers that differ
// between a wide and a tall frame; positions are derived from these and the
// sizes below in `useLayout()`. The header and the rows are centered
// vertically. Inside a row, `textY` is the center of the text line, `barY` the
// top of the bar, and `scoreWidth` the room kept for the score at the right
// end of the line. `nameWidth` is the room for the name before the bar; a
// format without it draws the bar under the name.
type Format = '16:9' | '9:16';
interface FormatLayout {
  width: number;
  height: number;
  marginX: number;
  headerGap: number;
  rowHeight: number;
  rowGap: number;
  textY: number;
  barY: number;
  scoreWidth: number;
  nameWidth?: number;
}
const FORMATS: Record<Format, FormatLayout> = {
  // One line per row: rank, name, bar, score.
  '16:9': {
    width: 1920, height: 1080, marginX: 144, headerGap: 72,
    rowHeight: 96, rowGap: 18, textY: 48, barY: 36, scoreWidth: 168, nameWidth: 492,
  },
  // Rows stack in a taller column: rank, name and score, with the bar below.
  '9:16': {
    width: 1080, height: 1920, marginX: 72, headerGap: 120,
    rowHeight: 168, rowGap: 24, textY: 60, barY: 108, scoreWidth: 168,
  },
};

// Text sizes. The title and the row names shrink, down to their minimum, to
// fit their room on one line; text still too long at the minimum is cut off.
const TITLE_SIZE = 96;
const TITLE_MIN_SIZE = 48;
const SUBTITLE_SIZE = 39;
const ROW_TEXT_SIZE = 45;
const NAME_MIN_SIZE = 24;
// The height of a fitted line, as a multiple of its font size.
const LINE_HEIGHT = 1.3;
// The accent stripe left of the title; its height is the header's height.
const ACCENT_WIDTH = 18;
const HEADER_HEIGHT = 138;
// The title and subtitle start TITLE_INDENT right of the stripe; their tops
// are TITLE_Y and SUBTITLE_Y below the stripe's top. The title slides in
// from TITLE_SLIDE to the left.
const TITLE_INDENT = 54;
const TITLE_Y = -26;
const SUBTITLE_Y = 117;
const TITLE_SLIDE = 36;
// Inside a row: the padding at both ends, where the name starts, the space
// kept between the name or the bar and what follows it, and the bar.
const ROW_PADDING = 42;
const NAME_OFFSET = 150;
const NAME_GAP = 24;
const BAR_HEIGHT = 24;
const CORNER_RADIUS = 18;
// How far a row slides up while it fades in.
const ROW_RISE = 30;

defineProjectProperties({
  title: { type: 'string', label: 'Title', defaultValue: '人気ランキング' },
  subtitle: { type: 'string', label: 'Subtitle', defaultValue: 'サンプルデータ' },
  accent: { type: 'color', label: 'Accent', defaultValue: '#4f8cff' },
  theme: { type: 'select', label: 'Theme', defaultValue: 'dark', options: ['dark', 'light'] },
  format: { type: 'select', label: 'Format', defaultValue: '16:9', options: ['16:9', '9:16'] },
  data: { type: 'path', label: 'Data', defaultValue: './data.json' },
});

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
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    throw new Error(`${file}: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error(`${file} must be a non-empty array of { name, score } rows`);
  }
  rows = parsed
    .map((row: unknown, index) => {
      const { name, score } = (row ?? {}) as Partial<Row>;
      if (typeof row !== 'object' || row === null || typeof name !== 'string' || typeof score !== 'number') {
        throw new Error(`${file}: row ${index} must be an object with a string "name" and a number "score"`);
      }
      return { name, score };
    })
    .sort((a, b) => b.score - a.score)
    .slice(0, MAX_ROWS);
  // Bar widths are relative to the top score.
  if (rows.some((row) => !Number.isFinite(row.score) || row.score < 0) || rows[0].score === 0) {
    throw new Error(`${file}: every score must be a non-negative number, and the top score above 0`);
  }
}

function useTheme() {
  return THEMES[useProjectProperty<'dark' | 'light'>('theme')];
}

function useLayout() {
  const format = FORMATS[useProjectProperty<Format>('format')];
  const { width, height, marginX, headerGap, rowHeight, rowGap, scoreWidth, nameWidth } = format;
  const rowWidth = width - 2 * marginX;
  // The header and the rows `prepare()` kept, centered as one block.
  const blockHeight = HEADER_HEIGHT + headerGap + rows.length * rowHeight + (rows.length - 1) * rowGap;
  const top = Math.round((height - blockHeight) / 2);
  // The end of the text line, where the score column starts.
  const scoreX = rowWidth - ROW_PADDING - scoreWidth;
  const barX = NAME_OFFSET + (nameWidth ?? 0);
  return {
    ...format,
    left: marginX,
    rowWidth,
    top,
    rowsTop: top + HEADER_HEIGHT + headerGap,
    rowPitch: rowHeight + rowGap,
    titleWidth: width - 2 * marginX - TITLE_INDENT,
    // The name stops short of the bar beside it, or of the score.
    nameBoxWidth: (nameWidth ?? scoreX - NAME_OFFSET) - NAME_GAP,
    barX,
    barMaxWidth: nameWidth === undefined ? rowWidth - ROW_PADDING - barX : scoreX - NAME_GAP - barX,
  };
}

function Header() {
  const frame = useCurrentFrame();
  const theme = useTheme();
  const layout = useLayout();
  const accent = useProjectProperty<string>('accent');
  const enter = progress(frame, 0, HEADER_IN, Easings.easeOutCubic);
  const textX = layout.left + TITLE_INDENT;
  return (
    <>
      <Rect x={layout.left} y={layout.top} width={ACCENT_WIDTH} height={HEADER_HEIGHT} fill={accent} opacity={enter} />
      <TextBox
        x={textX - TITLE_SLIDE * (1 - enter)}
        y={layout.top + TITLE_Y}
        width={layout.titleWidth}
        height={TITLE_SIZE * LINE_HEIGHT}
        minFontSize={TITLE_MIN_SIZE}
        maxFontSize={TITLE_SIZE}
        maxLines={1}
        lineHeight={LINE_HEIGHT}
        verticalAlign="middle"
        opacity={enter}
        style={{ fontFamily: FONT, fontWeight: 700, fill: { type: 'solid', color: theme.text } }}
      >
        {useProjectProperty<string>('title')}
      </TextBox>
      <Text
        x={textX}
        y={layout.top + SUBTITLE_Y}
        opacity={enter}
        maxWidth={layout.titleWidth}
        style={{ fontFamily: FONT, fontSize: SUBTITLE_SIZE, fill: { type: 'solid', color: theme.muted } }}
      >
        {useProjectProperty<string>('subtitle')}
      </Text>
    </>
  );
}

function RankRow({ row, index, top }: { row: Row; index: number; top: number }) {
  const frame = useCurrentFrame();
  const theme = useTheme();
  const layout = useLayout();
  const accent = useProjectProperty<string>('accent');
  const delay = ROW_START + index * ROW_STAGGER;
  const enter = progress(frame, delay, ROW_IN, Easings.easeOutCubic);
  const grow = progress(frame, delay + BAR_DELAY, BAR_GROW, BAR_EASING);
  const x = layout.left;
  const y = layout.rowsTop + index * layout.rowPitch + ROW_RISE * (1 - enter);
  const textY = y + layout.textY;
  const barWidth = layout.barMaxWidth * (row.score / top) * grow;
  return (
    <>
      <Rect x={x} y={y} width={layout.rowWidth} height={layout.rowHeight} cornerRadius={CORNER_RADIUS} fill={theme.panel} opacity={enter} />
      <Text
        x={x + ROW_PADDING}
        y={textY}
        anchorY={0.5}
        opacity={enter}
        style={{ fontFamily: FONT, fontSize: ROW_TEXT_SIZE, fontWeight: 700, fill: { type: 'solid', color: accent } }}
      >
        {String(index + 1).padStart(2, '0')}
      </Text>
      <TextBox
        x={x + NAME_OFFSET}
        y={textY - (ROW_TEXT_SIZE * LINE_HEIGHT) / 2}
        width={layout.nameBoxWidth}
        height={ROW_TEXT_SIZE * LINE_HEIGHT}
        minFontSize={NAME_MIN_SIZE}
        maxFontSize={ROW_TEXT_SIZE}
        maxLines={1}
        lineHeight={LINE_HEIGHT}
        verticalAlign="middle"
        opacity={enter}
        style={{ fontFamily: FONT, fill: { type: 'solid', color: theme.text } }}
      >
        {row.name}
      </TextBox>
      <Rect
        x={x + layout.barX}
        y={y + layout.barY}
        width={Math.max(barWidth, 1)}
        height={BAR_HEIGHT}
        cornerRadius={BAR_HEIGHT / 2}
        fill={accent}
        opacity={enter}
      />
      {/* Counts with the bar's timing and easing, so the score matches the bar. */}
      <Counter
        x={x + layout.rowWidth - ROW_PADDING}
        y={textY}
        anchorY={0.5}
        anchorX={1}
        opacity={enter}
        value={row.score}
        delay={delay + BAR_DELAY}
        durationInFrames={BAR_GROW}
        easing={BAR_EASING}
        style={{ fontFamily: FONT, fontSize: ROW_TEXT_SIZE, fontWeight: 700, fill: { type: 'solid', color: theme.text } }}
      />
    </>
  );
}

function Card() {
  const theme = useTheme();
  const { width, height } = useLayout();
  const top = rows[0].score;
  return (
    <>
      <Rect x={0} y={0} width={width} height={height} fill={theme.background} />
      <Header />
      {rows.map((row, index) => (
        <RankRow key={index} row={row} index={index} top={top} />
      ))}
    </>
  );
}

export default function Ranking() {
  const { width, height } = useLayout();
  // Fewer rows finish sooner; the hold after the last row stays the same.
  const durationInFrames = ROW_START + rows.length * ROW_STAGGER + BAR_GROW + HOLD;
  return (
    <Composition width={width} height={height} fps={FPS} durationInFrames={durationInFrames}>
      <Font src={FONT_SRC} />
      <Card />
    </Composition>
  );
}
