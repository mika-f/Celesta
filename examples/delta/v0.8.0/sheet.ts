import { Easings, interpolate } from '@celesta.studio/react';
import { CUT } from '../components/ChapterShell';
import { CARD_LENGTH, CHAPTER_LENGTH, FEATURES, MORE_AT, MORE_CARDS, OPEN_LENGTH, OUTRO_AT, chapterAt } from './timeline';

// The proof sheet, in world units: one tile is a 1920×1080 frame, so a
// camera zoom of 1 fills a 1920-wide screen with it. A header across the
// top, the twelve features six to a row, and the eight MORE cards below,
// four to a row: a sheet about as wide as a 16:9 screen.
export const TILE = { width: 1920, height: 1080 } as const;
const GAP = 200;
const MARGIN = 420;
const COLUMNS = 6;
const CARD_COLUMNS = 4;
const HEADER = 1500;
const MORE_GAP = 420;

const featureTop = MARGIN + HEADER;
const moreTop = featureTop + 2 * TILE.height + GAP + MORE_GAP;

export const SHEET = {
  width: 2 * MARGIN + COLUMNS * TILE.width + (COLUMNS - 1) * GAP,
  height: moreTop + 2 * TILE.height + GAP + MARGIN,
  margin: MARGIN,
  header: { x: MARGIN, y: MARGIN, height: HEADER },
  moreTop,
} as const;

export interface Slot { x: number; y: number }

const rowWidth = (columns: number) => columns * TILE.width + (columns - 1) * GAP;
const slot = (top: number, i: number, columns: number): Slot => ({
  x: MARGIN + (rowWidth(COLUMNS) - rowWidth(columns)) / 2 + (i % columns) * (TILE.width + GAP),
  y: top + Math.floor(i / columns) * (TILE.height + GAP),
});

/** Feature `i`'s tile, its top-left corner. */
export const featureSlot = (i: number) => slot(featureTop, i, COLUMNS);
/** MORE card `i`'s tile. */
export const cardSlot = (i: number) => slot(moreTop, i, CARD_COLUMNS);
const center = (s: Slot) => ({ x: s.x + TILE.width / 2, y: s.y + TILE.height / 2 });

// --- the camera -------------------------------------------------------------

/**
 * Where the camera looks: the world point at the screen's centre, the zoom
 * in "tiles across the screen" (1 fills the screen's width with a tile), and
 * how far the sheet is tipped back (rotationX) and turned (rotation).
 */
export interface Shot { x: number; y: number; zoom: number; tilt: number; turn: number }

const HEADER_SHOT: Shot = { x: SHEET.width / 2, y: SHEET.header.y + HEADER / 2 + 200, zoom: 0.24, tilt: 46, turn: -6 };
// The whole sheet in a 16:9 window, with a little desk around it.
const WHOLE: Shot = { x: SHEET.width / 2, y: SHEET.height / 2, zoom: 0.94 * Math.min(1920 / SHEET.width, 1080 / SHEET.height) * (TILE.width / 1920), tilt: 0, turn: 0 };
const on = (s: Slot, zoom = 1): Shot => ({ ...center(s), zoom, tilt: 0, turn: 0 });

/** A hop from one shot to another: up and back on the way, tipped and turned, landing flat. */
function hop(a: Shot, b: Shot, t: number, lift: { zoom: number; tilt: number; turn: number }): Shot {
  const e = Easings.easeInOutCubic(t);
  const bump = Math.sin(Math.PI * t);
  const zoom = Math.exp(Math.log(a.zoom) + (Math.log(b.zoom) - Math.log(a.zoom)) * e) * (1 - (1 - lift.zoom) * bump);
  return {
    x: a.x + (b.x - a.x) * e,
    y: a.y + (b.y - a.y) * e,
    zoom,
    tilt: a.tilt + (b.tilt - a.tilt) * e + lift.tilt * bump,
    turn: a.turn + (b.turn - a.turn) * e + lift.turn * bump,
  };
}

/** Frames before a downbeat that the camera leaves the last tile. */
export const TRAVEL = 36;
const CARD_TRAVEL = 14;

/** The camera at a frame of the film. */
export function shotAt(frame: number): Shot {
  // The open: the header, then a sweep over the features, landing on the first.
  if (frame < OPEN_LENGTH) {
    const t = (frame - OPEN_LENGTH / 2) / (OPEN_LENGTH / 2);
    if (t <= 0) return HEADER_SHOT;
    const over: Shot = { x: SHEET.width / 2, y: featureSlot(0).y + TILE.height + 100, zoom: 0.16, tilt: 34, turn: 5 };
    return t < 0.55 ? hop(HEADER_SHOT, over, t / 0.55, { zoom: 1, tilt: 0, turn: 0 })
      : hop(over, on(featureSlot(0)), (t - 0.55) / 0.45, { zoom: 1, tilt: 0, turn: 0 });
  }
  // The features: hold, drifting in a little, then hop to the next.
  if (frame < MORE_AT) {
    const i = Math.floor((frame - OPEN_LENGTH) / CHAPTER_LENGTH);
    const f = frame - chapterAt(i);
    const here = on(featureSlot(i), 1 + 0.03 * Math.min(1, f / (CHAPTER_LENGTH - TRAVEL)));
    const leave = CHAPTER_LENGTH - TRAVEL;
    if (f < leave) return here;
    const next = i + 1 < FEATURES ? on(featureSlot(i + 1)) : on(cardSlot(0));
    const far = i + 1 === FEATURES;
    return hop(here, next, (f - leave) / TRAVEL, { zoom: far ? 0.3 : 0.45, tilt: far ? 30 : 22, turn: i % 2 ? -4 : 4 });
  }
  // MORE: a card every two beats, quick hops along the rows.
  if (frame < OUTRO_AT) {
    const k = Math.floor((frame - MORE_AT) / CARD_LENGTH);
    const f = frame - MORE_AT - k * CARD_LENGTH;
    const here = on(cardSlot(k));
    const leave = CARD_LENGTH - CARD_TRAVEL;
    if (f < leave || k + 1 >= MORE_CARDS) return here;
    return hop(here, on(cardSlot(k + 1)), (f - leave) / CARD_TRAVEL, { zoom: 0.7, tilt: 10, turn: k % 2 ? -2 : 2 });
  }
  // The outro: up and away, tipped, then the whole sheet laid flat.
  const f = frame - OUTRO_AT;
  const lift: Shot = { x: SHEET.width / 2, y: SHEET.height * 0.55, zoom: WHOLE.zoom * 1.25, tilt: 38, turn: -8 };
  if (f < 100) return hop(on(cardSlot(MORE_CARDS - 1)), lift, f / 100, { zoom: 1, tilt: 0, turn: 0 });
  const t = interpolate(f, [100, 220], [0, 1], { extrapolateRight: 'clamp' });
  return hop(lift, WHOLE, t, { zoom: 1, tilt: 0, turn: 0 });
}

// --- which tiles play -------------------------------------------------------

/** A feature tile plays from just before the camera lands until it is well away; otherwise it rests on this frame. */
export const REST = 110;
export const featureLive = (i: number, frame: number) =>
  frame >= chapterAt(i) - CUT && frame < chapterAt(i) + CHAPTER_LENGTH + TRAVEL;
export const cardLive = (k: number, frame: number) =>
  frame >= MORE_AT + k * CARD_LENGTH - CARD_TRAVEL && frame < MORE_AT + (k + 1) * CARD_LENGTH + CARD_TRAVEL;
