import { Composition, Rect, useCurrentFrame } from '@celesta.studio/react';
import type { Paint } from '@celesta.studio/react';
import { Circle, Path } from '@celesta.studio/shapes';
import type { PolylinePoint } from '@celesta.studio/shapes';

// The five sample photos in ../assets/, one per frame: stylized landscapes
// drawn from gradients and silhouettes, so the template ships without
// third-party images. generate.sh exports frames 0-4 as PNG:
//
//   celesta-exporter --react --frames 0,1,2,3,4 draw-photos.tsx photo.png
//
// Every scene is deterministic, so regenerating gives the same pictures.

const WIDTH = 1920;
const HEIGHT = 1280;
// Silhouettes are sampled every STEP pixels across the width.
const STEP = 8;
// A 9:16 slideshow crops the photos to their middle: about x 600-1320.
// Suns and moons stay inside it so they show in both formats.

// A top-to-bottom gradient. `top` and `bottom` are in the shape's own
// coordinates: a <Rect>'s start at its top-left, a <Path>'s at the canvas'.
function vertical(top: number, bottom: number, stops: [number, string][]): Paint {
  return {
    type: 'linear',
    start: { x: 0, y: top },
    end: { x: 0, y: bottom },
    stops: stops.map(([offset, color]) => ({ offset, color })),
  };
}

function Sky({ stops, bottom = HEIGHT }: { stops: [number, string][]; bottom?: number }) {
  return <Rect width={WIDTH} height={HEIGHT} fill={vertical(0, bottom, stops)} />;
}

// A closed silhouette from `height(x)` (the top edge's y) down to the bottom.
function silhouette(height: (x: number) => number, bottom = HEIGHT): PolylinePoint[] {
  const points: PolylinePoint[] = [];
  for (let x = 0; x <= WIDTH; x += STEP) points.push([x, height(x)]);
  points.push([WIDTH, bottom], [0, bottom]);
  return points;
}

// Rolling hills: two sine waves around `base`.
function hills(base: number, amplitude: number, phase: number, wavelength = 900) {
  return (x: number) =>
    base -
    amplitude * (0.65 * Math.sin((x / wavelength) * Math.PI * 2 + phase) + 0.35 * Math.sin((x / (wavelength * 0.43)) * Math.PI * 2 + phase * 2.3));
}

// Mountains: the highest of several rounded peaks above `base`.
function peaks(base: number, list: [center: number, height: number, halfWidth: number][]) {
  return (x: number) => {
    let rise = 0;
    for (const [center, height, halfWidth] of list) {
      const t = Math.max(0, 1 - Math.abs(x - center) / halfWidth);
      rise = Math.max(rise, height * t * (1.4 - 0.4 * t));
    }
    return base - rise;
  };
}

function Land({ height, fill, bottom }: { height: (x: number) => number; fill: string | Paint; bottom?: number }) {
  return <Path points={silhouette(height, bottom)} closed fill={fill} />;
}

// A disc with a soft glow around it.
function Sun({ x, y, radius, color, glow }: { x: number; y: number; radius: number; color: string; glow: string }) {
  const outer = radius * 4;
  return (
    <>
      <Circle
        x={x}
        y={y}
        anchorX={0.5}
        anchorY={0.5}
        radius={outer}
        fill={{ type: 'radial', center: { x: outer, y: outer }, radius: outer, stops: [{ offset: 0, color: glow }, { offset: 1, color: `${glow.slice(0, 7)}00` }] }}
      />
      <Circle x={x} y={y} anchorX={0.5} anchorY={0.5} radius={radius} fill={color} />
    </>
  );
}

// Seeded pseudo-random numbers in [0, 1).
function random(seed: number) {
  let state = seed;
  return () => {
    state = (state * 1664525 + 1013904223) % 4294967296;
    return state / 4294967296;
  };
}

// 朝の港: dawn over a calm sea, the sun's path on the water.
function Harbor() {
  const horizon = 780;
  return (
    <>
      <Sky bottom={horizon} stops={[[0, '#8fb8de'], [0.55, '#f3c9a8'], [1, '#fbe3c4']]} />
      <Sun x={1100} y={640} radius={70} color="#fff4d6" glow="#ffe2b066" />
      <Land height={hills(720, 40, 1.2, 1300)} fill="#9aa9c0" bottom={horizon + 2} />
      <Rect y={horizon} width={WIDTH} height={HEIGHT - horizon} fill={vertical(0, HEIGHT - horizon, [[0, '#7d9fc0'], [1, '#2f5675']])} />
      {Array.from({ length: 9 }, (_, i) => (
        <Rect
          key={i}
          x={1100 - (60 + i * 26) / 2}
          y={horizon + 18 + i * 44}
          width={60 + i * 26}
          height={6 + i}
          cornerRadius={4}
          fill="#fff1d2"
          opacity={0.55 - i * 0.05}
        />
      ))}
      {/* A pier running out from the left. */}
      <Rect x={0} y={900} width={760} height={34} fill="#3a3a44" />
      {Array.from({ length: 7 }, (_, i) => (
        <Rect key={i} x={40 + i * 110} y={930} width={16} height={140} fill="#2c2c34" />
      ))}
    </>
  );
}

