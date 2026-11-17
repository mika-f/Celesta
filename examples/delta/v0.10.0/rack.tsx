import type { ReactNode } from 'react';
import { Group, Rect, Text } from '@celesta.studio/react';
import type { PathCommand } from '@celesta.studio/shapes';
import { Path } from '@celesta.studio/shapes';
import { C, FONT, W, solid } from '../components/theme';
import { CHAPTERS } from './chapters';
import { Faceplate, Jack, Led, MODULE, SCREEN, JACK_Y, jackX } from './module';
import { ChapterModule } from './program';
import { CABLE } from './theme';
import { FEATURES, blockOf } from './timeline';

// The rack: sixteen slots in two rows of eight, the core first and MORE
// last, each module the program's faceplate at a fifth of its size, each
// patched from its output (bottom right) to the next one's input (bottom
// left); the cable from the end of the top row runs back between the rails
// to the start of the bottom one. The open fills the rack, patches it, and
// dives into the first module; the outro pulls back out of the core's
// screen onto it.

export const SLOTS = 16;
export const SLOT_W = 210;
/** The rack draws the program's faceplate at this scale. */
export const K = SLOT_W / MODULE.width;
export const SLOT_H = MODULE.height * K;
const PITCH = 222;
const RACK_X = (W - (8 * PITCH - 12)) / 2;
export const ROW_Y = [452, 452 + SLOT_H + 72] as const;

/** A slot's top-left, in frame coordinates. */
export function slotAt(slot: number) {
  return { x: RACK_X + (slot % 8) * PITCH, y: ROW_Y[slot < 8 ? 0 : 1] };
}

/** The gap between the rows' rails, where the wrapping cable runs. */
const BETWEEN = (ROW_Y[0] + SLOT_H + 40 + ROW_Y[1] - 16) / 2;

/**
 * The camera that puts rectangle `from` (frame coordinates) at rectangle
 * `to`, `p` of the way: the scale grows geometrically, so the move keeps one
 * speed, and the rectangle travels in step with it.
 */
export function cameraOnto(from: { x: number; y: number; width: number }, to: { x: number; y: number; width: number }, p: number) {
  const k = to.width / from.width;
  const scale = k ** p;
  const q = k === 1 ? p : (scale - 1) / (k - 1);
  const x = from.x + (to.x - from.x) * q;
  const y = from.y + (to.y - from.y) * q;
  return { x: x - from.x * scale, y: y - from.y * scale, scale };
}

/** The camera from the rack to module `index`'s slot filling the program's faceplate. */
export function diveCamera(index: number, p: number) {
  const slot = slotAt(index + 1);
  return cameraOnto({ ...slot, width: SLOT_W }, { x: MODULE.x, y: MODULE.y, width: MODULE.width }, p);
}

/** The camera from the core's screen filling the frame (p = 0) back to the rack (p = 1). */
export function pullCamera(p: number) {
  const slot = slotAt(0);
  const screen = { x: slot.x + SCREEN.x * K, y: slot.y + SCREEN.y * K, width: SCREEN.width * K };
  return cameraOnto(screen, { x: 0, y: 0, width: W }, 1 - p);
}

const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

/** The core: Celesta itself, its screen showing `screen`, a 1920×1080 picture. */
export function CoreModule({ power, screen }: { power: number; screen: ReactNode }) {
  return (
    <Faceplate width={MODULE.width} height={MODULE.height} fill="metal">
      <Rect x={44} y={20} width={8} height={24} fill="paper" />
      <Text x={66} y={33} anchorY={0.5} style={mono(22, C.paper, 700)}>CELESTA</Text>
      <Text x={196} y={33} anchorY={0.5} style={mono(22, C.grey)}>CORE 0.10.0</Text>
      <Led x={MODULE.width - 118} y={33} on={power} color={C.orange} />
      <Text x={MODULE.width - 98} y={33} anchorY={0.5} style={mono(18, power > 0.5 ? C.orange : C.dim, 700)}>PWR</Text>
      <Rect x={SCREEN.x - 6} y={SCREEN.y - 6} width={SCREEN.width + 12} height={SCREEN.height + 12} fill="hole" stroke="line" strokeWidth={2} />
      <Group x={SCREEN.x} y={SCREEN.y} clip={{ width: SCREEN.width, height: SCREEN.height }}>
        <Group opacity={power} scale={SCREEN.width / W}>{screen}</Group>
      </Group>
      <Text x={SCREEN.x} y={SCREEN.y + SCREEN.height + 30} style={{ fontFamily: FONT.display, fontSize: 96, fill: solid(C.paper) }}>
        REACT · RUST · WGPU
      </Text>
      {['scene', 'audio'].map((label, n) => <Jack key={label} x={jackX('out', n)} y={JACK_Y} label={label} />)}
    </Faceplate>
  );
}

/** The last slot: MORE, twelve cards. */
export function MoreModule({ power }: { power: number }) {
  return (
    <Faceplate width={MODULE.width} height={MODULE.height}>
      <Rect x={44} y={20} width={8} height={24} fill="grey" />
      <Text x={66} y={33} anchorY={0.5} style={mono(22, C.paper, 700)}>MORE</Text>
      <Led x={MODULE.width - 118} y={33} on={power} />
      <Rect x={SCREEN.x - 6} y={SCREEN.y - 6} width={SCREEN.width + 12} height={SCREEN.height + 12} fill="hole" stroke="line" strokeWidth={2} />
      <Group x={SCREEN.x} y={SCREEN.y} opacity={power}>
        <Rect width={SCREEN.width} height={SCREEN.height} fill="orange" />
        <Text x={60} y={60} style={{ fontFamily: FONT.display, fontSize: 380, fill: solid(C.ink) }}>+12</Text>
      </Group>
      {['in'].map((label, n) => <Jack key={label} x={jackX('in', n)} y={JACK_Y} label={label} />)}
    </Faceplate>
  );
}

