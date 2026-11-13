// "Celesta-chan" — a concert-themed promo for Celesta, after the mascot
// launch videos of developer tools. Celesta-chan, an AI orchestra conductor,
// leads one show from the overture to the curtain call, and a note pops on
// every beat of the chart.
//
// 1920×1080 at 30 fps, cut to a 150 BPM score: one beat = 12 frames, one
// bar = 48 frames. The music and the beats that pop come from
// make-music.py (music.wav, hits.json); the portraits from ./chara.
import type { ReactNode } from 'react';

import {
  Assets,
  Audio,
  Composition,
  Easings,
  Font,
  Group,
  Image,
  Path,
  Rect,
  Sequence,
  Text,
  interpolate,
  noise,
  progress,
  random,
  spring,
  useCurrentFrame,
  useTextMetrics,
} from '@celesta/react';

import chart from './hits.json';

const W = 1920;
const H = 1080;
const FPS = 30;
const BEAT = 12;
const BAR = BEAT * 4;
const DURATION = BAR * 33;

// Scene starts, in bars.
const SCENE = {
  overture: 0,
  intro: 4,
  write: 6,
  layer: 10,
  render: 12,
  announce: 15,
  builtin: 19,
  beat: 21,
  stats: 24,
  bravo: 27,
  curtain: 29,
  end: 33,
} as const;
const at = (bar: number, beat = 0) => bar * BAR + beat * BEAT;

const C = {
  ink: '#0A0A0A',
  night: '#150F24',
  deep: '#241A3D',
  plum: '#3B2A6B',
  violet: '#57456C',
  lav: '#B5A2E7',
  lavSoft: '#E4DCF7',
  mist: '#F4F0FC',
  paper: '#EDEBE6',
  white: '#FFFFFF',
  pink: '#FF6FAE',
  pinkSoft: '#FFD3E6',
  gold: '#FFC94D',
  mint: '#5FDDB0',
  amber: '#FFB547',
  grey: '#8A8496',
} as const;

const FONT = {
  display: 'Dela Gothic One',
  round: 'M PLUS Rounded 1c',
  mono: 'JetBrains Mono',
  led: 'DotGothic16',
} as const;
const FONT_SRC =
  'https://fonts.googleapis.com/css2?family=Dela+Gothic+One&family=M+PLUS+Rounded+1c:wght@500;800&family=JetBrains+Mono:wght@400;700&family=DotGothic16&display=swap';

const CHARA = {
  conduct: './chara/full_conduct.png',
  jump: './chara/full_jump.png',
  wave: './chara/full_wave.png',
  point: './chara/bust_point.png',
  hi: './chara/bust_hi.png',
  wink: './chara/bust_wink.png',
  cheer: './chara/bust_cheer.png',
} as const;

// ------------------------------------------------------------------ chart

const HITS: number[] = chart.frames;
const LOUDNESS: number[] = chart.loudness;
// ------------------------------------------------------------------ helpers

const clamp = (n: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, n));
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
const pad = (n: number, width: number) => String(Math.floor(n)).padStart(width, '0');

type TProps = {
  children: string | number;
  x?: number;
  y?: number;
  size: number;
  font?: keyof typeof FONT;
  weight?: number;
  color?: string;
  fill?: any;
  stroke?: { color: string; width: number };
  ax?: number;
  ay?: number | 'baseline';
  opacity?: number;
  scale?: number;
  rotation?: number;
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;
  spacing?: number;
  glow?: { color: string; blur: number };
  shadow?: { color: string; blur: number; offsetX: number; offsetY: number };
};

function T({
  children, x = 0, y = 0, size, font = 'display', weight = 400, color = C.white, fill,
  stroke, ax = 0, ay = 0, opacity = 1, scale = 1, rotation = 0, align = 'left',
  lineHeight, spacing, glow, shadow,
}: TProps) {
  return (
    <Text x={x} y={y} anchorX={ax} anchorY={ay} opacity={opacity} scale={scale} rotation={rotation}
      glow={glow} shadow={shadow}
      style={{
        fontFamily: FONT[font], fontSize: size, fontWeight: weight, align, lineHeight,
        letterSpacing: spacing,
        fill: fill ?? { type: 'solid', color },
        stroke: stroke ? { paint: { type: 'solid', color: stroke.color }, width: stroke.width } : undefined,
      }}>
      {children}
    </Text>
  );
}

const vgrad = (h: number, top: string, bottom: string) => ({
  type: 'linear' as const, start: { x: 0, y: 0 }, end: { x: 0, y: h },
  stops: [{ offset: 0, color: top }, { offset: 1, color: bottom }],
});
const hgrad = (w: number, left: string, right: string) => ({
  type: 'linear' as const, start: { x: 0, y: 0 }, end: { x: w, y: 0 },
  stops: [{ offset: 0, color: left }, { offset: 1, color: right }],
});

function Background({ top, bottom }: { top: string; bottom: string }) {
  return <Rect width={W} height={H} fill={vgrad(H, top, bottom)} />;
}

// Celesta's mark: a crescent "C" with a lavender dot, after the app icon.
const CRESCENT: [number, number][] = (() => {
  const pts: [number, number][] = [];
  const c1 = { x: 34.46, y: 32 }, r1 = 16;
  const a0 = Math.atan2(17.68 - 32, 41.6 - c1.x);
  for (let i = 0; i <= 40; i++) {
    const a = a0 - (i / 40) * (2 * Math.PI + 2 * a0);
    pts.push([c1.x + r1 * Math.cos(a), c1.y + r1 * Math.sin(a)]);
  }
  for (let i = 0; i <= 24; i++) {
    const a = Math.PI / 2 + (i / 24) * Math.PI;
    pts.push([41.6 + 14.32 * Math.cos(a), 32 + 14.32 * Math.sin(a)]);
  }
  return pts.map(([x, y]) => [x - 32, y - 32] as [number, number]);
})();

function Mark({ x, y, size, color = C.paper, dot = C.lav, opacity = 1, rotation = 0 }:
  { x: number; y: number; size: number; color?: string; dot?: string; opacity?: number; rotation?: number }) {
  const s = size / 32;
  return (
    <Group x={x} y={y} opacity={opacity} rotation={rotation}>
      <Path points={CRESCENT.map(([px, py]) => [px * s, py * s] as [number, number])} closed fill={color} />
      <Rect x={9 * s - 3 * s} y={-3 * s} width={6 * s} height={6 * s} cornerRadius={3 * s} fill={dot} />
    </Group>
  );
}

// Five-point star, centered on 0,0.
const starPoints = (r: number, inner = 0.45): [number, number][] =>
  Array.from({ length: 10 }, (_, i) => {
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    const rr = i % 2 === 0 ? r : r * inner;
    return [Math.cos(a) * rr, Math.sin(a) * rr] as [number, number];
  });

function Star({ x, y, r, color, opacity = 1, rotation = 0 }:
  { x: number; y: number; r: number; color: string; opacity?: number; rotation?: number }) {
  return <Path x={x} y={y} rotation={rotation} opacity={opacity} points={starPoints(r)} closed fill={color} />;
}

// Twinkling stars and notes drifting across a scene.
function Sparkles({ seed, count, colors, drift = 0.6, fall = 0, size = 14 }:
  { seed: string; count: number; colors: string[]; drift?: number; fall?: number; size?: number }) {
  const frame = useCurrentFrame();
  return (
    <Group>
      {Array.from({ length: count }, (_, i) => {
        const x0 = random(`${seed}-x-${i}`) * W;
        const y0 = random(`${seed}-y-${i}`) * H;
        const x = (x0 + frame * drift * (0.5 + random(`${seed}-v-${i}`)) + W) % W;
        const y = (y0 + frame * fall * (0.6 + random(`${seed}-f-${i}`))) % (H + 40) - 20;
        const tw = 0.35 + 0.65 * Math.abs(Math.sin(frame / (9 + random(`${seed}-t-${i}`) * 14) + i));
        const r = size * (0.4 + random(`${seed}-r-${i}`) * 0.8);
        return <Star key={i} x={x} y={y} r={r} rotation={frame * 2 + i * 40} opacity={tw}
          color={colors[i % colors.length]} />;
      })}
    </Group>
  );
}

// Lines bursting out from a point.
function SpeedLines({ cx, cy, count, seed, color, opacity, inner = 380 }:
  { cx: number; cy: number; count: number; seed: string; color: string; opacity: number; inner?: number }) {
  const frame = useCurrentFrame();
  const commands = [] as any[];
  for (let i = 0; i < count; i++) {
    const a = (i / count) * Math.PI * 2 + random(`${seed}-a-${i}-${Math.floor(frame / 2)}`) * 0.05;
    const r0 = inner + random(`${seed}-r-${i}-${Math.floor(frame / 2)}`) * 260;
    const w = 0.004 + random(`${seed}-w-${i}`) * 0.01;
    commands.push({ type: 'moveTo', x: cx + Math.cos(a) * r0, y: cy + Math.sin(a) * r0 });
    commands.push({ type: 'lineTo', x: cx + Math.cos(a - w) * 2400, y: cy + Math.sin(a - w) * 2400 });
    commands.push({ type: 'lineTo', x: cx + Math.cos(a + w) * 2400, y: cy + Math.sin(a + w) * 2400 });
    commands.push({ type: 'close' });
  }
  return <Path commands={commands} fill={color} opacity={opacity} />;
}

