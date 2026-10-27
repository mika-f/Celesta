import type { ReactNode } from 'react';
import { Group, Rect, Text } from '@celesta/react';
import { C, FONT, solid } from '../components/theme';
import { CHAPTERS } from './chapters';
import { LANDSCAPE, PORTRAIT, useLayout } from './layout';
import type { Layout } from './layout';
import { STAGE, number } from './program';
import { PRS } from './prs';

// The multiviewer of a control room: the preview (PVW) and program (PGM)
// monitors on top, the twelve features as twelve sources under them, each
// with its label and tally, and a ticker of the release's pull requests.
// The open lights it monitor by monitor and dives into PGM; the outro pulls
// back out of it.

// The film's wall: PVW and PGM side by side, two rows of six sources. The
// short's: PVW and PGM as 9:16 monitors side by side, so the dive into PGM
// fills the portrait frame, and four rows of three sources under them.
export interface WallGeometry {
  width: number;
  height: number;
  big: { width: number; height: number };
  pvw: { x: number; y: number };
  pgm: { x: number; y: number };
  small: { width: number; height: number };
  umd: number;
  smallUmd: number;
  sourceAt: (i: number) => { x: number; y: number };
  tickerY: number;
  /** Margin of the ticker's text. */
  margin: number;
}

function geometry(layout: Layout): WallGeometry {
  const { width, height, portrait } = layout;
  const m = 24;
  const g = portrait ? 10 : 12;
  const columns = portrait ? 3 : 6;
  const rows = 12 / columns;
  const umd = 36;
  const smallUmd = portrait ? 26 : 30;
  const big = { width: (width - 2 * m - g) / 2, height: ((width - 2 * m - g) / 2) * (height / width) };
  const smallWidth = (width - 2 * m - (columns - 1) * g) / columns;
  const small = { width: smallWidth, height: smallWidth * (STAGE.height / STAGE.width) };
  const rowsY = m + big.height + umd + g;
  return {
    width, height, big, umd, smallUmd, small, margin: m,
    pvw: { x: m, y: m },
    pgm: { x: m + big.width + g, y: m },
    sourceAt: (i) => ({ x: m + (i % columns) * (small.width + g), y: rowsY + Math.floor(i / columns) * (small.height + smallUmd + g) }),
    tickerY: rowsY + rows * (small.height + smallUmd + g) - g + (portrait ? 4 : 12),
  };
}

const LANDSCAPE_WALL = geometry(LANDSCAPE);
const PORTRAIT_WALL = geometry(PORTRAIT);
export const wallGeometry = (layout: Layout) => (layout.portrait ? PORTRAIT_WALL : LANDSCAPE_WALL);

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/**
 * The camera on the wall: 0 shows the whole wall, 1 fills the frame with
 * PGM. The visible width shrinks geometrically, so the dive keeps one speed.
 */
export function wallCamera(dive: number, layout: Layout = LANDSCAPE) {
  const { width, big, pgm } = wallGeometry(layout);
  const scale = (width / big.width) ** dive;
  const u = (width - width / scale) / (width - big.width);
  return { x: -pgm.x * u * scale, y: -pgm.y * u * scale, scale };
}

export type Tally = 'program' | 'preview' | 'off';
const tallyFill = (t: Tally) => (t === 'program' ? 'tally' : t === 'preview' ? 'preview' : 'line');

/** A monitor: a black screen, the picture once powered on (a white flash as it comes up), its label under it. */
function Screen({ x, y, width, height, umd, label, tally, power, labelSize, children }: {
  x: number; y: number; width: number; height: number; umd: number; label: string; tally: Tally;
  /** 0 off, 1 on; in between, the flash. */
  power: number; labelSize: number; children?: ReactNode;
}) {
  return (
    <Group x={x} y={y}>
      <Rect width={width} height={height} fill="#000000" />
      {power > 0 ? <Group clip={{ width, height }}>{children}</Group> : null}
      {power > 0 && power < 1 ? <Rect width={width} height={height} fill="paper" opacity={1 - power} /> : null}
      {tally === 'program' ? <Rect x={-3} y={-3} width={width + 6} height={height + 6} stroke="tally" strokeWidth={5} /> : null}
      <Rect y={height} width={width} height={umd} fill="#08090B" />
      <Rect x={0} y={height} width={umd * 0.42} height={umd} fill={tallyFill(tally)} opacity={power > 0 ? 1 : 0.3} />
      <Text x={umd * 0.42 + 10} y={height + umd / 2} anchorY={0.5} style={mono(labelSize, power > 0 ? C.paper : C.dim, 700)}>{label}</Text>
    </Group>
  );
}