/** Where a slot's output and the next slot's input are, in frame coordinates. */
function jacks(slot: number) {
  const a = slotAt(slot);
  const b = slotAt(slot + 1);
  return {
    out: [a.x + jackX('out', 0) * K, a.y + JACK_Y * K] as const,
    in: [b.x + jackX('in', 0) * K, b.y + JACK_Y * K] as const,
  };
}

/** The cable from slot `slot` to the next, drawn on as `p` goes 0 → 1. */
function Patch({ slot, p, sway }: { slot: number; p: number; sway: number }) {
  if (p <= 0) return null;
  const { out, in: into } = jacks(slot);
  const color = slot + 1 > FEATURES ? C.paper : CABLE[blockOf(slot).cable];
  const commands: PathCommand[] = slot === 7 ? [
    // Back along the gap between the rows, then down into the bottom row's first input.
    { type: 'moveTo', x: out[0], y: out[1] },
    { type: 'cubicTo', x1: out[0] + 40, y1: out[1] + 30, x2: out[0] + 30, y2: BETWEEN, x: out[0] - 20, y: BETWEEN },
    { type: 'lineTo', x: into[0] + 40, y: BETWEEN + sway * 0.3 },
    { type: 'cubicTo', x1: into[0] - 30, y1: BETWEEN, x2: into[0] - 30, y2: into[1] + 40, x: into[0], y: into[1] },
  ] : [
    { type: 'moveTo', x: out[0], y: out[1] },
    { type: 'cubicTo', x1: out[0] + 6 + sway, y1: out[1] + 26, x2: into[0] - 6 + sway, y2: into[1] + 26, x: into[0], y: into[1] },
  ];
  return (
    <Group>
      <Path commands={commands} stroke="#060708" strokeWidth={7} cap="round" progress={p} />
      <Path commands={commands} stroke={color} strokeWidth={4.5} cap="round" progress={p} />
      <Rect x={out[0] - 3} y={out[1] - 2} width={6} height={9} fill={color} />
      {p >= 1 ? <Rect x={into[0] - 3} y={into[1] - 2} width={6} height={9} fill={color} /> : null}
    </Group>
  );
}

export interface RackState {
  /** 0–1: the rails drawn on. */
  rails: number;
  /** 0–1 per slot: the module in its slot (0: not yet, sliding in between). */
  seated: (slot: number) => number;
  /** 0–1 per slot: LED and screen. */
  power: (slot: number) => number;
  /** 0–1 per slot: the cable from it to the next drawn on. */
  patch: (slot: number) => number;
  /** The demo frame module `index` shows. */
  demoFrame: (index: number) => number;
  /** The core's screen, a 1920×1080 picture. */
  core: ReactNode;
  /** The cables' sway, in pixels. */
  sway?: number;
}

/** The rack in frame coordinates (put it under a camera to move through it). */
export function Rack({ rails, seated, power, patch, demoFrame, core, sway = 0 }: RackState) {
  const width = 8 * PITCH - 12 + 60;
  return (
    <Group>
      {ROW_Y.map((y) => (
        <Group key={y} x={RACK_X - 30}>
          {[y - 16, y + SLOT_H + 4].map((ry) => (
            <Group key={ry}>
              <Rect y={ry} width={width * rails} height={12} fill="rail" />
              {Array.from({ length: Math.floor((width * rails) / 24) }, (_, i) => (
                <Rect key={i} x={10 + i * 24} y={ry + 4} width={6} height={4} fill="hole" />
              ))}
            </Group>
          ))}
        </Group>
      ))}
      {Array.from({ length: SLOTS }, (_, slot) => {
        const { x, y } = slotAt(slot);
        const s = seated(slot);
        return (
          <Group key={slot} x={x} y={y}>
            <Rect width={SLOT_W} height={SLOT_H} fill="#08090BCC" stroke="line" strokeWidth={1} opacity={rails} />
            {s > 0 ? (
              <Group y={(1 - s) * 70} opacity={Math.min(1, s * 1.5)} scale={K}>
                {slot === 0 ? <CoreModule power={power(0)} screen={core} />
                  : slot > FEATURES ? <MoreModule power={power(slot)} />
                  : <ChapterModule chapter={CHAPTERS[slot - 1]} index={slot - 1} frame={demoFrame(slot - 1)} live={false} power={power(slot)} />}
              </Group>
            ) : null}
            <Text x={0} y={SLOT_H + 22} opacity={rails} style={mono(13, C.dim)}>
              {slot === 0 ? 'CORE' : slot > FEATURES ? 'MORE' : `Δ${String(slot).padStart(2, '0')} ${CHAPTERS[slot - 1].title}`}
            </Text>
          </Group>
        );
      })}
      {Array.from({ length: SLOTS - 1 }, (_, slot) => <Patch key={slot} slot={slot} p={patch(slot)} sway={sway} />)}
    </Group>
  );
}