// Wedges of a sunburst; `colors` cycle around it.
function Sunburst({ cx, cy, rays, colors, rotation, opacity = 1 }:
  { cx: number; cy: number; rays: number; colors: string[]; rotation: number; opacity?: number }) {
  return (
    <Group x={cx} y={cy} rotation={rotation} opacity={opacity}>
      {colors.map((color, k) => {
        const commands = [] as any[];
        for (let i = k; i < rays; i += colors.length) {
          const a0 = (i / rays) * Math.PI * 2;
          const a1 = ((i + 1) / rays) * Math.PI * 2;
          commands.push({ type: 'moveTo', x: 0, y: 0 });
          commands.push({ type: 'lineTo', x: Math.cos(a0) * 2400, y: Math.sin(a0) * 2400 });
          commands.push({ type: 'lineTo', x: Math.cos(a1) * 2400, y: Math.sin(a1) * 2400 });
          commands.push({ type: 'close' });
        }
        return <Path key={color} commands={commands} fill={color} />;
      })}
    </Group>
  );
}

// A portrait that bounces in from below with a spring and keeps breathing.
function Portrait({ src, x, y, height, delay = 0, from = 'bottom', bob = 6, flip = false }:
  { src: string; x: number; y: number; height: number; delay?: number; from?: 'bottom' | 'left' | 'right'; bob?: number; flip?: boolean }) {
  const frame = useCurrentFrame();
  const p = spring({ frame, fps: FPS, delay, config: { damping: 13, stiffness: 140 } });
  const dx = from === 'left' ? -500 * (1 - p) : from === 'right' ? 500 * (1 - p) : 0;
  const dy = from === 'bottom' ? 420 * (1 - p) : 0;
  const breathe = Math.sin(frame / 14) * bob;
  return (
    <Image src={src} height={height} x={x + dx} y={y + dy + breathe} anchorX={0.5} anchorY={1}
      scaleX={flip ? -1 : 1} opacity={clamp(p * 2)} />
  );
}

type TailSide = 'left' | 'right' | 'down-left' | 'down-right';

// A comic speech bubble sized to its text. `text` is typed out over time.
function Bubble({ x, y, text, size = 52, tail, delay = 0, cps = 2, ax = 0, ay = 0, minWidth = 0 }:
  { x: number; y: number; text: string; size?: number; tail: TailSide; delay?: number; cps?: number; ax?: number; ay?: number; minWidth?: number }) {
  const frame = useCurrentFrame() - delay;
  const style = { fontFamily: FONT.round, fontSize: size, fontWeight: 800, lineHeight: size * 1.3, align: 'center' as const };
  const m = useTextMetrics(text, style);
  if (frame < 0) return null;
  const padX = size * 0.9;
  const padY = size * 0.6;
  const w = Math.max(minWidth, m.width + padX * 2);
  const h = m.height + padY * 2;
  const pop = spring({ frame, fps: FPS, config: { damping: 11, stiffness: 220 } });
  const shown = Array.from(text).slice(0, Math.floor(frame * cps)).join('');
  const t = 30;
  const tailCmds = {
    left: [[0, h * 0.42], [-t * 1.6, h * 0.55], [0, h * 0.62]],
    right: [[w, h * 0.42], [w + t * 1.6, h * 0.55], [w, h * 0.62]],
    'down-left': [[w * 0.18, h], [w * 0.12, h + t * 1.7], [w * 0.32, h]],
    'down-right': [[w * 0.68, h], [w * 0.86, h + t * 1.7], [w * 0.82, h]],
  }[tail] as [number, number][];
  return (
    <Group x={x - ax * w} y={y - ay * h} scale={0.6 + 0.4 * pop} opacity={clamp(pop * 3)}
      shadow={{ color: '#2A1E4A40', blur: 18, offsetX: 0, offsetY: 8 }}>
      <Rect x={-4} y={-4} width={w + 8} height={h + 8} cornerRadius={h / 2.6 + 4} fill={C.deep} />
      <Path points={tailCmds.map(([a, b]) => [a + (tail === 'left' ? -5 : tail === 'right' ? 5 : 0), b + (tail.startsWith('down') ? 6 : 0)] as [number, number])} closed fill={C.deep} />
      <Rect width={w} height={h} cornerRadius={h / 2.6} fill={C.white} />
      <Path points={tailCmds} closed fill={C.white} />
      <Text x={w / 2} y={padY} anchorX={0.5} style={{ ...style, fill: { type: 'solid', color: C.deep } }}>
        {shown.length ? shown : ' '}
      </Text>
    </Group>
  );
}

// A rounded pill with a measured label.
function Pill({ x, y, label, size = 26, bg = C.deep, color = C.white, ax = 0, font = 'round', weight = 800, opacity = 1, dot }:
  { x: number; y: number; label: string; size?: number; bg?: string; color?: string; ax?: number; font?: keyof typeof FONT; weight?: number; opacity?: number; dot?: string }) {
  const style = { fontFamily: FONT[font], fontSize: size, fontWeight: weight };
  const m = useTextMetrics(label, style);
  const dotW = dot ? size * 0.9 : 0;
  const w = m.width + size * 1.4 + dotW;
  const h = size * 1.7;
  return (
    <Group x={x - ax * w} y={y} opacity={opacity}>
      <Rect width={w} height={h} cornerRadius={h / 2} fill={bg} />
      {dot && <Rect x={size * 0.7} y={h / 2 - size * 0.25} width={size * 0.5} height={size * 0.5} cornerRadius={size * 0.25} fill={dot} />}
      <Text x={size * 0.7 + dotW} y={h / 2} anchorY={0.5} style={{ ...style, fill: { type: 'solid', color } }}>{label}</Text>
    </Group>
  );
}

// Five staff lines, for decoration.
function Staff({ x = 0, y = 0, width, gap = 12, color, opacity = 1, strokeWidth = 2 }:
  { x?: number; y?: number; width: number; gap?: number; color: string; opacity?: number; strokeWidth?: number }) {
  const commands = [] as any[];
  for (let k = 0; k < 5; k++) {
    commands.push({ type: 'moveTo', x: 0, y: k * gap });
    commands.push({ type: 'lineTo', x: width, y: k * gap });
  }
  return <Path x={x} y={y} commands={commands} stroke={color} strokeWidth={strokeWidth} opacity={opacity} />;
}

// A quarter note: an oval head and a stem.
function Note({ x, y, color, scale = 1, opacity = 1 }: { x: number; y: number; color: string; scale?: number; opacity?: number }) {
  return (
    <Group x={x} y={y} scale={scale} opacity={opacity}>
      <Rect x={-11} y={-8} width={22} height={16} cornerRadius={8} rotation={-20} fill={color} />
      <Rect x={8} y={-50} width={3} height={50} fill={color} />
    </Group>
  );
}

// The section plate, like a movement in a concert program.
function Plate({ no, ja, en }: { no: string; ja: string; en: string }) {
  const frame = useCurrentFrame();
  const p = progress(frame, 0, 14, Easings.easeOutBack);
  return (
    <Group x={56 - 300 * (1 - p)} y={70} opacity={clamp(p * 2)}
      shadow={{ color: '#2A1E4A30', blur: 16, offsetX: 0, offsetY: 6 }}>
      <Rect width={470} height={150} cornerRadius={14} fill={C.white} stroke={C.deep} strokeWidth={4} />
      <Rect x={20} y={22} width={80} height={80} cornerRadius={40} fill={hgrad(80, C.plum, C.lav)} />
      <T x={60} y={44} size={15} ax={0.5} ay={0.5} color={C.white} font="round" weight={800}>Mvt.</T>
      <T x={60} y={74} size={32} ax={0.5} ay={0.5} color={C.white}>{no}</T>
      <T x={285} y={52} size={52} ax={0.5} ay={0.5} color={C.deep} spacing={10}>{ja}</T>
      <T x={285} y={94} size={17} ax={0.5} ay={0.5} color={C.violet} spacing={6}>{en}</T>
      <Staff x={20} y={116} width={430} gap={5} color="#B5A2E7" strokeWidth={1.5} />
      {[0, 1, 2, 3, 4, 5].map((k) => (
        <Note key={k} x={60 + k * 68} y={126 - (k % 3) * 5 + 5} color={k % 2 ? C.pink : C.plum} scale={0.4}
          opacity={progress(frame, 6 + k * 2, 4)} />
      ))}
    </Group>
  );
}

