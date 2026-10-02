import { noise, random } from '@celesta/math';
import {
  Assets, Audio, Camera, Composition, Font, Grid, Group, Line, Polyline, Rect, Series, Text,
  TextReveal, computeSeries, Easings, frameToTimecode, interpolate, progress,
  useBeat, useCountUp, useCue, useCurrentFrame, useTypewriter,
} from '@celesta/react';

// APEX — a one-minute motion film. 1800 frames at 30 fps, 120 BPM (15 frames per beat).
// Every chapter is a pure function of the frame; every cut lands on a beat.
const W = 1920, H = 1080, FPS = 30, TAU = Math.PI * 2;
const INK = '#07080F', BONE = '#ECEAE3', ACID = '#C6FF3D', MAG = '#FF2E88', CYAN = '#28E0FF';
const GREY = '#6B7080';
const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const SCENES = [
  { name: 'open', durationInFrames: 120, Scene: Open },
  { name: 'tunnel', durationInFrames: 240, Scene: Tunnel },
  { name: 'words', durationInFrames: 240, Scene: Words },
  { name: 'gyro', durationInFrames: 240, Scene: Gyro },
  { name: 'spectrum', durationInFrames: 240, Scene: Spectrum },
  { name: 'cubes', durationInFrames: 240, Scene: Cubes },
  { name: 'field', durationInFrames: 300, Scene: Field },
  { name: 'finale', durationInFrames: 180, Scene: Finale },
];
const { durationInFrames } = computeSeries(SCENES);

type TProps = {
  children: string; x: number; y: number; size: number; color?: string; mono?: boolean;
  anchorX?: number; anchorY?: number; opacity?: number; outline?: boolean; blendMode?: 'add' | 'normal';
  scale?: number; letterSpacing?: number;
};
function T({ children, x, y, size, color = BONE, mono = false, anchorX = 0, anchorY = 0,
  opacity = 1, outline = false, blendMode = 'normal', scale = 1, letterSpacing = 0 }: TProps) {
  return <Text x={x} y={y} anchorX={anchorX} anchorY={anchorY} opacity={opacity}
    blendMode={blendMode} scale={scale} style={{
      fontFamily: mono ? 'IBM Plex Mono' : 'Bebas Neue', fontSize: size, letterSpacing,
      ...(outline ? { stroke: { paint: { type: 'solid', color }, width: 3 } }
        : { fill: { type: 'solid', color } }),
    }}>{children}</Text>;
}
function Disc({ x, y, r, color, opacity = 1 }: { x: number; y: number; r: number; color: string; opacity?: number }) {
  return <Rect x={x} y={y} anchorX={0.5} anchorY={0.5} width={2 * r} height={2 * r}
    cornerRadius={r} fill={color} opacity={opacity} />;
}
// Chromatic-split text: two additive ghosts that tear apart on beats and random glitch frames.
function Glitch({ children, x, y, size, frame, amount, color = BONE }: {
  children: string; x: number; y: number; size: number; frame: number; amount: number; color?: string;
}) {
  const burst = random(`g${frame}`) > 0.88 ? 1 : 0;
  const d = amount + burst * 38;
  const jy = burst * (random(`j${frame}`) - 0.5) * 24;
  return <>
    <T x={x - d} y={y + jy} size={size} color={MAG} anchorX={0.5} anchorY={0.5} blendMode="add" opacity={0.85}>{children}</T>
    <T x={x + d} y={y - jy} size={size} color={CYAN} anchorX={0.5} anchorY={0.5} blendMode="add" opacity={0.85}>{children}</T>
    <T x={x} y={y} size={size} color={color} anchorX={0.5} anchorY={0.5}>{children}</T>
  </>;
}
const rot = ([x, y, z]: number[], ax: number, ay: number): number[] => {
  const y1 = y * Math.cos(ax) - z * Math.sin(ax), z1 = y * Math.sin(ax) + z * Math.cos(ax);
  return [x * Math.cos(ay) + z1 * Math.sin(ay), y1, -x * Math.sin(ay) + z1 * Math.cos(ay)];
};
const proj = (p: number[], cx: number, cy: number, d = 1400): [number, number] => {
  const k = d / (d + p[2]);
  return [cx + p[0] * k, cy + p[1] * k];
};