// 山あいの道: a road winding into green mountains.
function MountainRoad() {
  const horizon = 760;
  const road: PolylinePoint[] = [];
  const edge = (t: number, side: number): PolylinePoint => {
    const center = WIDTH * 0.52 + Math.sin(t * Math.PI * 1.6) * 260 * (1 - t);
    const half = 300 * (1 - t) + 6;
    return [center + side * half, HEIGHT - t * (HEIGHT - horizon)];
  };
  for (let i = 0; i <= 40; i += 1) road.push(edge(i / 40, -1));
  for (let i = 40; i >= 0; i -= 1) road.push(edge(i / 40, 1));
  return (
    <>
      <Sky bottom={horizon} stops={[[0, '#3d84c6'], [1, '#bfe0f2']]} />
      <Sun x={720} y={200} radius={56} color="#fffbe8" glow="#ffffff55" />
      <Land height={peaks(700, [[300, 330, 420], [900, 420, 520], [1500, 360, 460], [1920, 260, 380]])} fill="#7f9fb8" />
      <Land height={peaks(780, [[100, 220, 380], [700, 260, 420], [1300, 200, 360], [1800, 280, 420]])} fill="#4f7f6a" />
      <Land height={hills(860, 50, 0.4, 1100)} fill={vertical(780, HEIGHT, [[0, '#4f9a55'], [1, '#2f6a35']])} />
      <Path points={road} closed fill={vertical(horizon, HEIGHT, [[0, '#b9a98a'], [1, '#8a7a62']])} />
    </>
  );
}

// 湖のほとり: mountains mirrored in a still lake.
function Lake() {
  const shore = 820;
  const range = peaks(shore, [[200, 380, 520], [820, 520, 560], [1450, 430, 520], [1950, 300, 420]]);
  const near = peaks(shore, [[0, 160, 420], [1700, 200, 520]]);
  const mirror = (height: (x: number) => number) => (x: number) => 2 * shore - height(x);
  return (
    <>
      <Sky bottom={shore} stops={[[0, '#5aa6b8'], [1, '#d8eef0']]} />
      <Land height={range} fill="#6f8fa6" bottom={shore} />
      <Land height={near} fill="#2f5a52" bottom={shore} />
      <Rect y={shore} width={WIDTH} height={HEIGHT - shore} fill={vertical(0, HEIGHT - shore, [[0, '#9ccfd6'], [1, '#2e6f7c']])} />
      <Path points={silhouette(mirror(range), shore)} closed fill="#6f8fa6" opacity={0.35} />
      <Path points={silhouette(mirror(near), shore)} closed fill="#2f5a52" opacity={0.45} />
      {Array.from({ length: 6 }, (_, i) => (
        <Rect key={i} x={200 + i * 260} y={shore + 60 + i * 52} width={320} height={4} cornerRadius={2} fill="#ffffff" opacity={0.35} />
      ))}
    </>
  );
}

// 夕暮れの丘: a large low sun behind layered hills.
function SunsetHills() {
  return (
    <>
      <Sky stops={[[0, '#3b2a5c'], [0.45, '#d9606a'], [0.75, '#f5a25d'], [1, '#f7c97e']]} bottom={900} />
      <Sun x={760} y={700} radius={150} color="#ffd98a" glow="#ffb36a88" />
      <Land height={hills(760, 70, 2.1, 1500)} fill="#a2486a" />
      <Land height={hills(880, 80, 0.6, 1200)} fill="#6e2f5a" />
      <Land height={hills(1020, 70, 3.3, 1000)} fill="#3d1d40" />
      <Land height={hills(1150, 40, 1.7, 800)} fill="#24122a" />
    </>
  );
}

// 星空の下で: a night sky with a moon and stars over dark hills.
function NightSky() {
  const next = random(7);
  const stars = Array.from({ length: 160 }, () => ({ x: next() * WIDTH, y: next() * 820, r: 1 + next() * 2.2, o: 0.4 + next() * 0.6 }));
  return (
    <>
      <Sky stops={[[0, '#070b1e'], [0.7, '#1b2a55'], [1, '#33477a']]} bottom={1000} />
      {stars.map((star, i) => (
        <Circle key={i} x={star.x} y={star.y} anchorX={0.5} anchorY={0.5} radius={star.r} fill="#fdf8e8" opacity={star.o} />
      ))}
      <Sun x={1180} y={280} radius={64} color="#f4efd8" glow="#c9d6ff44" />
      <Land height={hills(900, 60, 0.9, 1400)} fill="#1a2440" />
      <Land height={hills(1040, 50, 2.6, 1000)} fill="#0d1328" />
    </>
  );
}

const SCENES = [Harbor, MountainRoad, Lake, SunsetHills, NightSky];

function Photo() {
  const Scene = SCENES[useCurrentFrame()];
  return <Scene />;
}

export default function Photos() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={SCENES.length}>
      <Photo />
    </Composition>
  );
}