// Velvet stage curtains. `open` 0 = closed, 1 = drawn to the sides.
function Curtains({ open, top = 0, bottom = H }: { open: number; top?: number; bottom?: number }) {
  const frame = useCurrentFrame();
  const w = lerp(W / 2 + 40, 210, open);
  const folds = 9;
  const fw = w / folds;
  const velvet = (width: number) => ({
    type: 'linear' as const, start: { x: 0, y: 0 }, end: { x: width, y: 0 },
    stops: [{ offset: 0, color: '#26103C' }, { offset: 0.45, color: '#7A48A8' }, { offset: 0.62, color: '#5B2F86' }, { offset: 1, color: '#26103C' }],
  });
  const scallops = [] as any[];
  scallops.push({ type: 'moveTo', x: 0, y: top });
  scallops.push({ type: 'lineTo', x: 0, y: top + 96 });
  for (let k = 0; k < 12; k++) {
    scallops.push({ type: 'quadTo', x1: (k + 0.5) * 160, y1: top + 150, x: (k + 1) * 160, y: top + 96 });
  }
  scallops.push({ type: 'lineTo', x: W, y: top });
  scallops.push({ type: 'close' });
  return (
    <Group>
      {(['left', 'right'] as const).map((side) => {
        const x0 = side === 'left' ? 0 : W - w;
        return (
          <Group key={side}>
            {Array.from({ length: folds }, (_, i) => (
              <Rect key={i} x={x0 + i * fw + Math.sin(frame / 18 + i) * 3 - 2} y={top} width={fw + 4} height={bottom - top}
                fill={velvet(fw + 4)} />
            ))}
            <Rect x={x0} y={bottom - 22} width={w} height={22} fill={vgrad(22, '#F2D27A', '#B88A2E')} />
          </Group>
        );
      })}
      <Path commands={scallops} fill={vgrad(top + 150, '#3A1A5C', '#6A3A98')} />
      <Path points={Array.from({ length: 97 }, (_, j) => {
        const x = (j / 96) * W;
        const k = (x % 160) / 160;
        return [x, top + 96 + 54 * 2 * k * (1 - k)] as [number, number];
      })} stroke="#F2D27A" strokeWidth={6} />
    </Group>
  );
}

// The audience: silhouettes waving penlights on the beat. The sticks are
// grouped by colour so each colour's glow is one blur pass, not one per stick.
const PEN_COLORS = [C.lav, C.pink, C.lav, C.mint, C.lav, C.gold];
function Penlights({ seed, count = 20, y = 1010, opacity = 1 }: { seed: string; count?: number; y?: number; opacity?: number }) {
  const frame = useCurrentFrame();
  const people = Array.from({ length: count }, (_, i) => {
    const lag = random(`${seed}-p-${i}`) * 0.25;
    return {
      i,
      x: (i + 0.5) * (W / count) + (random(`${seed}-x-${i}`) - 0.5) * 40,
      y: y + random(`${seed}-y-${i}`) * 24,
      swing: Math.sin((frame / BEAT - lag) * Math.PI) * 24,
      color: PEN_COLORS[i % PEN_COLORS.length],
    };
  });
  return (
    <Group opacity={opacity}>
      {[...new Set(PEN_COLORS)].map((color) => (
        <Group key={color} glow={{ color, blur: 16 }}>
          {people.filter((p) => p.color === color).map((p) => (
            <Group key={p.i} x={p.x + 46} y={p.y + 10} rotation={p.swing}>
              <Rect x={-6} y={-16} width={12} height={26} cornerRadius={4} fill="#2A2238" />
              <Rect x={-8} y={-118} width={16} height={104} cornerRadius={8} fill={color} />
            </Group>
          ))}
        </Group>
      ))}
      {people.map((p) => (
        <Rect key={p.i} x={p.x - 46} y={p.y} width={92} height={120} cornerRadius={46} fill="#1E1433" />
      ))}
    </Group>
  );
}

// ------------------------------------------------------------------ beats

// Where each beat's note pops. Most scenes scatter them in a free region;
// the built-in grid puts one on each card, the beat scene on the playhead.
const REGIONS: { from: number; to: number; x: [number, number]; y: [number, number] }[] = [
  { from: SCENE.intro, to: SCENE.write, x: [960, 1800], y: [950, 990] },
  { from: SCENE.write, to: SCENE.layer, x: [260, 1700], y: [905, 975] },
  { from: SCENE.layer, to: SCENE.render, x: [1100, 1750], y: [478, 492] },
  { from: SCENE.render, to: SCENE.announce, x: [900, 1100], y: [730, 810] },
  { from: SCENE.announce, to: SCENE.builtin, x: [1140, 1760], y: [720, 900] },
  { from: SCENE.curtain, to: SCENE.end, x: [870, 870], y: [800, 800] },
];

const GRID = { x: 155, y: 360, w: 380, h: 200, gap: 30 } as const;
const PLAYHEAD = { x: 1100, y: 600 } as const;

function hitPosition(i: number, frame: number): [number, number] {
  if (frame >= at(SCENE.builtin) && frame < at(SCENE.beat)) {
    const k = Math.round((frame - at(SCENE.builtin)) / BEAT);
    const col = k % 4, row = Math.floor(k / 4);
    return [GRID.x + col * (GRID.w + GRID.gap) + GRID.w - 46, GRID.y + row * (GRID.h + GRID.gap) + 46];
  }
  if (frame >= at(SCENE.beat) && frame < at(SCENE.stats)) {
    return [PLAYHEAD.x, PLAYHEAD.y - 150 + (i % 3) * 150];
  }
  const r = REGIONS.find((g) => frame >= at(g.from) && frame < at(g.to)) ?? REGIONS[0];
  return [
    lerp(r.x[0], r.x[1], random(`hit-x-${i}`)),
    lerp(r.y[0], r.y[1], random(`hit-y-${i}`)),
  ];
}

const POP_COLORS = [C.pink, C.plum, C.lav, C.gold];

// On every beat of the chart a note pops up, sparkles, and floats away.
function NotePops() {
  const frame = useCurrentFrame();
  const life = 22;
  return (
    <Group>
      {HITS.map((h, i) => {
        const local = frame - h;
        if (local < 0 || local > life) return null;
        const [x, y] = hitPosition(i, h);
        const color = POP_COLORS[i % POP_COLORS.length];
        const t = local / life;
        const pop = spring({ frame: local, fps: FPS, config: { damping: 9, stiffness: 260 } });
        const spread = Easings.easeOutCubic(t);
        return (
          <Group key={i} x={x} y={y}>
            {[0, 1, 2, 3, 4].map((k) => {
              const a = (k / 5) * Math.PI * 2 + i;
              return <Star key={k} x={Math.cos(a) * 70 * spread} y={Math.sin(a) * 70 * spread} r={9 * (1 - t)}
                color={k % 2 ? C.gold : C.white} rotation={local * 12} opacity={1 - t} />;
            })}
            <Group y={-40 * spread} rotation={Math.sin(local / 3 + i) * 10} scale={0.4 + 1.1 * pop} opacity={1 - t * t}>
              <Rect x={-30} y={-30} width={60} height={60} cornerRadius={30} fill={C.white}
                shadow={{ color: '#2A1E4A44', blur: 10, offsetX: 0, offsetY: 4 }} />
              <T x={0} y={2} ax={0.5} ay={0.5} size={40} font="round" weight={800} color={color}>{i % 3 ? '♪' : '♫'}</T>
            </Group>
          </Group>
        );
      })}
    </Group>
  );
}

// ------------------------------------------------------------------ scenes

// 0. Overture: the curtain is down, the program sits on the music stand,
// the baton taps three times, and the curtain rises.
const PROGRAM = [
  { no: 'I.', title: 'VIDEO IS CODE', ja: 'コードで映像を', at: 12 },
  { no: 'II.', title: 'WRITE & PREVIEW', ja: '書いて、すぐ見る', at: 24 },
  { no: 'III.', title: 'RENDER & PLAY', ja: '書き出して、再生', at: 36 },
];

