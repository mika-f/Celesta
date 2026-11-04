import { useContext } from 'react';
import { Easings, FreezeFrame, Group, MotionBlur, Rect, Text, Trail, interpolate, progress, useCurrentFrame } from '@celesta.studio/react';
import { chromaticAberration, scanlines } from '@celesta.studio/shader';
import { Path, interpolatePath } from '@celesta.studio/shapes';
import { Backdrop } from '../components/Backdrop';
import { PREVIEW } from '../components/ChapterShell';
import { glitch } from '../components/shaders';
import { C, FONT, ease, solid } from '../components/theme';
import { FilmContext } from './film-context';
import { useLayout } from './layout';
import { outlines } from './outlines';
import { PRS } from './prs';
import { scoreAt } from './score';
import { BAR, BEAT, chapterAt } from './timeline';

// Bars 1-2: the release's pull requests whip past under a motion blur.
// Bar 3: a Δ draws itself on and morphs through the new shapes, trailing.
// Bar 4: 0.7.0 as a mask, the chapters to come flashing through it.
// Bars 5-6: the orange DELTA card, glitched by a chain of shaders.
// Laid out for the film's 16:9 or the short's 9:16 (useLayout()).
export const TAGLINE = 'Every update is a diff.';

const ROW = 34;
const LOG_END = 2 * BAR - 4;

const scroll = (frame: number) => interpolate(frame, [0, LOG_END], [0, PRS.length - 1],
  { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });

function Log({ frame, cursorY, height }: { frame: number; cursorY: number; height: number }) {
  const at = scroll(frame);
  const row = Math.round(at);
  const mono = { fontFamily: FONT.mono, fontSize: 22 };
  return (
    <Group>
      {PRS.map(([n, title], i) => {
        const y = cursorY + (i - at) * ROW;
        if (y < -ROW || y > height + ROW) return null;
        const near = Math.max(0, 1 - Math.abs(i - at) / 12);
        const on = i === row;
        return (
          <Group key={n} y={y} opacity={on ? 1 : 0.16 + 0.5 * near}>
            {on ? <Text x={92} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(C.orange) }}>{'>'}</Text> : null}
            <Text x={120} anchorY={0.5} style={{ ...mono, fill: solid(on ? C.orange : C.dim) }}>{`#${n}`}</Text>
            <Text x={210} anchorY={0.5} style={{ ...mono, fill: solid(on ? C.paper : C.grey) }}>{title}</Text>
          </Group>
        );
      })}
    </Group>
  );
}

function Counter({ frame }: { frame: number }) {
  const layout = useLayout();
  const row = Math.round(scroll(frame));
  const p = 1 - progress(frame, LOG_END - 6, 10);
  const mono = { fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) };
  // The film: a panel on the right. The short: a band across the lower half.
  const box = layout.portrait
    ? { x: 0, y: 1120, width: layout.width, height: layout.height - 1120, right: layout.width - layout.safe.right, top: 1200 }
    : { x: 1290, y: 0, width: 630, height: layout.height, right: 1800, top: 480 };
  return (
    <Group opacity={p}>
      <Rect x={box.x} y={box.y} width={box.width} height={box.height} fill="#0C0D0FEE" />
      <Text x={box.right} y={box.top} anchorX={1} anchorY={0.5} style={mono}>PULL REQUESTS SINCE v0.6.0</Text>
      <Text x={box.right} y={box.top + 140} anchorX={1} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: 220, fill: solid(C.orange) }}>{`#${PRS[row][0]}`}</Text>
      <Text x={box.right} y={box.top + 270} anchorX={1} anchorY={0.5} style={mono}>{`${row + 1} / ${PRS.length} merged`}</Text>
    </Group>
  );
}

// --- bar 3: the shapes -------------------------------------------------------

const SHAPE = outlines(380);
const SEQUENCE = [SHAPE.delta, SHAPE.star, SHAPE.heart, SHAPE.hexagon, SHAPE.delta];
const SHAPES_AT = 2 * BAR;