// ── 1. COLD OPEN ───────────────────────────────────────────────────────────
function Open() {
  const f = useCurrentFrame();
  const tw = useTypewriter('SIGNAL ACQUIRED', { from: 20, framesPerChar: 2 });
  const n = useCountUp(4096, { delay: 8, durationInFrames: 90 });
  const w = 1700 * progress(f, 6, 40, Easings.easeOutExpo);
  const flood = progress(f, 96, 24, Easings.easeInExpo);
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Rect x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} width={w}
      height={mix(2, H, flood)} fill={flood > 0 ? BONE : ACID} />
    <T x={160} y={470} size={34} mono color={BONE} anchorY={1} opacity={1 - flood}>
      {tw.text + (tw.caretVisible ? '_' : ' ')}
    </T>
    <T x={160} y={620} size={26} mono color={GREY} opacity={progress(f, 14, 12) * (1 - flood)}>
      {`FRAME ${String(f).padStart(4, '0')} / ${String(n).padStart(4, '0')} NODES`}
    </T>
    <T x={W - 160} y={620} size={26} mono color={GREY} anchorX={1} opacity={progress(f, 30, 12) * (1 - flood)}>
      {'48 kHz · 1920×1080 · 30 FPS'}
    </T>
  </>;
}

// ── 2. TUNNEL + TITLE ──────────────────────────────────────────────────────
function Tunnel() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const N = 16;
  const colors = [ACID, MAG, CYAN];
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Group x={W / 2} y={H / 2} rotation={Math.sin(f / 45) * 5} scale={1 + pulse * 0.04}>
      {Array.from({ length: N }, (_, i) => {
        const t = ((i / N + f * 0.0045) % 1);
        const s = t ** 2.6 * 2.1;
        return <Rect key={i} anchorX={0.5} anchorY={0.5} width={W * s} height={H * s}
          stroke={colors[i % 3]} strokeWidth={2 + t * 5} rotation={t * 38 * Math.sin(f / 70)}
          opacity={Math.min(1, t * 6) * (1 - t * 0.25)} />;
      })}
    </Group>
    <Rect width={W} height={H} fill={BONE} opacity={1 - progress(f, 0, 14)} />
    <Group opacity={progress(f, 48, 6)}>
      <Glitch x={W / 2} y={H / 2 - 20} size={560} frame={f} amount={pulse * 14}>{'APEX'}</Glitch>
    </Group>
    <T x={W / 2} y={H - 120} size={30} mono color={BONE} anchorX={0.5} letterSpacing={8}
      opacity={progress(f, 120, 20)}>{'A FILM WRITTEN IN REACT'}</T>
  </>;
}

// ── 3. KINETIC WORDS ───────────────────────────────────────────────────────
const WORDS = ['WRITE', 'CODE.', 'RENDER', 'FRAMES.', 'MOTION', 'IS', 'MATH.', 'LAYER', 'BY', 'LAYER.',
  'NO', 'TIMELINE', 'JUST', 'FUNCTIONS', 'OF', 'TIME.'];
const SWATCH = [
  { bg: BONE, fg: INK }, { bg: ACID, fg: INK }, { bg: INK, fg: BONE }, { bg: MAG, fg: INK },
  { bg: INK, fg: ACID }, { bg: CYAN, fg: INK },
];
const CUES = WORDS.map((word, i) => ({ at: i * 15, word, ...SWATCH[i % SWATCH.length] }));
function Words() {
  const cue = useCue(CUES);
  if (!cue) return <Rect width={W} height={H} fill={INK} />;
  const { cue: c, previous, frame, index } = cue;
  const wipe = progress(frame, 0, 8, Easings.easeOutExpo);
  const pop = progress(frame, 0, 10, Easings.easeOutBack);
  return <>
    <Rect width={W} height={H} fill={previous ? previous.bg : INK} />
    <Rect width={W * wipe} height={H} fill={c.bg} />
    <Ring r={mix(200, 760, progress(frame, 0, 15, Easings.easeOutCubic))} color={c.fg} opacity={0.35 * (1 - progress(frame, 0, 15))} />
    <T x={W / 2} y={H / 2 + 10} size={mix(300, 420, pop)} color={c.fg} anchorX={0.5} anchorY={0.5}
      scale={0.88 + 0.12 * pop}>{c.word}</T>
    <T x={72} y={64} size={28} mono color={c.fg}>{`${String(index + 1).padStart(2, '0')} / ${WORDS.length}`}</T>
    <Rect x={72} y={H - 80} width={(W - 144) * ((index + progress(frame, 0, 15)) / WORDS.length)} height={4} fill={c.fg} />
  </>;
}
function Ring({ r, color, opacity = 1, x = W / 2, y = H / 2, width = 3 }: {
  r: number; color: string; opacity?: number; x?: number; y?: number; width?: number;
}) {
  return <Rect x={x} y={y} anchorX={0.5} anchorY={0.5} width={2 * r} height={2 * r}
    cornerRadius={r} stroke={color} strokeWidth={width} opacity={opacity} />;
}