function Overture() {
  const frame = useCurrentFrame();
  const rise = progress(frame, at(3), 26, Easings.easeInOutCubic);
  const zoomIn = progress(frame, at(3) - 4, 44, Easings.easeInCubic);
  const zoom = lerp(1.0, 1.06, progress(frame, 0, at(3), Easings.easeInOutSine)) + zoomIn * 0.5;
  const camX = 960 + noise('ov-x', frame / 30) * 10;
  const camY = 540 + noise('ov-y', frame / 30) * 8;
  const ready = frame - (at(3) + 6);
  const flash = clamp(1 - Math.abs(frame - (at(4) - 2)) / 4);
  const standDrop = progress(frame, at(3), 14, Easings.easeInCubic) * 800;
  // The baton taps the stand on every beat of bar 2.
  const tapping = frame >= at(2) - 8 && frame < at(3) + 4;
  const batonIn = progress(frame, at(2) - 8, 8, Easings.easeOutCubic);
  const beatPhase = ((frame - at(2)) % BEAT + BEAT) % BEAT;
  const lift = frame < at(2) ? 12 : Math.sin(Math.PI * beatPhase / BEAT) * 16;
  const bulbOn = (k: number) => (Math.floor(frame / 6) + k) % 2 === 0;
  return (
    <Group>
      <Background top="#0E0919" bottom="#2A1B48" />
      <Group x={960} y={540} scale={zoom}>
        <Group x={-camX} y={-camY}>
          {/* Behind the curtain: a lit stage */}
          <Rect x={0} y={170} width={W} height={700} fill={{ type: 'radial', center: { x: 960, y: 560 }, radius: 900,
            stops: [{ offset: 0, color: '#FFFFFF' }, { offset: 0.35, color: '#F1E8FF' }, { offset: 1, color: '#B5A2E7' }] }} />
          {Array.from({ length: 5 }, (_, i) => (
            <Path key={i} points={[[760 + i * 100, 170], [800 + i * 100, 170], [560 + i * 200, 870], [440 + i * 200, 870]]}
              closed fill="#FFFFFF66" />
          ))}
          <Curtains open={rise} top={170} bottom={880} />
          {/* Marquee with bulbs */}
          <Rect x={360} y={22} width={1200} height={130} cornerRadius={24} fill="#1A1030" stroke="#F2D27A" strokeWidth={4} />
          {/* Bulbs: the lit ones share one glow. */}
          {[false, true].map((lit) => (
            <Group key={String(lit)} glow={lit ? { color: '#FFD36A', blur: 10 } : undefined}>
              {Array.from({ length: 24 }, (_, k) => [[k, 30], [k + 1, 128]] as const).flat()
                .filter(([k]) => bulbOn(k) === lit)
                .map(([k, by], n) => (
                  <Rect key={n} x={378 + (by === 30 ? k : k - 1) * 50} y={by} width={14} height={14} cornerRadius={7}
                    fill={lit ? '#FFE9A8' : '#6B5A3A'} />
                ))}
            </Group>
          ))}
          <Mark x={500} y={87} size={44} />
          <T x={1000} y={78} ax={0.5} ay={0.5} size={50} color={C.white} glow={{ color: '#B5A2E7', blur: 16 }} spacing={4}>CELESTA · LIVE IN CONCERT</T>
          <T x={1000} y={116} ax={0.5} ay={0.5} size={18} font="round" weight={800} color="#F2D27A" spacing={6}>セレスタ ライブ・コンサート ♪ 本日の指揮 CELESTA-CHAN</T>
          {/* Stage apron: a celesta keyboard */}
          <Rect x={-200} y={870} width={2320} height={400} fill={vgrad(400, '#E4DCF7', '#B5A2E7')} />
          <Rect x={-200} y={870} width={2320} height={14} fill={C.pink} />
          {Array.from({ length: 40 }, (_, i) => (
            <Rect key={i} x={-200 + i * 60} y={910} width={56} height={150} cornerRadius={6} fill={C.white} />
          ))}
          {Array.from({ length: 40 }, (_, i) => ([1, 2, 4, 5, 6].includes(i % 7) ? (
            <Rect key={i} x={-200 + i * 60 - 17} y={910} width={34} height={92} cornerRadius={5} fill={C.deep} />
          ) : null))}
          {/* Music stand with tonight's program */}
          <Group y={standDrop}>
            <Rect x={950} y={700} width={20} height={300} fill="#2A2238" />
            <Path points={[[560, 690], [1360, 690], [1330, 720], [590, 720]]} closed fill="#2A2238" />
            <Group x={960} y={480} rotation={-2}>
              <Group x={-380} y={-220}>
                <Rect width={760} height={440} cornerRadius={10} fill="#FFFDF7" shadow={{ color: '#00000088', blur: 24, offsetX: 0, offsetY: 10 }} />
                <T x={380} y={56} ax={0.5} ay={0.5} size={42} color={C.deep} spacing={10}>PROGRAM</T>
                <T x={380} y={98} ax={0.5} ay={0.5} size={18} font="round" weight={800} color={C.grey} spacing={6}>本日の演目</T>
                <Staff x={40} y={122} width={680} gap={7} color="#CFC6E6" strokeWidth={1.5} />
                {PROGRAM.map((row, i) => {
                  const p = progress(frame, row.at, 8, Easings.easeOutBack);
                  const y = 210 + i * 76;
                  return (
                    <Group key={row.no} opacity={clamp(p * 2)} x={20 * (1 - p)}>
                      <Note x={62} y={y + 14} color={[C.pink, C.plum, C.lav][i]} scale={0.8} />
                      <T x={100} y={y} ay={0.5} size={30} color={C.plum}>{row.no}</T>
                      <T x={210} y={y} ay={0.5} size={30} color={C.deep}>{row.title}</T>
                      <T x={720} y={y} ax={1} ay={0.5} size={18} font="round" weight={800} color={C.grey}>{row.ja}</T>
                    </Group>
                  );
                })}
              </Group>
            </Group>
          </Group>
          {/* The baton, tapping the top edge of the stand */}
          {tapping && (
            <Group x={1560 + 300 * (1 - batonIn)} y={170} rotation={-24 + lift}>
              <Rect x={-420} y={-5} width={420} height={10} cornerRadius={5} fill={C.white} stroke="#C9C1DD" strokeWidth={2} />
              <Star x={-420} y={0} r={20} color="#E8E8F0" rotation={frame * 3} />
              <Rect x={-20} y={-24} width={60} height={48} cornerRadius={22} fill={C.white} stroke="#C9C1DD" strokeWidth={3} />
            </Group>
          )}
          {frame >= at(2) && frame < at(3) && beatPhase < 8 && (
            <T x={1210} y={232} ax={0.5} ay={0.5} size={34} color={C.white} stroke={{ color: C.plum, width: 6 }}
              rotation={-8} opacity={1 - beatPhase / 8} scale={1 + beatPhase / 20}>コツ</T>
          )}
        </Group>
      </Group>
      <Sparkles seed="ov" count={30} colors={[C.white, C.lav, C.pinkSoft]} drift={0.3} fall={1.2} size={9} />
      {frame >= at(3) - 4 && (
        <SpeedLines cx={960} cy={540} count={70} seed="ov-speed" color={C.white}
          opacity={0.5 * progress(frame, at(3) - 4, 10)} inner={300} />
      )}
      {ready >= 0 && (
        <T x={960} y={540} ax={0.5} ay={0.5} size={120} color={C.white} rotation={-4}
          scale={1 + 0.6 * (1 - progress(ready, 0, 8, Easings.easeOutBack))}
          opacity={progress(ready, 0, 4)} stroke={{ color: C.deep, width: 12 }}
          shadow={{ color: '#3B2A6BAA', blur: 0, offsetX: 8, offsetY: 8 }}>READY?</T>
      )}
      <Rect width={W} height={H} fill={C.white} opacity={flash} />
    </Group>
  );
}

// 1. Introduction: tonight's conductor, from the concert program.
function Intro() {
  const frame = useCurrentFrame();
  const card = spring({ frame, fps: FPS, delay: 8, config: { damping: 12, stiffness: 150 } });
  const badge = progress(frame, 40, 8, Easings.easeOutBack);
  return (
    <Group>
      <Background top="#FBF7FF" bottom="#E4DCF7" />
      {/* spotlight on the conductor */}
      <Path points={[[470, 0], [650, 0], [900, 900], [220, 900]]} closed fill={vgrad(900, '#FFFFFFEE', '#FFFFFF33')} />
      <Rect y={880} width={W} height={200} fill={vgrad(200, '#D9CCF5', '#B5A2E7')} />
      <Rect y={880} width={W} height={10} fill={C.pink} />
      <Rect x={250} y={905} width={620} height={60} cornerRadius={30} fill="#FFFFFF77" />
      <Curtains open={1} />
      <Sparkles seed="tk" count={28} colors={[C.pink, C.lav, C.gold]} drift={0.3} size={14} />
      <Portrait src={CHARA.conduct} x={560} y={1080} height={1000} from="left" />
      <Group x={1330} y={520} rotation={-4 + 8 * (1 - card)} scale={0.5 + 0.5 * card}
        opacity={clamp(card * 3)} shadow={{ color: '#3B2A6B44', blur: 30, offsetX: 0, offsetY: 16 }}>
        <Group x={-400} y={-240}>
          <Rect width={800} height={480} cornerRadius={22} fill={C.white} stroke={C.deep} strokeWidth={5} />
          <Rect x={5} y={5} width={790} height={70} cornerRadius={18} fill={hgrad(790, C.plum, C.lav)} />
          <T x={36} y={40} size={24} ay={0.5} color={C.white} spacing={4}>本日の指揮者</T>
          <T x={764} y={40} size={22} ax={1} ay={0.5} color={C.white} spacing={4}>TONIGHT'S CONDUCTOR</T>
          <T x={36} y={118} size={100} color={C.white} fill={hgrad(640, C.plum, C.pink)}
            stroke={{ color: C.deep, width: 4 }} spacing={6}>CELESTA</T>
          <T x={40} y={272} size={26} color={C.violet} font="round" weight={800}>セレスタちゃん　·　AI CONDUCTOR</T>
          <Staff x={40} y={330} width={720} gap={9} color="#D6CCEE" strokeWidth={2} />
          {[0, 1, 2, 3, 4, 5, 6].map((k) => (
            <Note key={k} x={90 + k * 86} y={360 - [0, 2, 1, 3, 2, 4, 3, 1][k] * 9} color={k % 2 ? C.pink : C.plum}
              opacity={progress(frame, 16 + k * 3, 5)} scale={0.9} />
          ))}
          <T x={40} y={420} size={30} color={C.deep}>your code  →  a video, on beat ♪</T>
          <Group x={735} y={405} scale={0.8 * (1.6 - 0.6 * badge)} opacity={badge} rotation={-10}>
            <Rect x={-74} y={-74} width={148} height={148} cornerRadius={74} fill={C.pinkSoft} stroke={C.pink} strokeWidth={6} />
            <T x={0} y={-16} size={24} ax={0.5} ay={0.5} color={C.pink}>ON</T>
            <T x={0} y={18} size={24} ax={0.5} ay={0.5} color={C.pink}>STAGE</T>
          </Group>
        </Group>
      </Group>
    </Group>
  );
}

