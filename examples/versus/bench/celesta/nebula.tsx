// NEBULA benchmark scene, Celesta version. See ../SCENE.md for the spec that
// the Remotion and fframes versions implement with the same math.
import { Assets, Composition, Font, Group, Path, Rect, Text, useCurrentFrame } from '@celesta/react';

const W = 1920;
const H = 1080;
const FPS = 60;
const FRAMES = 600;
const TAU = Math.PI * 2;

const BLOBS = ['#FF3D7F', '#3DA5FF', '#7B5CFF', '#00E0B8', '#FFB13D', '#FF6B3D'];
const STARS = ['#FFFFFF', '#9EC9FF', '#FFC2E0', '#B8FFF0'];

const rand = (i: number, k: number) => {
  const v = Math.sin(i * 12.9898 + k * 78.233) * 43758.5453;
  return v - Math.floor(v);
};
const mix = (a: string, b: string, k: number) => {
  const ch = (s: string, o: number) => parseInt(s.slice(o, o + 2), 16);
  const c = (o: number) => Math.round(ch(a, o) + (ch(b, o) - ch(a, o)) * k).toString(16).padStart(2, '0');
  return `#${c(1)}${c(3)}${c(5)}`;
};

// Four cubic Béziers around the origin.
const ellipse = (rx: number, ry: number) => {
  const k = 0.5523;
  return [
    { type: 'moveTo' as const, x: rx, y: 0 },
    { type: 'cubicTo' as const, x1: rx, y1: ry * k, x2: rx * k, y2: ry, x: 0, y: ry },
    { type: 'cubicTo' as const, x1: -rx * k, y1: ry, x2: -rx, y2: ry * k, x: -rx, y: 0 },
    { type: 'cubicTo' as const, x1: -rx, y1: -ry * k, x2: -rx * k, y2: -ry, x: 0, y: -ry },
    { type: 'cubicTo' as const, x1: rx * k, y1: -ry, x2: rx, y2: -ry * k, x: rx, y: 0 },
    { type: 'close' as const },
  ];
};

function Background({ t }: { t: number }) {
  const k = 0.5 + 0.5 * Math.sin(t * 0.6);
  return (
    <Rect width={W} height={H} fill={{
      type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: H },
      stops: [
        { offset: 0, color: mix('#0B1026', '#1A0B2E', k) },
        { offset: 1, color: mix('#04050C', '#0B1A2E', k) },
      ],
    }} />
  );
}

function Blobs({ t }: { t: number }) {
  return BLOBS.map((color, j) => {
    const r = 260 + 60 * Math.sin(t * 0.8 + j);
    return (
      <Rect key={j} x={960 + Math.cos(t * 0.35 + j * 1.047) * 520} y={540 + Math.sin(t * 0.5 + j * 1.3) * 260}
        anchorX={0.5} anchorY={0.5} width={r * 2} height={r * 2} cornerRadius={r}
        fill={color} opacity={0.55} blur={48} blendMode="screen" />
    );
  });
}

function Rings({ t }: { t: number }) {
  return Array.from({ length: 24 }, (_, k) => {
    const rx = 180 + k * 30;
    return (
      <Path key={k} x={960} y={540} rotation={k * 7.5 + t * (k % 2 ? -12 : 12)}
        commands={ellipse(rx, rx * 0.38)} stroke="#8FB8FF" strokeWidth={1.5}
        opacity={0.18 + 0.22 * (0.5 + 0.5 * Math.sin(t * 2 + k * 0.4))} />
    );
  });
}

function Particles({ t }: { t: number }) {
  return Array.from({ length: 1500 }, (_, i) => {
    const r = 60 + rand(i, 1) * 1000;
    const a = rand(i, 2) * TAU + t * (0.1 + rand(i, 3) * 0.4) * (rand(i, 4) < 0.5 ? -1 : 1);
    const size = 2 + rand(i, 3) * 6;
    return (
      <Rect key={i} x={960 + Math.cos(a) * r} y={540 + Math.sin(a) * r * 0.45}
        anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size / 2}
        fill={STARS[i % 4]} opacity={0.3 + 0.7 * (0.5 + 0.5 * Math.sin(t * (2 + rand(i, 5) * 4) + i))} />
    );
  });
}

function Spectrum({ t }: { t: number }) {
  return Array.from({ length: 80 }, (_, k) => {
    const h = 20 + 180 * Math.abs(Math.sin(t * 2.1 + k * 0.27) * Math.cos(t * 1.3 + k * 0.11));
    return (
      <Rect key={k} x={163 + k * 20} y={1040 - h} width={14} height={h} cornerRadius={4}
        fill={{
          type: 'linear', start: { x: 0, y: h }, end: { x: 0, y: 0 },
          stops: [{ offset: 0, color: '#3DA5FF' }, { offset: 1, color: '#FF3D7F' }],
        }} />
    );
  });
}

function Title({ t }: { t: number }) {
  return (
    <Group x={960} y={470} scale={1 + 0.03 * Math.sin(t * 2)} glow={{ color: '#7B5CFF', blur: 24 }}>
      <Text anchorX={0.5} anchorY={0.5} style={{
        fontFamily: 'Bebas Neue', fontSize: 220, letterSpacing: 20 + 10 * Math.sin(t),
        fill: { type: 'solid', color: '#FFFFFF' },
      }}>NEBULA</Text>
      <Text y={150} anchorX={0.5} anchorY={0.5} style={{
        fontFamily: 'IBM Plex Mono', fontSize: 24, fill: { type: 'solid', color: '#C8D6FF' },
      }}>1500 PARTICLES / 24 RINGS / 80 BARS / 6 BLURS</Text>
    </Group>
  );
}

function Hud({ t, frame }: { t: number; frame: number }) {
  const style = { fontFamily: 'IBM Plex Mono', fontSize: 18, fill: { type: 'solid' as const, color: '#9EC9FF' } };
  return (
    <Group opacity={0.8}>
      {Array.from({ length: 48 }, (_, j) => {
        const v = Math.sin(t * (1 + j * 0.13) + j) * 100;
        const label = `CH${String(j).padStart(2, '0')} ${v < 0 ? '-' : '+'}${Math.abs(v).toFixed(3)}`;
        return <Text key={j} x={j < 24 ? 40 : 1720} y={60 + (j % 24) * 30} style={style}>{label}</Text>;
      })}
      <Text x={1880} y={1060} anchorX={1} anchorY={1} style={style}>
        {`FRAME ${String(frame).padStart(4, '0')} / ${FRAMES}`}
      </Text>
    </Group>
  );
}

function Nebula() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  return (
    <>
      <Background t={t} />
      <Blobs t={t} />
      <Rings t={t} />
      <Particles t={t} />
      <Spectrum t={t} />
      <Title t={t} />
      <Hud t={t} frame={frame} />
    </>
  );
}

export default function Root() {
  return (
    <Composition width={W} height={H} fps={FPS} durationInFrames={FRAMES}>
      <Assets>
        <Font src="../assets/fonts/BebasNeue-Regular.ttf" />
        <Font src="../assets/fonts/IBMPlexMono-Regular.ttf" />
      </Assets>
      <Nebula />
    </Composition>
  );
}