// ── 4. GYROSCOPE ───────────────────────────────────────────────────────────
function Gyro() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const rings = [
    { r: 250, ax: 0.0, ay: 0.4, sx: 0.011, sy: 0.017, color: ACID },
    { r: 330, ax: 1.0, ay: 0.0, sx: 0.013, sy: -0.009, color: MAG },
    { r: 410, ax: 0.4, ay: 1.2, sx: -0.008, sy: 0.014, color: CYAN },
    { r: 490, ax: 2.0, ay: 0.7, sx: 0.006, sy: 0.01, color: BONE },
    { r: 570, ax: 0.7, ay: 2.4, sx: -0.01, sy: -0.007, color: ACID },
  ];
  const zoom = interpolate(f, [0, 240], [0.82, 1.18], { easing: Easings.easeInOutSine });
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Camera zoom={zoom} rotation={Math.sin(f / 80) * 3}>
      {rings.map((g, i) => {
        const pts = Array.from({ length: 129 }, (_, k): [number, number] => {
          const a = k / 128 * TAU;
          return proj(rot([Math.cos(a) * g.r, Math.sin(a) * g.r, 0], g.ax + f * g.sx, g.ay + f * g.sy), W / 2, H / 2);
        });
        return <Polyline key={i} points={pts} progress={progress(f, i * 8, 40, Easings.easeOutCubic)}
          stroke={g.color} strokeWidth={i % 2 ? 2 : 4} opacity={0.9} />;
      })}
      {rings.map((g, i) => {
        const a = f * 0.05 * (i % 2 ? -1 : 1) + i * 1.3;
        const [px, py] = proj(rot([Math.cos(a) * g.r, Math.sin(a) * g.r, 0], g.ax + f * g.sx, g.ay + f * g.sy), W / 2, H / 2);
        return <Disc key={i} x={px} y={py} r={10 + pulse * 5} color={g.color} />;
      })}
      <Rect x={W / 2} y={H / 2} anchorX={0.5} anchorY={0.5} width={110 + pulse * 22} height={110 + pulse * 22}
        rotation={f * 1.5} fill={ACID} glow={{ color: ACID, blur: 40 }} />
    </Camera>
    <T x={72} y={64} size={28} mono color={GREY}>{'03 — GYRO'}</T>
    <T x={72} y={H - 140} size={110} color={BONE}>{`${String(Math.round((f * 1.5) % 360)).padStart(3, '0')}°`}</T>
    <T x={W - 72} y={H - 100} size={26} mono color={GREY} anchorX={1}>{'5 AXES · 640 SAMPLES · NO MESH'}</T>
  </>;
}