// 2. Write: type a component, save, and the preview reloads.
type Seg = [string, string];
const CODE: Seg[][] = [
  [['export default ', C.lav], ['function ', C.lav], ['Hello', C.gold], ['() {', '#D8D2E8']],
  [['  return (', '#D8D2E8']],
  [['    <', '#8B83A3'], ['Composition ', C.pink], ['fps', C.mint], ['={', '#D8D2E8'], ['30', C.gold], ['}>', '#8B83A3']],
  [['      <', '#8B83A3'], ['Text ', C.pink], ['style', C.mint], ['={{ ', '#D8D2E8'], ['fontSize', C.mint], [': ', '#D8D2E8'], ['120', C.gold], [' }}>', '#8B83A3']],
  [['        Hello, Celesta!', C.white]],
  [['      </', '#8B83A3'], ['Text', C.pink], ['>', '#8B83A3']],
  [['    </', '#8B83A3'], ['Composition', C.pink], ['>', '#8B83A3']],
  [['  );', '#D8D2E8']],
  [['}', '#D8D2E8']],
];
const CODE_CHARS = CODE.reduce((n, line) => n + line.reduce((m, [s]) => m + s.length, 0) + 1, 0);
const SAVE_AT = at(2) - at(0); // local frame of ⌘S (bar 8)

function CodeView({ typed, size = 28 }: { typed: number; size?: number }) {
  const adv = size * 0.6;
  const lh = size * 1.62;
  let left = typed;
  let caret: [number, number] = [0, 0];
  return (
    <Group>
      {CODE.map((line, li) => {
        let col = 0;
        const parts: ReactNode[] = [];
        for (const [s, color] of line) {
          const shown = s.slice(0, Math.max(0, left));
          left -= s.length;
          if (shown.length) {
            parts.push(<Text key={col} x={col * adv} y={li * lh} anchorY="baseline"
              style={{ fontFamily: FONT.mono, fontSize: size, fill: { type: 'solid', color } }}>{shown}</Text>);
            caret = [col + shown.length, li];
          }
          col += s.length;
        }
        left -= 1;
        return <Group key={li}>{parts}</Group>;
      })}
      <Rect x={caret[0] * adv + 2} y={caret[1] * lh - size * 0.85} width={3} height={size * 1.05} fill={C.pink} />
    </Group>
  );
}

function Write() {
  const frame = useCurrentFrame();
  const typed = Math.floor(clamp((frame - 6) / 78) * CODE_CHARS);
  const saved = frame >= SAVE_AT;
  const shift = progress(frame, SAVE_AT + 22, 18, Easings.easeInOutCubic);
  const textPop = spring({ frame: frame - SAVE_AT, fps: FPS, config: { damping: 10, stiffness: 180 } });
  const toast = frame - SAVE_AT;
  return (
    <Group>
      <Background top="#F3EEFF" bottom="#DCD0F7" />
      {Array.from({ length: 6 }, (_, i) => (
        <T key={i} x={(random(`wn-x-${i}`) * W + frame * (1 + i % 3)) % (W + 200) - 100}
          y={200 + random(`wn-y-${i}`) * 700 + Math.sin(frame / 20 + i) * 20}
          size={60 + (i % 3) * 30} color="#B5A2E755" font="round" weight={800}>{i % 2 ? '♪' : '♫'}</T>
      ))}
      <Plate no="I" ja="書く" en="WRITE" />
      {/* Editor */}
      <Group x={120 - 700 * shift} y={260} opacity={1 - shift * 0.8}>
        <Rect width={900} height={600} cornerRadius={20} fill={C.night} shadow={{ color: '#2A1E4A55', blur: 30, offsetX: 0, offsetY: 14 }} />
        {[C.pink, C.gold, C.mint].map((c, i) => <Rect key={c} x={26 + i * 26} y={24} width={14} height={14} cornerRadius={7} fill={c} />)}
        <Rect x={120} y={14} width={170} height={36} cornerRadius={8} fill="#2A2140" />
        <T x={140} y={32} size={20} ay={0.5} font="mono" color={C.lavSoft}>film.tsx</T>
        {!saved && frame > 6 && <Rect x={274} y={27} width={10} height={10} cornerRadius={5} fill={C.lavSoft} />}
        <Group x={40} y={120}><CodeView typed={typed} /></Group>
        {toast >= 0 && toast < 40 && (
          <Group x={560} y={520} opacity={clamp(toast / 4) * clamp((40 - toast) / 6)}>
            <Pill x={0} y={0} label="⌘S  saved · reloaded" size={24} bg={C.pink} dot={C.white} />
          </Group>
        )}
      </Group>
      {/* Preview */}
      <Group x={lerp(1080, 160, shift)} y={lerp(260, 330, shift)} scale={lerp(1, 1.22, shift)}>
        <Rect width={720} height={460} cornerRadius={20} fill={C.white} stroke={C.deep} strokeWidth={4}
          shadow={{ color: '#2A1E4A40', blur: 30, offsetX: 0, offsetY: 14 }} />
        <T x={28} y={34} size={20} ay={0.5} color={C.deep} spacing={3}>PREVIEW</T>
        {saved ? (
          <Pill x={692} y={14} ax={1} label="LIVE" size={18} bg={C.mint} dot={C.white} />
        ) : (
          <Pill x={692} y={14} ax={1} label="waiting for save…" size={18} bg="#E9E4F5" color={C.grey} />
        )}
        <Rect x={20} y={60} width={680} height={382} cornerRadius={10} fill={saved ? '#20243A' : '#F1EEF8'} />
        {saved && (
          <T x={360} y={252} ax={0.5} ay={0.5} size={66} color={C.white} scale={textPop}
            opacity={clamp(textPop * 2)}>Hello, Celesta!</T>
        )}
        {!saved && <T x={360} y={252} ax={0.5} ay={0.5} size={26} color="#C9C1DD" font="round" weight={800}>1920 × 1080 · 30 fps</T>}
      </Group>
      {frame > 30 && (
        <Pill x={960} y={980} ax={0.5} label="no build step · just save" size={24}
          opacity={progress(frame, 30, 8) * (1 - shift)} />
      )}
      {frame >= SAVE_AT + 26 && (
        <Sequence from={SAVE_AT + 26}>
          <Portrait src={CHARA.point} x={1530} y={1110} height={840} from="right" />
          <Bubble x={1100} y={150} text={'I just hit save…\nand it\'s live~! ♪'} tail="down-right" delay={8} size={46} />
        </Sequence>
      )}
    </Group>
  );
}

// 3. Layer: five tracks join the score one part at a time, like an ensemble.
const TRACKS = [
  { name: 'VIDEO', color: C.plum, clips: [[0, 0.32], [0.34, 0.7], [0.72, 1]] },
  { name: 'AUDIO', color: C.pink, clips: [[0, 1]], wave: true },
  { name: 'OVERLAY', color: '#7C64C9', clips: [[0.08, 0.3], [0.5, 0.86]] },
  { name: 'DIALOGUE', color: '#E46BB6', clips: [[0.16, 0.44], [0.6, 0.78]] },
  { name: 'TEXT', color: C.gold, clips: [[0.02, 0.2], [0.36, 0.58], [0.8, 0.98]] },
];

function Layer() {
  const frame = useCurrentFrame();
  const x0 = 120, lane = 1680, top = 560, rowH = 84;
  const sweep = progress(frame, 60, 34, Easings.easeInOutSine);
  return (
    <Group>
      <Background top="#FFE3F0" bottom="#E3D6FF" />
      <Sparkles seed="ly" count={24} colors={[C.white, C.pink]} drift={0.5} size={12} />
      <Plate no="II" ja="かさねる" en="LAYER" />
      <Group x={1840} y={150}>
        <T x={0} y={0} ax={1} size={150} fill={hgrad(700, C.plum, C.pink)} color={C.white}
          stroke={{ color: C.white, width: 6 }} scale={1 + 0.06 * Math.max(0, 1 - (frame % BEAT) / 6)}>5 TRACKS</T>
        <T x={-8} y={190} ax={1} size={26} color={C.deep} spacing={6}>ONE SCORE · ALL PARTS IN SYNC</T>
      </Group>
      {TRACKS.map((track, i) => {
        const p = progress(frame, i * BEAT - 10, 10, Easings.easeOutBack);
        const y = top + i * rowH;
        const settle = frame - i * BEAT;
        return (
          <Group key={track.name} x={x0 + 1900 * (1 - p)} y={y}>
            <Rect width={lane} height={rowH - 14} cornerRadius={14} fill="#FFFFFFCC" stroke={C.deep} strokeWidth={3} />
            <Staff x={220} y={14} width={lane - 240} gap={(rowH - 42) / 4} color="#D6CCEE" strokeWidth={1.5} />
            <Rect x={10} y={10} width={190} height={rowH - 34} cornerRadius={10} fill={track.color} />
            <T x={105} y={(rowH - 14) / 2} ax={0.5} ay={0.5} size={22} color={C.white} spacing={2}>{track.name}</T>
            {track.clips.map(([a, b], k) => (
              <Group key={k}>
                <Rect x={220 + a * (lane - 240)} y={12} width={(b - a) * (lane - 240) - 6} height={rowH - 38} cornerRadius={8}
                  fill={track.color} opacity={0.8} />
                {track.wave && (
                  <Path points={Array.from({ length: 120 }, (_, j) => {
                    const v = LOUDNESS[(j * 13) % LOUDNESS.length] ?? 0;
                    return [226 + j * ((lane - 252) / 119), (rowH - 14) / 2 + (j % 2 ? 1 : -1) * v * 22] as [number, number];
                  })} stroke={C.white} strokeWidth={2} join="round" />
                )}
              </Group>
            ))}
            {settle >= 0 && settle < 14 && (
              <T x={lane - 10} y={-8} ax={1} ay={1} size={30} color={C.pink} stroke={{ color: C.white, width: 6 }}
                opacity={1 - settle / 14} rotation={-8} scale={1 + settle / 30}>♪ in!</T>
            )}
          </Group>
        );
      })}
      {sweep > 0 && (
        <Group x={x0 + 220 + sweep * (lane - 240)} y={top - 20}>
          <Rect x={-2} width={4} height={rowH * 5 + 10} fill={C.pink} glow={{ color: C.pink, blur: 10 }} />
          <Star x={0} y={-14} r={20} color={C.pink} rotation={frame * 4} />
        </Group>
      )}
    </Group>
  );
}

