import { Easings, Group, Instances, Rect, Text, interpolate, progress, useCurrentFrame } from '@celesta.studio/react';
import { random } from '@celesta.studio/math';
import { Backdrop } from '../components/Backdrop';
import { Delta } from '../components/Delta';
import { glitch } from '../components/shaders';
import { C, FONT, W, H, ease, solid } from '../components/theme';
import { PRS } from './prs';

// Bar 1: the release's pull requests scroll by and the counter runs up.
// Bar 2: the log breaks into particles that gather into the Δ.
// Bar 3: the Δ swells into an orange card and DELTA glitches into place.
export const OPEN_LENGTH = 180;
export const TAGLINE = 'Every update is a diff.';

const ROW = 34;
const CURSOR_Y = 520;

function rowAt(frame: number) {
  return Math.round(interpolate(frame, [0, 58], [0, PRS.length - 1],
    { easing: Easings.easeInOutQuad, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' }));
}

function Log({ frame }: { frame: number }) {
  const at = interpolate(frame, [0, 58], [0, PRS.length - 1],
    { easing: Easings.easeInOutQuad, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const row = rowAt(frame);
  const fade = 1 - progress(frame, 52, 14);
  const mono = { fontFamily: FONT.mono, fontSize: 21 };
  return (
    <Group opacity={fade}>
      {PRS.map(([n, title], i) => {
        const y = CURSOR_Y + (i - at) * ROW;
        if (y < -ROW || y > H + ROW) return null;
        const near = Math.max(0, 1 - Math.abs(i - at) / 14);
        const on = i === row;
        return (
          <Group key={n} y={y} opacity={on ? 1 : 0.18 + 0.5 * near}>
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
  const row = rowAt(frame);
  const p = ease(frame, 0, 14) * (1 - progress(frame, 52, 14));
  const mono = { fontFamily: FONT.mono, fontSize: 18, fill: solid(C.grey) };
  return (
    <Group opacity={p}>
      <Rect x={1290} y={0} width={630} height={H} fill="#0C0D0FEE" />
      <Text x={1800} y={500} anchorX={1} anchorY={0.5} style={mono}>PULL REQUESTS SINCE v0.5.0</Text>
      <Text x={1800} y={640} anchorX={1} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: 220, fill: solid(C.orange) }}>{`#${PRS[row][0]}`}</Text>
      <Text x={1800} y={770} anchorX={1} anchorY={0.5} style={mono}>{`${row + 1} / ${PRS.length} merged`}</Text>
    </Group>
  );
}

// The particles leave from where the log was and land inside the Δ.
const N = 7000;
const SIZE = 420;
const CENTER = { x: W / 2, y: 470 };
const from = { x: new Float32Array(N), y: new Float32Array(N) };
const to = { x: new Float32Array(N), y: new Float32Array(N) };
const delay = new Float32Array(N);
const xs = new Float32Array(N);
const ys = new Float32Array(N);
const colors = new Uint8Array(N * 4);
for (let i = 0; i < N; i++) {
  from.x[i] = 120 + random(`open-x-${i}`) * 1500;
  from.y[i] = CURSOR_Y + (random(`open-y-${i}`) - 0.5) * 900;
  // An even spread over the triangle: fold the unit square in half.
  let u = random(`open-u-${i}`);
  let v = random(`open-v-${i}`);
  if (u + v > 1) { u = 1 - u; v = 1 - v; }
  const h = SIZE * Math.sqrt(3) / 2;
  const a = { x: 0, y: -h / 2 };
  const b = { x: SIZE / 2, y: h / 2 };
  const c = { x: -SIZE / 2, y: h / 2 };
  to.x[i] = CENTER.x + a.x + u * (b.x - a.x) + v * (c.x - a.x);
  to.y[i] = CENTER.y + a.y + u * (b.y - a.y) + v * (c.y - a.y);
  delay[i] = random(`open-d-${i}`) * 16;
  const warm = random(`open-c-${i}`);
  colors.set([242, Math.round(106 + warm * 80), Math.round(46 + warm * 30), 150 + Math.round(warm * 105)], i * 4);
}

function Gather({ frame }: { frame: number }) {
  if (frame < 50 || frame > 112) return null;
  for (let i = 0; i < N; i++) {
    const p = progress(frame, 52 + delay[i], 38, Easings.easeInOutCubic);
    const swirl = Math.sin(i * 0.7 + frame * 0.08) * 60 * Math.sin(Math.PI * p);
    xs[i] = from.x[i] + (to.x[i] - from.x[i]) * p + swirl;
    ys[i] = from.y[i] + (to.y[i] - from.y[i]) * p - swirl * 0.5;
  }
  const opacity = progress(frame, 50, 8) * (1 - progress(frame, 100, 10));
  return <Instances count={N} x={xs} y={ys} color={colors} shape="circle" width={4} height={4} opacity={opacity} />;
}

function Card({ frame }: { frame: number }) {
  if (frame < 120) return null;
  const f = frame - 120;
  const settle = 1 - progress(f, 4, 26, Easings.easeOutCubic);
  const mono = { fontFamily: FONT.mono, fontSize: 22, fill: solid(C.ink) };
  const line = ease(f, 14, 20);
  return (
    <Group shader={glitch({ amount: 90 * settle, split: 14 * settle, seed: Math.floor(f / 3) })}>
      <Rect width={W} height={H} fill={C.orange} />
      <Text x={W / 2} y={330} anchorX={0.5} anchorY={0.5} opacity={line} style={{ ...mono, fontWeight: 700 }}>
        CELESTA   ·   WHAT'S NEW   ·   12 FEATURES
      </Text>
      <Text x={W / 2} y={530} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: 340, letterSpacing: { em: 0.02 + 0.1 * settle }, fill: solid(C.ink) }}>
        DELTA
      </Text>
      <Text x={W / 2} y={730} anchorX={0.5} anchorY={0.5} opacity={line}
        style={{ fontFamily: FONT.sans, fontSize: 40, fontWeight: 700, fill: solid(C.ink) }}>
        {TAGLINE}
      </Text>
      <Text x={W / 2} y={800} anchorX={0.5} anchorY={0.5} opacity={line} style={mono}>
        v0.5.0   →   v0.6.0
      </Text>
    </Group>
  );
}

export function Open() {
  const frame = useCurrentFrame();
  // The Δ lands, holds a beat, then swells past the frame's corners.
  const solidIn = progress(frame, 96, 10);
  const swell = interpolate(frame, [104, 120], [1, 14], { easing: Easings.easeInExpo, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  return (
    <Group>
      <Backdrop />
      <Log frame={frame} />
      <Counter frame={frame} />
      <Gather frame={frame} />
      {frame >= 96 && frame < 122 ? (
        <Delta x={CENTER.x} y={CENTER.y} size={SIZE} opacity={solidIn} scale={swell}
          glow={{ color: '#F26A2E88', blur: 40 }} />
      ) : null}
      <Card frame={frame} />
    </Group>
  );
}