// ── 5. SPECTRUM ────────────────────────────────────────────────────────────
function Spectrum() {
  const f = useCurrentFrame();
  const { pulse, beat } = useBeat({ bpm: 120, decay: 5 });
  const count = 80, step = (W - 160) / count;
  const bpm = useCountUp(120, { durationInFrames: 36 });
  return <>
    <Rect width={W} height={H} fill={INK} />
    <T x={W / 2} y={H / 2} size={720} color={BONE} anchorX={0.5} anchorY={0.5} outline opacity={0.1}>
      {`${bpm}`}
    </T>
    {Array.from({ length: count }, (_, i) => {
      const u = i / (count - 1);
      const env = 0.35 + 0.65 * Math.sin(u * Math.PI) ** 0.8;
      const v = (0.5 + 0.5 * noise(`s${i}`, f / 6 + i * 0.4)) * env + pulse * 0.28 * (1 - u * 0.5);
      const h = 14 + 440 * Math.min(1, v) * progress(f, i * 0.5, 16);
      const col = u < 0.5 ? mix(0, 1, u * 2) : 1;
      return <Group key={i}>
        <Rect x={80 + i * step} y={H / 2 - h / 2} width={step - 5} height={h}
          fill={u < 0.34 ? ACID : u < 0.67 ? CYAN : MAG} opacity={0.55 + 0.45 * col} />
      </Group>;
    })}
    <Rect x={80} y={H / 2 - 1} width={W - 160} height={2} fill={BONE} />
    <T x={72} y={64} size={28} mono color={GREY}>{'04 — SPECTRUM'}</T>
    <T x={W - 72} y={64} size={28} mono color={BONE} anchorX={1}>{`BEAT ${String(beat + 1).padStart(3, '0')}`}</T>
    <T x={72} y={H - 96} size={26} mono color={GREY}>{'80 BANDS · DRIVEN BY noise() + useBeat()'}</T>
  </>;
}

// ── 6. WIREFRAME CUBES ─────────────────────────────────────────────────────
const CORNERS = [[-1, -1, -1], [1, -1, -1], [1, 1, -1], [-1, 1, -1], [-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1]];
const EDGES = [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]];
function Cubes() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const colors = [ACID, CYAN, MAG, BONE, ACID, CYAN];
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Camera shake={pulse * 10} seed="cubes" zoom={1 + pulse * 0.02}>
      {colors.map((color, c) => {
        const size = 80 + c * 46;
        const ax = f * 0.012 * (1 + c * 0.2) + c * 0.5, ay = f * 0.017 * (c % 2 ? -1 : 1) + c * 0.8;
        const pts = CORNERS.map((p) => proj(rot(p.map((v) => v * size), ax, ay), W / 2, H / 2, 2200));
        const on = progress(f, c * 10, 20);
        return <Group key={c} opacity={on}>
          {EDGES.map(([a, b], k) => <Line key={k} x1={pts[a][0]} y1={pts[a][1]} x2={pts[b][0]} y2={pts[b][1]}
            stroke={color} strokeWidth={c === 0 ? 5 : 2.5} />)}
          {pts.map(([x, y], k) => <Disc key={k} x={x} y={y} r={c === 0 ? 8 : 5} color={color} />)}
        </Group>;
      })}
    </Camera>
    <T x={72} y={64} size={28} mono color={GREY}>{'05 — VOLUME'}</T>
    <T x={W - 72} y={H - 100} size={26} mono color={GREY} anchorX={1}>{'6 CUBES · 48 EDGES · PERSPECTIVE IN 6 LINES OF MATH'}</T>
  </>;
}

// ── 7. FIELD ───────────────────────────────────────────────────────────────
function Field() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120, decay: 4 });
  const cols = 24, rows = 14, cw = W / cols, ch = H / rows;
  const out = progress(f, 270, 30);
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Grid columns={cols} columnWidth={cw} rowHeight={ch}>
      {Array.from({ length: cols * rows }, (_, i) => {
        const cx = (i % cols + 0.5) * cw - W / 2, cy = (Math.floor(i / cols) + 0.5) * ch - H / 2;
        const d = Math.hypot(cx, cy);
        const wave = 0.5 + 0.5 * Math.sin(d * 0.011 - f * 0.12 + pulse * 1.5);
        const on = progress(f, d * 0.04, 14);
        const s = (0.12 + 0.86 * wave) * on * (1 - out);
        return <Rect key={i} x={cw / 2} y={ch / 2} anchorX={0.5} anchorY={0.5} width={cw * 0.86 * s}
          height={ch * 0.86 * s} rotation={(1 - wave) * 90 * on} cornerRadius={wave > 0.8 ? ch * 0.43 * s : 0}
          fill={wave < 0.4 ? CYAN : wave < 0.72 ? MAG : ACID} />;
      })}
    </Grid>
    <Group opacity={progress(f, 30, 12) * (1 - out)}>
      <Rect x={W / 2} y={H / 2 + 25} anchorX={0.5} anchorY={0.5} width={1560} height={640} fill={INK} opacity={0.9} />
      <Rect x={W / 2 - 780} y={H / 2 - 295} width={1560} height={4} fill={ACID} />
      <T x={W / 2} y={H / 2 - 100} size={300} color={BONE} anchorX={0.5} anchorY={0.5}>{'EVERY FRAME'}</T>
      <T x={W / 2} y={H / 2 + 150} size={300} color={BONE} anchorX={0.5} anchorY={0.5}>{'A FUNCTION'}</T>
    </Group>
    <T x={72} y={64} size={28} mono color={BONE} blendMode="normal">{'06 — FIELD · 336 CELLS'}</T>
  </>;
}