/** A ticker of the release's pull requests, `offset` pixels along, drawing only what is in view. */
function Ticker({ offset, y, lead, width, margin }: { offset: number; y: number; lead: string; width: number; margin: number }) {
  const size = 19;
  const advance = (s: string) => [...s].reduce((sum, ch) => sum + (ch.charCodeAt(0) > 0x2000 ? size : size * 0.6), 0);
  const items = [lead, ...PRS.map(([n, title]) => `#${n} ${title}`)];
  const gap = 70;
  const widths = items.map(advance);
  const loop = widths.reduce((a, b) => a + b + gap, 0);
  const start = -(offset % loop);
  const placed: { text: string; x: number; first: boolean }[] = [];
  for (let pass = 0; pass < 2; pass++) {
    let x = start + pass * loop;
    items.forEach((text, i) => {
      if (x < width && x + widths[i] > 0) placed.push({ text, x, first: i === 0 });
      x += widths[i] + gap;
    });
  }
  return (
    <Group y={y}>
      <Rect width={width} height={38} fill="#08090B" />
      <Group x={margin} clip={{ width: width - 2 * margin, height: 38 }}>
        {placed.map(({ text, x, first }, i) => (
          <Group key={i} x={x}>
            <Text x={-gap / 2} y={19} anchorX={0.5} anchorY={0.5} style={mono(size, C.orange, 700)}>Δ</Text>
            <Text y={19} anchorY={0.5} style={mono(size, first ? C.orange : C.paper, first ? 700 : 400)}>{text}</Text>
          </Group>
        ))}
      </Group>
    </Group>
  );
}

export interface WallProps {
  pvw: ReactNode;
  pgm: ReactNode;
  pvwLabel: string;
  pgmLabel: string;
  /** A source's picture: drawn at STAGE size, scaled into its monitor. */
  source: (index: number) => ReactNode;
  /** Power of a monitor: 0 is PVW, 1 is PGM, 2.. are the sources. */
  power: (monitor: number) => number;
  onAir: boolean;
  /** The source on air, if any, and the one in preview. */
  sourceTally: (index: number) => Tally;
  ticker: number;
  lead: string;
  /** Left and right levels, 0–1, overlaid on PGM. */
  meters: readonly [number, number];
}

/** The multiviewer, filling the layout's frame; PVW and PGM draw at the frame's size. */
export function Wall(props: WallProps) {
  const { pvw, pgm, pvwLabel, pgmLabel, source, power, onAir, sourceTally, ticker, lead, meters } = props;
  const wall = wallGeometry(useLayout());
  const { big, small } = wall;
  const k = big.width / wall.width;
  return (
    <Group>
      <Rect width={wall.width} height={wall.height} fill="#050506" />
      <Screen {...wall.pvw} {...big} umd={wall.umd} label={pvwLabel} tally="preview" power={power(0)} labelSize={18}>
        <Group scale={k}>{pvw}</Group>
      </Screen>
      <Screen {...wall.pgm} {...big} umd={wall.umd} label={pgmLabel} tally={onAir ? 'program' : 'off'} power={power(1)} labelSize={18}>
        <Group scale={k}>{pgm}</Group>
        {meters.map((level, i) => (
          <Group key={i} x={big.width - 40 + i * 16} y={24}>
            <Rect width={10} height={big.height - 48} fill="#000000" opacity={0.6} />
            <Rect y={(big.height - 48) * (1 - level)} width={10} height={(big.height - 48) * level}
              fill={level > 0.85 ? 'tally' : level > 0.65 ? 'amber' : 'preview'} />
          </Group>
        ))}
      </Screen>
      {CHAPTERS.map((chapter, i) => {
        const at = wall.sourceAt(i);
        return (
          <Screen key={i} {...at} {...small} umd={wall.smallUmd} label={`${number(i)} ${chapter.title}`} tally={sourceTally(i)}
            power={power(i + 2)} labelSize={14}>
            <Group scale={small.width / STAGE.width}>
              <Rect width={STAGE.width} height={STAGE.height} fill="panel" />
              {source(i)}
            </Group>
          </Screen>
        );
      })}
      <Ticker offset={ticker} y={wall.tickerY} lead={lead} width={wall.width} margin={wall.margin} />
    </Group>
  );
}

/** Bars to show on a monitor with no picture yet: the series palette, as a test signal. */
export function Bars({ label }: { label: string }) {
  const { width: w0, height: h } = useLayout();
  const colors = [C.paper, C.amber, C.blue, C.green, C.violet, C.red, C.orange, C.grey];
  const w = w0 / colors.length;
  return (
    <Group>
      {colors.map((color, i) => <Rect key={i} x={i * w} width={w + 1} height={h * 0.72} fill={color} />)}
      <Rect y={h * 0.72} width={w0} height={h * 0.28} fill="ink" />
      <Text x={w0 / 2} y={h * 0.86} anchorX={0.5} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 150, fill: solid(C.paper) }}>{label}</Text>
    </Group>
  );
}