/** The outline at a (fractional) frame: one morph a beat, each in six frames. */
function outlineAt(frame: number) {
  const beat = Math.max(0, Math.min(SEQUENCE.length - 2, Math.floor((frame - SHAPES_AT - BEAT) / BEAT)));
  const t = progress(frame, SHAPES_AT + BEAT * (beat + 1), 6, Easings.easeInOutCubic);
  return interpolatePath(SEQUENCE[beat], SEQUENCE[beat + 1], t);
}

function Shapes({ frame }: { frame: number }) {
  const layout = useLayout();
  if (frame < SHAPES_AT - 2 || frame >= 3 * BAR + 2) return null;
  const draw = progress(frame, SHAPES_AT, BEAT, Easings.easeOutCubic);
  const out = progress(frame, 3 * BAR - 4, 6);
  const center = { x: layout.width / 2, y: layout.height / 2 + 20 };
  return (
    <Group opacity={1 - out}>
      <Trail copies={6} lagInFrames={1} trailOpacity={0.6}>
        {(moment) => (
          <Path x={center.x} y={center.y} commands={outlineAt(moment)} progress={draw} closed
            stroke={C.orange} strokeWidth={8} join="round" dash={[28, 14]} dashOffset={-moment * 6}
            glow={{ color: '#F26A2EAA', blur: 24 }} />
        )}
      </Trail>
      <Text x={center.x} y={center.y + 290} anchorX={0.5} anchorY={0.5}
        opacity={progress(frame, SHAPES_AT + BEAT, 6)}
        style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.grey) }}>
        {['<Path progress>', '<Star>', '<Heart>', '<Polygon>', 'interpolatePath()'][Math.max(0, Math.min(4, Math.floor((frame - SHAPES_AT - 3) / BEAT)))]}
      </Text>
    </Group>
  );
}

// --- bar 4: the teaser -------------------------------------------------------

const TEASER_AT = 3 * BAR;
/** One chapter a beat, through the letters: transitions, effects, blend, the spectrum. */
const TEASED = [0, 3, 5, 8].map((i) => chapterAt(i) + 70);
const PREVIEW_CENTER = { x: PREVIEW.x + PREVIEW.width / 2, y: PREVIEW.y + PREVIEW.height / 2 };

function Teaser({ frame }: { frame: number }) {
  const Film = useContext(FilmContext);
  const layout = useLayout();
  if (frame < TEASER_AT || frame >= 4 * BAR + 2) return null;
  const f = frame - TEASER_AT;
  const beat = Math.min(3, Math.floor(f / BEAT));
  const hit = 1 - progress(f % BEAT, 0, 6);
  const scale = interpolate(f, [0, BAR], [1.18, 1], { easing: Easings.easeOutCubic, extrapolateRight: 'clamp' });
  const word = { fontFamily: FONT.display, fontSize: layout.portrait ? 420 : 640, letterSpacing: { em: 0.02 } } as const;
  const center = { x: layout.width / 2, y: layout.height / 2 };
  // The chapter's preview panel, magnified to fill the letters.
  const zoom = 1.7;
  return (
    <Group x={center.x} y={center.y} scale={scale}
      shader={[chromaticAberration({ offset: 2 + 12 * hit }), scanlines({ amount: 0.3, spacing: 5 })]}>
      <Text anchorX={0.5} anchorY={0.5} style={{ ...word, fill: solid('#2A170F'), stroke: { paint: solid(C.orange), width: 3 } }}>0.7.0</Text>
      <Group mask={<Text anchorX={0.5} anchorY={0.5} style={{ ...word, fill: solid('#FFFFFF') }}>0.7.0</Text>}>
        <Group x={-PREVIEW_CENTER.x * zoom} y={-PREVIEW_CENTER.y * zoom} scale={zoom}>
          <FreezeFrame frame={TEASED[beat]}><Film /></FreezeFrame>
        </Group>
        <Rect x={-center.x} y={-center.y} width={layout.width} height={layout.height} fill="#F26A2E" opacity={0.3 * hit} blendMode="screen" />
      </Group>
    </Group>
  );
}