// ── 8. FINALE ──────────────────────────────────────────────────────────────
function Finale() {
  const f = useCurrentFrame();
  const { pulse } = useBeat({ bpm: 120 });
  const out = progress(f, 150, 30);
  return <>
    <Rect width={W} height={H} fill={INK} />
    <Group opacity={1 - out}>
      {[0, 1, 2, 3, 4, 5].map((i) => {
        const age = (((f - i * 15) % 90) + 90) % 90 / 90;
        return <Ring key={i} r={120 + age * 900} color={[ACID, CYAN, MAG][i % 3]} width={3}
          opacity={(1 - age) * 0.7 * progress(f, i * 6, 10)} />;
      })}
      <Group x={W / 2} y={H / 2} scale={1 + pulse * 0.03}>
        <TextReveal x={-440} y={-312} lineHeight={560} baseline={0.82} stagger={0} durationInFrames={26}
          style={{ fontFamily: 'Bebas Neue', fontSize: 600, fill: { type: 'solid', color: BONE } }}>
          {'APEX'}
        </TextReveal>
      </Group>
      <T x={W / 2} y={H - 170} size={30} mono color={BONE} anchorX={0.5} letterSpacing={10}
        opacity={progress(f, 40, 20)}>{'MADE WITH CELESTA'}</T>
      <T x={W / 2} y={H - 120} size={22} mono color={GREY} anchorX={0.5} opacity={progress(f, 56, 20)}>
        {'REACT · RUST · FFMPEG — EVERY FRAME RENDERED FROM CODE'}
      </T>
    </Group>
  </>;
}

// Global frame HUD: corner brackets, timecode, and a film-long progress line.
function Hud() {
  const f = useCurrentFrame();
  const m = 40, L = 36;
  const corners: [number, number, number, number][] = [[m, m, 1, 1], [W - m, m, -1, 1], [m, H - m, 1, -1], [W - m, H - m, -1, -1]];
  return <Group blendMode="difference" opacity={0.9}>
    {corners.map(([x, y, dx, dy], i) => <Group key={i}>
      <Line x1={x} y1={y} x2={x + dx * L} y2={y} stroke={BONE} strokeWidth={2} cap="butt" />
      <Line x1={x} y1={y} x2={x} y2={y + dy * L} stroke={BONE} strokeWidth={2} cap="butt" />
    </Group>)}
    <T x={W - 72} y={H - 62} size={20} mono color={BONE} anchorX={1}>{frameToTimecode(f, FPS)}</T>
    <Rect x={m} y={H - 8} width={(W - 2 * m) * f / durationInFrames} height={3} fill={BONE} />
  </Group>;
}

export default function Root() {
  return <Composition width={W} height={H} fps={FPS} durationInFrames={durationInFrames}>
    <Assets>
      <Font src="../afterimage/assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="../afterimage/assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Audio src="./assets/score.wav" />
    <Series>
      {SCENES.map(({ name, durationInFrames, Scene }) => (
        <Series.Sequence key={name} durationInFrames={durationInFrames}><Scene /></Series.Sequence>
      ))}
    </Series>
    <Hud />
  </Composition>;
}