// 4. Render: one command renders the whole film.
const TERM = [
  { at: 0, text: '$ Celesta-export --react film.tsx out.mp4', color: C.white, type: true },
  { at: 34, text: '◇ bundling film.tsx …', color: '#B9B0D0' },
  { at: 44, text: '✓ prepare()  fonts · music · 7 portraits', color: C.mint },
  { at: 54, text: '▸ rendering 1584 frames on the GPU', color: C.lavSoft },
  { at: 104, text: '✓ H.264 + AAC  →  out.mp4', color: C.mint },
  { at: 114, text: '♪ 52.8 s · 1920×1080 · 30 fps · on beat', color: C.pink },
];

function Render() {
  const frame = useCurrentFrame();
  const size = 30;
  const bar = progress(frame, 56, 46, Easings.easeInOutSine);
  return (
    <Group>
      <Background top="#1C1430" bottom="#3B2A6B" />
      {Array.from({ length: 20 }, (_, i) => (
        <Rect key={i} x={i * 100 - (frame * 2) % 100} y={0} width={2} height={H} fill="#B5A2E712" />
      ))}
      <Sparkles seed="rd" count={30} colors={[C.lav, C.white]} drift={0.8} size={8} />
      <Group x={100} y={120}>
        <Rect width={1080} height={760} cornerRadius={20} fill="#0F0B18" stroke="#3B3150" strokeWidth={3}
          shadow={{ color: '#00000088', blur: 40, offsetX: 0, offsetY: 18 }} />
        {[C.pink, C.gold, C.mint].map((c, i) => <Rect key={c} x={26 + i * 26} y={24} width={14} height={14} cornerRadius={7} fill={c} />)}
        <T x={540} y={31} ax={0.5} ay={0.5} size={18} font="mono" color={C.grey}>celesta@stage: ~/film</T>
        {TERM.map((line, i) => {
          if (frame < line.at) return null;
          const shown = line.type ? line.text.slice(0, Math.floor((frame - line.at) * 1.5) + 1) : line.text;
          return (
            <Text key={i} x={44} y={120 + i * 64} anchorY="baseline" opacity={progress(frame, line.at, 4)}
              style={{ fontFamily: FONT.mono, fontSize: size, fontWeight: i === 0 ? 700 : 400, fill: { type: 'solid', color: line.color } }}>
              {shown}
            </Text>
          );
        })}
        {frame >= 54 && (
          <Group x={44} y={470}>
            <Rect width={700} height={30} cornerRadius={15} fill="#2A2140" />
            <Rect width={Math.max(30, 700 * bar)} height={30} cornerRadius={15} fill={hgrad(700, C.lav, C.pink)} />
            <T x={720} y={15} ay={0.5} size={26} font="mono" weight={700} color={C.white}>{`${Math.round(bar * 100)}%`}</T>
            <T x={0} y={60} ay={0.5} size={22} font="mono" color={C.grey}>{`frame ${pad(bar * 1583, 4)} / 1583`}</T>
          </Group>
        )}
      </Group>
      {frame >= 20 && (
        <Pill x={640} y={950} ax={0.5} label="one command · one file · zero clicks" size={26} bg={C.white} color={C.deep}
          opacity={progress(frame, 20, 8)} />
      )}
      <Sequence from={30}>
        <Portrait src={CHARA.point} x={1560} y={1120} height={880} from="right" />
        <Bubble x={1290} y={110} text={'my own\nrender studio!?'} tail="down-right" delay={30} size={46} />
      </Sequence>
    </Group>
  );
}

// 5. MC time: four lines between songs, one per bar, over the penlights.
const ANNOUNCE = [
  { pose: CHARA.hi, text: 'Hi! I\'m Celesta-chan ♪\nyour video conductor~' },
  { pose: CHARA.point, text: 'one .tsx file and\nI get a whole video!' },
  { pose: CHARA.wink, text: 'no dragging clips,\nno waiting ♡' },
  { pose: CHARA.cheer, text: 'Celesta makes videos\nsooo much easier~!' },
];

function Announce() {
  const frame = useCurrentFrame();
  const k = Math.min(3, Math.floor(frame / BAR));
  const local = frame - k * BAR;
  const bounce = 1 + 0.04 * Math.sin(Math.min(1, local / 10) * Math.PI);
  const line = ANNOUNCE[k];
  return (
    <Group>
      <Background top="#FBF6FF" bottom="#E7DBFB" />
      <Sunburst cx={560} cy={620} rays={36} colors={['#FFFFFF66', '#FFFFFF00']} rotation={frame * 0.3} />
      {Array.from({ length: 10 }, (_, i) => (
        <T key={i} x={1100 + random(`an-x-${i}`) * 780} y={(1100 - ((frame * (1.5 + random(`an-v-${i}`)) + random(`an-y-${i}`) * 1100) % 1200))}
          size={34 + (i % 3) * 14} color={i % 2 ? '#FF6FAE66' : '#B5A2E788'} font="round" weight={800}>{i % 3 ? '♥' : '♪'}</T>
      ))}
      <Penlights seed="mc" />
      <Pill x={960} y={52} ax={0.5} label="MC タイム · MC TIME" size={22} bg={C.deep} dot={C.pink} />
      <Group x={560} y={1080} scale={bounce} anchorX={0.5} anchorY={1}>
        <Group x={-560} y={-1080}>
          <Sequence key={k} from={k * BAR} durationInFrames={BAR}>
            <Portrait src={line.pose} x={560} y={1120} height={1060} delay={0} bob={4} />
          </Sequence>
        </Group>
      </Group>
      <Sequence key={`b${k}`} from={k * BAR} durationInFrames={BAR}>
        <Bubble x={1440} y={420} ax={0.5} ay={0.5} text={line.text} tail="left" size={56} cps={2.6} minWidth={760} />
      </Sequence>
      {ANNOUNCE.map((_, i) => (
        <Rect key={i} x={1380 + i * 34} y={620} width={i === k ? 40 : 14} height={14} cornerRadius={7}
          fill={i === k ? C.pink : '#B5A2E7'} />
      ))}
    </Group>
  );
}

// 6. Built in: what @celesta/react brings, one card per beat.
const FEATURES = [
  ['<Text>', 'fonts · gradients · metrics'],
  ['<Image>', 'png · svg · webp'],
  ['<Video>', 'anything FFmpeg reads'],
  ['<Audio>', 'volume keyframes · mixing'],
  ['<Path>', 'lines & bézier curves'],
  ['<Camera>', 'zoom · pan · shake'],
  ['Lip sync', 'mouths from a .wav'],
  ['PSD portraits', 'layers & presets'],
];

function BuiltIn() {
  const frame = useCurrentFrame();
  return (
    <Group>
      <Background top="#2B1F4D" bottom="#6A4FB0" />
      <Sparkles seed="bi" count={40} colors={[C.white, C.lav, C.gold]} drift={0.3} size={9} />
      <T x={960} y={120} ax={0.5} ay={0.5} size={64} color={C.white} glow={{ color: '#B5A2E7', blur: 20 }}
        opacity={progress(frame, 0, 8)}>BUILT INTO EVERY FRAME</T>
      <T x={960} y={190} ax={0.5} ay={0.5} size={26} color={C.lavSoft} font="round" weight={800}
        opacity={progress(frame, 4, 8)}>import するだけ、すぐ使える · ready the moment you import</T>
      <Group x={GRID.x} y={262}>
        <Rect width={4 * GRID.w + 3 * GRID.gap} height={56} cornerRadius={12} fill="#0F0B18" />
        <T x={24} y={28} ay={0.5} size={22} font="mono" weight={700} color={C.amber}>@celesta/react</T>
        <T x={270} y={28} ay={0.5} size={22} font="mono" color={C.lavSoft}>· React 18 · 1920×1080 · 30 fps · 150 BPM</T>
        <T x={4 * GRID.w + 3 * GRID.gap - 24} y={28} ax={1} ay={0.5} size={22} font="mono" color={C.grey}>Mvt. III</T>
      </Group>
      {FEATURES.map(([title, sub], k) => {
        const col = k % 4, row = Math.floor(k / 4);
        const appear = k * BEAT - 8;
        const p = progress(frame, appear, 10, Easings.easeOutBack);
        const ready = frame >= k * BEAT;
        return (
          <Group key={title} x={GRID.x + col * (GRID.w + GRID.gap) + GRID.w / 2} y={GRID.y + row * (GRID.h + GRID.gap) + GRID.h / 2}
            scale={0.6 + 0.4 * p} opacity={clamp(p * 2)}>
            <Group x={-GRID.w / 2} y={-GRID.h / 2}>
              <Rect width={GRID.w} height={GRID.h} cornerRadius={18} fill={C.white} stroke={C.deep} strokeWidth={4} />
              <Rect x={4} y={GRID.h - 14} width={GRID.w - 8} height={10} cornerRadius={4} fill={ready ? C.mint : '#E2DCF0'} />
              <T x={28} y={58} ay={0.5} size={40} color={C.deep}>{title}</T>
              <T x={30} y={106} ay={0.5} size={20} font="round" weight={800} color={C.grey}>{sub}</T>
              {ready && <Pill x={GRID.w - 24} y={130} ax={1} label="✓ READY" size={18} bg={C.mint} />}
            </Group>
          </Group>
        );
      })}
    </Group>
  );
}