// --- bars 5-6: the card ------------------------------------------------------

const CARD_AT = 4 * BAR;

function Card({ frame, chapters }: { frame: number; chapters: number }) {
  const layout = useLayout();
  if (frame < CARD_AT) return null;
  const f = frame - CARD_AT;
  const settle = 1 - progress(f, 2, 24, Easings.easeOutCubic);
  // Bar 6 hits again, and the last beat (silence in the score) flips the card.
  const again = f >= BAR ? 1 - progress(f - BAR, 0, 14, Easings.easeOutCubic) : 0;
  const silence = f >= BAR + 3 * BEAT;
  const kick = scoreAt(frame).rms;
  const mono = { fontFamily: FONT.mono, fontSize: 22, fill: solid(C.ink) };
  const line = ease(f, 10, 18);
  const amount = 90 * settle + 70 * again;
  // Centred on the frame, not the safe area, which would pull it up and left.
  const x = layout.width / 2;
  // Rows of the card, from its middle: the meta line, DELTA, the tagline, the versions.
  const mid = layout.portrait ? layout.height / 2 - 60 : 520;
  const spacing = layout.portrait ? '  ·  ' : '   ·   ';
  return (
    <Group invert={silence ? 1 : 0}
      zoomBlur={silence ? { amount: progress(f - BAR - 3 * BEAT, 0, BEAT) * 0.5 } : undefined}
      shader={[
        glitch({ amount, split: 14 * (settle + again), seed: Math.floor(f / 3) }),
        chromaticAberration({ offset: 3 + 10 * kick }),
        scanlines({ amount: 0.18, spacing: 4 }),
      ]}>
      <Rect width={layout.width} height={layout.height} fill={C.orange} />
      <Text x={x} y={mid - 220} anchorX={0.5} anchorY={0.5} opacity={line} style={{ ...mono, fontWeight: 700 }}>
        {['CELESTA', "WHAT'S NEW", `${chapters} FEATURES`, `${PRS.length} PRS`].join(spacing)}
      </Text>
      <Text x={x} y={mid} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: layout.portrait ? 300 : 360, letterSpacing: { em: 0.02 + 0.12 * settle }, fill: solid(C.ink) }}>
        DELTA
      </Text>
      <Text x={x} y={mid + 210} anchorX={0.5} anchorY={0.5} opacity={line}
        style={{ fontFamily: FONT.sans, fontSize: 40, fontWeight: 700, fill: solid(C.ink) }}>
        {TAGLINE}
      </Text>
      <Text x={x} y={mid + 280} anchorX={0.5} anchorY={0.5} opacity={line} style={mono}>
        v0.6.0   →   v0.7.0
      </Text>
    </Group>
  );
}

export function Open({ chapters }: { chapters: number }) {
  const layout = useLayout();
  const frame = useCurrentFrame();
  const { rms } = scoreAt(frame);
  const logOut = 1 - progress(frame, LOG_END - 6, 10);
  const cursorY = layout.portrait ? 760 : 540;
  return (
    <Group>
      <Backdrop width={layout.width} height={layout.height} />
      {frame < LOG_END + 6 ? (
        <Group opacity={logOut} shader={[chromaticAberration({ offset: 1 + 16 * rms + 30 * (1 - progress(frame, 0, 2 * BEAT)) })]}>
          <MotionBlur shutterAngle={360} samples={10}>
            {(moment) => <Log frame={moment} cursorY={cursorY} height={layout.height} />}
          </MotionBlur>
        </Group>
      ) : null}
      <Counter frame={frame} />
      {/* The first downbeat: a flash over the sub boom. */}
      {frame < BEAT ? <Rect width={layout.width} height={layout.height} fill={C.paper} opacity={0.9 * (1 - progress(frame, 0, BEAT, Easings.easeOutCubic))} /> : null}
      <Shapes frame={frame} />
      <Teaser frame={frame} />
      <Card frame={frame} chapters={chapters} />
    </Group>
  );
}
