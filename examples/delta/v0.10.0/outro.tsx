import { Easings, Group, Rect, Span, Text, interpolate, progress, useCurrentFrame } from '@celesta.studio/react';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { C, FONT, solid } from '../components/theme';
import { CARDS } from './More';
import { COMMITS, DEPENDENCY_UPDATES, PRS } from './prs';
import { Rack, pullCamera } from './rack';
import { scoreAt } from './score';
import { DeltaTheme } from './theme';
import { BAR, CHAPTER_LENGTH, DURATION, FEATURES, OUTRO_AT, step } from './timeline';

// The outro: the end card for a bar, filling the core's screen; then the
// camera pulls back out of that screen onto the rack, every module running
// and patched. On the last bar the modules power down, one a sixteenth from
// the end of the chain back to the core, and their screens hold a frame.
// The last frame, the whole rack, is the poster.

const PULL = BAR;
const HOLD_AT = 2 * BAR;
const OFF = 3 * BAR;
/** Where each module holds once powered down: a frame of its chapter that shows it best. */
const HOLD = [60, 50, 54, 90, 100, 90, 80, 100, 110, 90, 100, 70, 80, 100];

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

/** The score's bands as a row of bars, at a frame of the film. */
function Meter({ frame, x, y, width, height }: { frame: number; x: number; y: number; width: number; height: number }) {
  const { bands } = scoreAt(frame);
  const w = width / bands.length;
  return (
    <Group x={x} y={y}>
      {bands.map((level, i) => {
        const h = Math.max(3, level * height);
        return <Rect key={i} x={i * w} y={height - h} width={w - 4} height={h} fill={level > 0.72 ? C.orange : C.paper} opacity={0.3 + 0.7 * level} />;
      })}
    </Group>
  );
}

/** The end card: the release, the line, the numbers. `frame` is the film's frame. */
export function EndCard({ frame }: { frame: number }) {
  const f = frame - OUTRO_AT;
  const rise = progress(f, 0, 16, Easings.easeOutExpo);
  const info = [
    'v0.9.0  →  v0.10.0',
    `${PRS.length} pull requests  +  ${DEPENDENCY_UPDATES} dependency updates`,
    `${COMMITS} commits`,
    `${FEATURES} modules  +  ${CARDS.length} more  ·  120 BPM  ·  ${DURATION} frames`,
  ];
  return (
    <Group>
      <Backdrop />
      <Text x={116} y={150} style={{ fontFamily: FONT.mono, fontSize: 34, fontWeight: 700, fill: solid(C.grey) }}>CELESTA</Text>
      <Group x={104} y={200} clip={{ x: -20, y: 0, width: 1240, height: 240 }}>
        <Text y={240 * (1 - rise)} style={{ fontFamily: FONT.display, fontSize: 240, letterSpacing: { em: 0.01 }, fill: solid(C.paper) }}>
          DELTA <Span style={{ fill: C.orange }}>0.10.0</Span>
        </Text>
      </Group>
      <Text x={116} y={470} opacity={progress(f, 6, 14)}
        style={{ fontFamily: FONT.sans, fontSize: 72, fontWeight: 700, fill: solid(C.paper) }}>Every update is a diff.</Text>
      {info.map((line, i) => (
        <Text key={line} x={120} y={630 + i * 48} opacity={progress(f, 10 + i * 3, 12)}
          style={{ fontFamily: FONT.mono, fontSize: 30, fontWeight: i === 0 ? 700 : 400, fill: solid(i === 0 ? C.orange : C.grey) }}>{line}</Text>
      ))}
      <Group x={1500} y={430}>
        <Delta size={300} fill={C.orange} />
      </Group>
      <Meter frame={frame} x={1290} y={650} width={420} height={180} />
      <Text x={120} y={940} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.dim) }}>
        {'celesta.natsuneko.cat   ·   github.com/mika-f/Celesta   ·   examples/delta'}
      </Text>
    </Group>
  );
}

/** The title over the rack once pulled back. */
function RackTitle({ f }: { f: number }) {
  const rise = progress(f, HOLD_AT - 10, 14, Easings.easeOutExpo);
  return (
    <Group>
      <Group x={74} y={116} clip={{ x: -20, y: 0, width: 1820, height: 230 }}>
        <Text y={230 * (1 - rise)} style={{ fontFamily: FONT.display, fontSize: 230, letterSpacing: { em: 0.01 }, fill: solid(C.paper) }}>
          DELTA <Span style={{ fill: C.orange }}>0.10.0</Span> PATCH
        </Text>
      </Group>
      <Text x={86} y={358} opacity={progress(f, HOLD_AT, 12)} style={{ fontFamily: FONT.mono, fontSize: 30, fill: solid(C.grey) }}>
        {f >= OFF + 2 * 15 ? 'POWER OFF   ·   every update is a diff' : `${FEATURES} modules patched in   ·   ${PRS.length} pull requests`}
      </Text>
    </Group>
  );
}

export function Outro() {
  const frame = useCurrentFrame();
  const f = frame - OUTRO_AT;
  const pull = interpolate(f, [PULL, HOLD_AT], [0, 1], { ...clamp, easing: Easings.easeInOutCubic });
  const camera = pullCamera(pull);
  // Slot s powers down at OFF + a sixteenth for each slot after it; the core last.
  const offAt = (slot: number) => OFF + step(15 - slot);
  return (
    <DeltaTheme>
      <Backdrop />
      <Group {...camera}>
        <Rack rails={1} seated={() => 1} patch={() => 1}
          power={(slot) => (slot === 0 ? 1 : interpolate(f, [offAt(slot), offAt(slot) + 2], [1, 0.35], clamp))}
          demoFrame={(i) => (f >= offAt(i + 1) ? HOLD[i] : (HOLD[i] + f - offAt(i + 1) + CHAPTER_LENGTH * 4) % CHAPTER_LENGTH)}
          core={<EndCard frame={frame} />} />
      </Group>
      {pull > 0.98 ? <RackTitle f={f} /> : null}
    </DeltaTheme>
  );
}