// 7. On the beat: the score's waveform runs under a playhead.
function Beat({ start }: { start: number }) {
  const frame = useCurrentFrame();
  const g = start + frame;
  const beats = Math.floor(frame / BEAT) + 1;
  const since = frame % BEAT;
  const px = 9;
  const half = 64;
  const left = 470, right = 1880;
  const cols: [number, number][] = [];
  const lower: [number, number][] = [];
  for (let d = -half; d <= half + 92; d++) {
    const x = PLAYHEAD.x + d * px;
    if (x < left || x > right) continue;
    const v = LOUDNESS[g + d] ?? 0;
    cols.push([x, PLAYHEAD.y - 20 - v * 230]);
    lower.unshift([x, PLAYHEAD.y + 20 + v * 230]);
  }
  return (
    <Group>
      <Background top="#F6EEFF" bottom="#FFD9EC" />
      <Sparkles seed="bt" count={26} colors={[C.lav, C.pink]} drift={-1.2} size={12} />
      <Pill x={960} y={52} ax={0.5} label="on the beat ♪ ビートにぴったり" size={24} bg={C.pink} dot={C.white} />
      <Group x={240} y={560}>
        <T x={0} y={0} ax={0.5} ay={0.5} size={260} fill={vgrad(260, C.plum, C.pink)} color={C.white}
          stroke={{ color: C.white, width: 8 }} scale={1 + 0.15 * Math.max(0, 1 - since / 6)}>{beats}</T>
        <T x={0} y={180} ax={0.5} ay={0.5} size={34} color={C.deep} spacing={4}>BEATS</T>
        <T x={0} y={226} ax={0.5} ay={0.5} size={22} font="mono" weight={700} color={C.violet}>useBeat() · 12 f / beat</T>
      </Group>
      <Group clip={{ x: left, y: 200, width: right - left, height: 800, cornerRadius: 30 }}>
        <Rect x={left} y={200} width={right - left} height={800} cornerRadius={30} fill="#FFFFFFAA" stroke={C.deep} strokeWidth={4} />
        {Array.from({ length: 30 }, (_, i) => {
          const bf = Math.ceil((g - half) / BEAT) * BEAT + i * BEAT;
          const x = PLAYHEAD.x + (bf - g) * px;
          if (x < left || x > right) return null;
          return <Rect key={i} x={x - 1} y={220} width={2} height={760} fill={(bf / BEAT) % 4 === 0 ? '#57456C55' : '#57456C22'} />;
        })}
        <Path points={[...cols, ...lower]} closed fill={hgrad(right - left, C.lav, C.pink)} x={0} opacity={0.9} />
        <Rect x={PLAYHEAD.x - 3} y={210} width={6} height={780} fill={C.pink} glow={{ color: C.pink, blur: 14 }} />
      </Group>
      <Portrait src={CHARA.hi} x={1700} y={1150} height={560} from="right" delay={6} bob={4} />
      <Bubble x={1390} y={770} text={'right on\nthe beat~! ♪'} tail="right" delay={20} size={34} />
    </Group>
  );
}

// 8. Tonight's stats, then the audience takes a breath: "せーの…"
const STATS: [string, string, string][] = [
  ['RESOLUTION', '1920×1080', ''],
  ['FRAME RATE', '30', 'fps'],
  ['TRACKS', '5', ''],
  ['MOUSE DRAGS', '0', ''],
  ['RENDER FARMS', '0', ''],
  ['TIME TO VIDEO', '52.8', 's'],
];

function Stats() {
  const frame = useCurrentFrame();
  const count = Math.round(DURATION * Easings.easeOutCubic(progress(frame, 14, 70)));
  const breath = frame - at(2, 1);
  const out = progress(frame, at(2), 8, Easings.easeInCubic);
  const card = progress(frame, 0, 12, Easings.easeOutCubic);
  return (
    <Group>
      <Background top="#1D1433" bottom="#3B2A6B" />
      {Array.from({ length: 5 }, (_, i) => (
        <Path key={i} points={[[300 + i * 340, -20], [380 + i * 340, -20], [560 + i * 300, H], [200 + i * 300, H]]} closed
          fill={vgrad(H, '#B5A2E733', '#B5A2E700')} rotation={Math.sin(frame / 30 + i) * 3} />
      ))}
      <Sparkles seed="rs" count={24} colors={[C.white, C.lav]} drift={0.2} size={8} />
      <Group x={110 - 200 * (1 - card) - 900 * out} y={110} opacity={card}>
        <Rect width={840} height={860} cornerRadius={18} fill={C.white} shadow={{ color: '#00000066', blur: 30, offsetX: 0, offsetY: 16 }} />
        <Rect width={840} height={96} cornerRadius={18} fill={hgrad(840, C.plum, C.lav)} />
        <Rect y={80} width={840} height={16} fill={C.lav} />
        <T x={40} y={50} ay={0.5} size={34} color={C.white} spacing={6}>本日の公演データ</T>
        <T x={800} y={50} ax={1} ay={0.5} size={20} color={C.white} spacing={4}>TONIGHT'S STATS</T>
        {STATS.map(([label, value, unit], i) => {
          const p = progress(frame, 8 + i * 8, 8, Easings.easeOutBack);
          const y = 170 + i * 100;
          return (
            <Group key={label} opacity={clamp(p * 2)} x={30 * (1 - p)}>
              <Rect x={30} y={y + 46} width={780} height={2} fill="#EEE9F7" />
              <T x={44} y={y} ay={0.5} size={24} font="round" weight={800} color={C.grey} spacing={3}>{label}</T>
              <T x={unit ? 720 : 790} y={y} ax={1} ay={0.5} size={54} color={C.deep}>{value}</T>
              {unit && <T x={730} y={y + 10} ay={0.5} size={22} font="round" weight={800} color={C.grey}>{unit}</T>}
            </Group>
          );
        })}
        <Staff x={40} y={770} width={760} gap={7} color="#E2DAF2" strokeWidth={1.5} />
        <T x={44} y={830} ay={0.5} size={20} font="round" weight={800} color={C.violet}
          opacity={progress(frame, 60, 8)}>all from one film.tsx ♪</T>
      </Group>
      <Group x={1440 + 600 * out} y={460} opacity={1 - out}>
        <T x={0} y={0} ax={0.5} ay={0.5} size={180} color={C.white} glow={{ color: '#B5A2E7', blur: 24 }} spacing={4}>{count}</T>
        <T x={0} y={150} ax={0.5} ay={0.5} size={30} color={C.lav} spacing={14}>FRAMES</T>
        <T x={0} y={196} ax={0.5} ay={0.5} size={22} font="round" weight={800} color={C.lavSoft}>描いたフレーム、ぜんぶ React</T>
      </Group>
      {breath >= 0 && (
        <Group x={960} y={480}>
          <T x={0} y={0} ax={0.5} ay={0.5} size={220} fill={vgrad(220, C.white, C.lav)} color={C.white}
            stroke={{ color: C.deep, width: 10 }} rotation={-6}
            scale={1 + 1.2 * (1 - progress(breath, 0, 8, Easings.easeOutBack))} opacity={progress(breath, 0, 3)}
            shadow={{ color: '#00000088', blur: 0, offsetX: 10, offsetY: 10 }}>せーの…</T>
          {breath > 10 && <T x={0} y={190} ax={0.5} ay={0.5} size={80} color={C.white}
            opacity={Math.floor(breath / 4) % 2 ? 0.4 : 1}>• • •</T>}
        </Group>
      )}
    </Group>
  );
}

