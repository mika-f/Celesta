import { Easings, Group, Rect, Span, Text, interpolate, progress, useCurrentFrame } from '@celesta.studio/react';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { C, FONT, W, solid } from '../components/theme';
import { COMMITS, PRS } from './prs';
import { Rack, diveCamera } from './rack';
import { DeltaTheme } from './theme';
import { BAR, CHAPTER_LENGTH, FEATURES, step } from './timeline';

// The open, four bars on the rack. Bar 1: the rails, the empty slots, and
// the core powering up. Bar 2: the fourteen modules and MORE slide in, one a
// sixteenth. Bar 3: the title, and the cables go in one a sixteenth, each
// module lighting as its input lands. Bar 4: the camera goes into the first
// module, which is the first chapter by the downbeat.

const SEAT = BAR;
const PATCH = 2 * BAR;
const DIVE = 3 * BAR;

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

/** The core's screen: the wordmark and the release. */
export function CoreScreen({ frame }: { frame: number }) {
  const rise = progress(frame, 0, 14, Easings.easeOutExpo);
  return (
    <Group>
      <Backdrop />
      <Delta x={960} y={330} size={300} fill={C.orange} rotation={(1 - rise) * -120} />
      <Text x={960} y={560} anchorX={0.5} opacity={rise} style={{ fontFamily: FONT.display, fontSize: 260, fill: solid(C.paper) }}>CELESTA</Text>
      <Text x={960} y={840} anchorX={0.5} opacity={rise} style={{ fontFamily: FONT.mono, fontSize: 80, fontWeight: 700, fill: solid(C.orange) }}>0.10.0</Text>
    </Group>
  );
}

/** The title over the rack: the release, the line, the numbers. */
function Title({ frame }: { frame: number }) {
  const rise = progress(frame, PATCH, 14, Easings.easeOutExpo);
  const out = interpolate(frame, [DIVE, DIVE + 14], [1, 0], clamp);
  const pre = progress(frame, 6, 14);
  return (
    <Group opacity={out}>
      <Text x={84} y={70} opacity={pre} style={{ fontFamily: FONT.mono, fontSize: 30, fontWeight: 700, fill: solid(C.grey) }}>
        {`CELESTA   v0.9.0 → v0.10.0`}
      </Text>
      <Group x={74} y={116} clip={{ x: -20, y: 0, width: 1820, height: 230 }}>
        <Text y={230 * (1 - rise)} style={{ fontFamily: FONT.display, fontSize: 230, letterSpacing: { em: 0.01 }, fill: solid(C.paper) }}>
          DELTA <Span style={{ fill: C.orange }}>0.10.0</Span> PATCH
        </Text>
      </Group>
      <Text x={86} y={358} opacity={progress(frame, PATCH + 8, 12)}
        style={{ fontFamily: FONT.mono, fontSize: 30, fill: solid(C.grey) }}>
        {`${PRS.length} pull requests · ${COMMITS} commits · ${FEATURES} modules patched in`}
      </Text>
    </Group>
  );
}

const TICKER = { y: 968, size: 22, gap: 64, speed: 11 } as const;
/** A title's width in the ticker: JetBrains Mono's 0.6 em, a full em for Japanese. */
const widthOf = (text: string) => [...text].reduce((w, ch) => w + (ch > '\u2000' ? 1 : 0.6), 0) * TICKER.size;
const ITEMS = PRS.map(([n, title]) => `#${n}  ${title}`);
const OFFSETS = ITEMS.reduce<number[]>((xs, item, i) => [...xs, i === 0 ? 0 : xs[i - 1] + widthOf(ITEMS[i - 1]) + TICKER.gap], []);

/** The pull requests running along the bottom, from the second bar; only those on screen are drawn. */
function Ticker({ frame }: { frame: number }) {
  if (frame < SEAT) return null;
  const shift = W - (frame - SEAT) * TICKER.speed;
  const out = interpolate(frame, [DIVE, DIVE + 10], [1, 0], clamp);
  return (
    <Group opacity={out}>
      <Rect y={TICKER.y - 20} width={W} height={TICKER.size + 40} fill="panel" />
      <Rect y={TICKER.y - 20} width={W} height={1} fill="line" />
      {ITEMS.map((item, i) => {
        const x = shift + OFFSETS[i];
        if (x > W || x + widthOf(item) < 0) return null;
        return <Text key={item} x={x} y={TICKER.y} style={{ fontFamily: FONT.mono, fontSize: TICKER.size, fill: solid(C.grey) }}>{item}</Text>;
      })}
    </Group>
  );
}

export function Open() {
  const frame = useCurrentFrame();
  const dive = interpolate(frame, [DIVE, DIVE + BAR], [0, 1], { ...clamp, easing: Easings.easeInOutCubic });
  const camera = diveCamera(0, dive);
  const seatedAt = (slot: number) => (slot === 0 ? 4 : SEAT + step(slot - 1));
  const patchedAt = (slot: number) => PATCH + 4 + step(slot);
  return (
    <DeltaTheme>
      <Backdrop />
      <Group {...camera}>
        <Rack
          rails={interpolate(frame, [0, 20], [0, 1], { ...clamp, easing: Easings.easeOutCubic })}
          seated={(slot) => interpolate(frame, [seatedAt(slot), seatedAt(slot) + 6], [0, 1], { ...clamp, easing: Easings.easeOutCubic })}
          power={(slot) => (slot === 0 ? progress(frame, 12, 6)
            : interpolate(frame, [patchedAt(slot - 1) + 3, patchedAt(slot - 1) + 6], [0, 1], clamp))}
          patch={(slot) => interpolate(frame, [patchedAt(slot), patchedAt(slot) + 3], [0, 1], clamp)}
          // Each lit module runs its demo from where it lit; the first holds
          // frame 0 through the dive, as its chapter starts there.
          demoFrame={(i) => (i === 0 && frame >= DIVE ? 0 : Math.max(0, frame - patchedAt(i)) % CHAPTER_LENGTH)}
          core={<CoreScreen frame={frame - 12} />}
          sway={frame >= PATCH ? 8 * Math.sin((frame - PATCH) * 0.3) * Math.exp(-(frame - PATCH) / 40) : 0}
        />
      </Group>
      <Title frame={frame} />
      <Ticker frame={frame} />
    </DeltaTheme>
  );
}