// 9. BRAVO!! — the drop, and a standing ovation.
function Bravo() {
  const frame = useCurrentFrame();
  const rainbow = ['#FF9EC7', '#FFD27A', '#9FF0C8', '#9CCBFF', '#C8B2FF', '#FFB3E6'];
  const slam = (d: number) => progress(frame, d, 9, Easings.easeOutBack);
  const jump = Math.abs(Math.sin((frame / BEAT) * Math.PI / 2)) * -40;
  return (
    <Group>
      <Rect width={W} height={H} fill="#FFF4FB" />
      <Sunburst cx={1250} cy={520} rays={36} colors={rainbow} rotation={frame * 0.6} />
      <Rect width={W} height={H} fill={{ type: 'radial', center: { x: 1250, y: 520 }, radius: 1100,
        stops: [{ offset: 0, color: '#FFFFFFCC' }, { offset: 0.5, color: '#FFFFFF33' }, { offset: 1, color: '#FFFFFF00' }] }} />
      {Array.from({ length: 70 }, (_, i) => {
        const x = random(`cf-x-${i}`) * W + Math.sin(frame / 10 + i) * 30;
        const y = (random(`cf-y-${i}`) * H * 1.2 + frame * (5 + random(`cf-v-${i}`) * 6)) % (H + 60) - 30;
        return <Rect key={i} x={x} y={y} width={14} height={24} cornerRadius={3}
          rotation={frame * (4 + i % 5) + i * 30} fill={[C.pink, C.gold, C.mint, C.lav, '#7CB8FF'][i % 5]} />;
      })}
      <Penlights seed="bv" y={1000} />
      <Group y={jump}>
        <Portrait src={CHARA.jump} x={520} y={1090} height={1040} delay={0} from="left" bob={0} />
      </Group>
      <T x={1220} y={400} ax={0.5} ay={0.5} size={215} rotation={-5} fill={vgrad(215, '#FFFFFF', C.gold)} color={C.white}
        stroke={{ color: C.plum, width: 14 }} scale={1 + 1.4 * (1 - slam(0))} opacity={clamp(slam(0) * 3)}
        shadow={{ color: '#3B2A6B88', blur: 0, offsetX: 10, offsetY: 12 }}>BRAVO!!</T>
      <T x={1290} y={650} ax={0.5} ay={0.5} size={140} rotation={-5} fill={vgrad(140, '#FFFFFF', C.pink)} color={C.white}
        stroke={{ color: C.plum, width: 12 }} scale={1 + 1.4 * (1 - slam(6))} opacity={clamp(slam(6) * 3)}
        shadow={{ color: '#3B2A6B88', blur: 0, offsetX: 8, offsetY: 10 }}>ブラボー！</T>
      {frame >= 16 && (
        <Group x={1730} y={170} rotation={-12 + 360 * (1 - progress(frame, 16, 14, Easings.easeOutCubic))}
          scale={progress(frame, 16, 14, Easings.easeOutBack)}>
          <Rect x={-100} y={-100} width={200} height={200} cornerRadius={100} fill={vgrad(200, '#FFE39A', '#FFB020')}
            stroke={C.plum} strokeWidth={8} />
          <T x={0} y={-18} ax={0.5} ay={0.5} size={28} color={C.white} stroke={{ color: C.plum, width: 5 }}>ENCORE</T>
          <T x={0} y={34} ax={0.5} ay={0.5} size={44} color={C.white} stroke={{ color: C.plum, width: 6 }}>♪♪</T>
        </Group>
      )}
      <Rect width={W} height={H} fill={C.white} opacity={clamp(1 - frame / 6)} />
    </Group>
  );
}

// 10. Curtain call: thank-yous, the way in, "Fin.", and the curtain falls.
function CurtainCall() {
  const frame = useCurrentFrame();
  const card = spring({ frame, fps: FPS, delay: 2, config: { damping: 13, stiffness: 120 } });
  const button = spring({ frame: frame - 28, fps: FPS, config: { damping: 11, stiffness: 200 } });
  const chip = (d: number) => progress(frame, d, 8, Easings.easeOutBack);
  const fin = frame - (at(31, 2) - at(SCENE.curtain));
  const close = progress(frame, BAR * 3 + 6, 28, Easings.easeInOutCubic);
  const fade = progress(frame, BAR * 4 - 14, 14);
  return (
    <Group>
      <Background top="#F7EEFF" bottom="#FFD6EA" />
      {[0, 1, 2].map((i) => (
        <Path key={i} points={[[560 + i * 400, 0], [700 + i * 400, 0], [900 + i * 400, 900], [360 + i * 400, 900]]}
          closed fill={vgrad(900, '#FFFFFFAA', '#FFFFFF11')} opacity={0.7 + 0.3 * Math.sin(frame / 16 + i * 2)} />
      ))}
      <Rect y={890} width={W} height={190} fill={vgrad(190, '#E4D6F7', '#C6B2EE')} />
      <Sparkles seed="ed" count={34} colors={[C.pink, C.lav, C.gold, C.white]} drift={0.3} fall={0.8} size={12} />
      <Portrait src={CHARA.wave} x={1500} y={1060} height={980} from="right" delay={10} />
      <Group x={740} y={480} rotation={-3 + 6 * (1 - card)} scale={0.6 + 0.4 * card} opacity={clamp(card * 3)}
        shadow={{ color: '#3B2A6B44', blur: 34, offsetX: 0, offsetY: 18 }}>
        <Group x={-560} y={-280}>
          <Rect width={1120} height={560} cornerRadius={26} fill={C.white} stroke={C.deep} strokeWidth={5} />
          <Rect x={5} y={5} width={1110} height={96} cornerRadius={22} fill={C.ink} />
          <Mark x={62} y={53} size={52} />
          <T x={104} y={53} size={44} ay={0.5} color={C.paper}>Celesta</T>
          <T x={1080} y={53} size={22} ax={1} ay={0.5} color={C.lav} spacing={4}>カーテンコール · CURTAIN CALL</T>
          <T x={56} y={150} size={60} color={C.deep}>Thank you for listening ♪</T>
          <T x={60} y={236} size={24} font="round" weight={800} color={C.violet} spacing={4}>ご清聴ありがとうございました　—　次はあなたの番です</T>
          <Staff x={56} y={290} width={1008} gap={7} color="#E2DAF2" strokeWidth={1.5} />
          {frame >= 28 && (
            <Group x={560} y={386} scale={0.7 + 0.3 * button} opacity={clamp(button * 3)}>
              <Group x={-510} y={-46}>
                <Rect width={1020} height={92} cornerRadius={46} fill={hgrad(1020, C.plum, C.pink)} />
                <Path x={64} y={46} points={[[-14, -24], [14, -24], [14, -2], [26, -2], [0, 24], [-26, -2], [-14, -2]]} closed fill={C.white} />
                <T x={108} y={46} ay={0.5} size={38} color={C.white}>Download Celesta</T>
                <Group opacity={clamp(chip(40) * 2)}>
                  <Pill x={808} y={23} ax={1} label="macOS" size={22} bg={C.white} color={C.plum} />
                </Group>
                <Group opacity={clamp(chip(46) * 2)}>
                  <Pill x={990} y={23} ax={1} label="Windows" size={22} bg={C.white} color={C.plum} />
                </Group>
              </Group>
            </Group>
          )}
          <Pill x={50} y={470} label="free & open source" size={22} bg={C.pink} dot={C.white} />
          <T x={720} y={490} size={30} ax={0.5} ay={0.5} font="mono" weight={700} color={C.deep}>github.com/mika-f/Celesta/releases</T>
        </Group>
      </Group>
      <Penlights seed="cc" y={1000} />
      {fin >= 0 && (
        <T x={700} y={880} ax={0.5} ay={0.5} size={120} color={C.pink} stroke={{ color: C.white, width: 12 }} rotation={-10}
          scale={1 + 0.6 * (1 - progress(fin, 0, 8, Easings.easeOutBack))} opacity={progress(fin, 0, 3)}
          shadow={{ color: '#3B2A6B66', blur: 0, offsetX: 6, offsetY: 6 }}>Fin.</T>
      )}
      <T x={960} y={1060} ax={0.5} ay={0.5} size={18} font="round" weight={800} color={C.white} spacing={4}
        opacity={progress(frame, 50, 12) * (1 - close)}>THANK YOU FOR LISTENING ♪ this video was made with Celesta</T>
      <Curtains open={1 - close} />
      <Rect width={W} height={H} fill={C.ink} opacity={fade} />
      {fade > 0 && <Mark x={960} y={540} size={140} opacity={fade} />}
    </Group>
  );
}

// ------------------------------------------------------------------ root

const SCENES: { bar: number; C: (props: { start: number }) => ReactNode }[] = [
  { bar: SCENE.overture, C: Overture },
  { bar: SCENE.intro, C: Intro },
  { bar: SCENE.write, C: Write },
  { bar: SCENE.layer, C: Layer },
  { bar: SCENE.render, C: Render },
  { bar: SCENE.announce, C: Announce },
  { bar: SCENE.builtin, C: BuiltIn },
  { bar: SCENE.beat, C: Beat },
  { bar: SCENE.stats, C: Stats },
  { bar: SCENE.bravo, C: Bravo },
  { bar: SCENE.curtain, C: CurtainCall },
];

function CutFlash() {
  const frame = useCurrentFrame();
  const cut = SCENES.slice(2).map((s) => at(s.bar)).find((c) => frame >= c && frame < c + 6);
  if (cut === undefined) return null;
  return <Rect width={W} height={H} fill={C.white} opacity={0.65 * (1 - (frame - cut) / 6)} />;
}

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={DURATION}>
      <Assets>
        <Font src={FONT_SRC} />
      </Assets>
      {SCENES.map(({ bar, C: Scene }, i) => {
        const from = at(bar);
        const to = at(SCENES[i + 1]?.bar ?? SCENE.end);
        return (
          <Sequence key={bar} from={from} durationInFrames={to - from}>
            <Scene start={from} />
          </Sequence>
        );
      })}
      <NotePops />
      <CutFlash />
      <Audio src="./music.wav" />
    </Composition>
  );
}
