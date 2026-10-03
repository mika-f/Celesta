// NEBULA benchmark scene, Remotion version. See ../../SCENE.md for the spec
// that the Celesta and fframes versions implement with the same math.
import { loadFont } from '@remotion/fonts';
import { AbsoluteFill, staticFile, useCurrentFrame } from 'remotion';

const FPS = 60;
const FRAMES = 600;
const TAU = Math.PI * 2;

const BLOBS = ['#FF3D7F', '#3DA5FF', '#7B5CFF', '#00E0B8', '#FFB13D', '#FF6B3D'];
const STARS = ['#FFFFFF', '#9EC9FF', '#FFC2E0', '#B8FFF0'];

loadFont({ family: 'Bebas Neue', url: staticFile('fonts/BebasNeue-Regular.ttf') });
loadFont({ family: 'IBM Plex Mono', url: staticFile('fonts/IBMPlexMono-Regular.ttf') });

const rand = (i: number, k: number) => {
  const v = Math.sin(i * 12.9898 + k * 78.233) * 43758.5453;
  return v - Math.floor(v);
};
const mix = (a: string, b: string, k: number) => {
  const ch = (s: string, o: number) => parseInt(s.slice(o, o + 2), 16);
  const c = (o: number) => Math.round(ch(a, o) + (ch(b, o) - ch(a, o)) * k).toString(16).padStart(2, '0');
  return `#${c(1)}${c(3)}${c(5)}`;
};

const Background: React.FC<{ t: number }> = ({ t }) => {
  const k = 0.5 + 0.5 * Math.sin(t * 0.6);
  return <AbsoluteFill style={{
    background: `linear-gradient(to bottom, ${mix('#0B1026', '#1A0B2E', k)}, ${mix('#04050C', '#0B1A2E', k)})`,
  }} />;
};

const Blobs: React.FC<{ t: number }> = ({ t }) => (
  <>
    {BLOBS.map((color, j) => {
      const r = 260 + 60 * Math.sin(t * 0.8 + j);
      return <div key={j} style={{
        position: 'absolute', width: r * 2, height: r * 2, borderRadius: '50%',
        left: 960 + Math.cos(t * 0.35 + j * 1.047) * 520 - r,
        top: 540 + Math.sin(t * 0.5 + j * 1.3) * 260 - r,
        background: color, opacity: 0.55, filter: 'blur(48px)', mixBlendMode: 'screen',
      }} />;
    })}
  </>
);

const Rings: React.FC<{ t: number }> = ({ t }) => (
  <svg width={1920} height={1080} style={{ position: 'absolute' }}>
    {Array.from({ length: 24 }, (_, k) => {
      const rx = 180 + k * 30;
      return <ellipse key={k} rx={rx} ry={rx * 0.38} fill="none" stroke="#8FB8FF" strokeWidth={1.5}
        transform={`translate(960 540) rotate(${k * 7.5 + t * (k % 2 ? -12 : 12)})`}
        opacity={0.18 + 0.22 * (0.5 + 0.5 * Math.sin(t * 2 + k * 0.4))} />;
    })}
  </svg>
);

const Particles: React.FC<{ t: number }> = ({ t }) => (
  <>
    {Array.from({ length: 1500 }, (_, i) => {
      const r = 60 + rand(i, 1) * 1000;
      const a = rand(i, 2) * TAU + t * (0.1 + rand(i, 3) * 0.4) * (rand(i, 4) < 0.5 ? -1 : 1);
      const size = 2 + rand(i, 3) * 6;
      return <div key={i} style={{
        position: 'absolute', width: size, height: size, borderRadius: '50%',
        left: 960 + Math.cos(a) * r - size / 2, top: 540 + Math.sin(a) * r * 0.45 - size / 2,
        background: STARS[i % 4], opacity: 0.3 + 0.7 * (0.5 + 0.5 * Math.sin(t * (2 + rand(i, 5) * 4) + i)),
      }} />;
    })}
  </>
);

const Spectrum: React.FC<{ t: number }> = ({ t }) => (
  <>
    {Array.from({ length: 80 }, (_, k) => {
      const h = 20 + 180 * Math.abs(Math.sin(t * 2.1 + k * 0.27) * Math.cos(t * 1.3 + k * 0.11));
      return <div key={k} style={{
        position: 'absolute', left: 163 + k * 20, top: 1040 - h, width: 14, height: h, borderRadius: 4,
        background: 'linear-gradient(to top, #3DA5FF, #FF3D7F)',
      }} />;
    })}
  </>
);

const Title: React.FC<{ t: number }> = ({ t }) => (
  // drop-shadow's radius is twice the Gaussian's standard deviation (24).
  <div style={{
    position: 'absolute', left: 960, top: 470, transform: `scale(${1 + 0.03 * Math.sin(t * 2)})`,
    filter: 'drop-shadow(0 0 48px #7B5CFF)',
  }}>
    <div style={{
      position: 'absolute', transform: 'translate(-50%, -50%)', fontFamily: 'Bebas Neue', fontSize: 220,
      lineHeight: 1, letterSpacing: 20 + 10 * Math.sin(t), color: '#FFFFFF', whiteSpace: 'nowrap',
    }}>NEBULA</div>
    <div style={{
      position: 'absolute', top: 150, transform: 'translate(-50%, -50%)', fontFamily: 'IBM Plex Mono',
      fontSize: 24, color: '#C8D6FF', whiteSpace: 'nowrap',
    }}>1500 PARTICLES / 24 RINGS / 80 BARS / 6 BLURS</div>
  </div>
);

const Hud: React.FC<{ t: number; frame: number }> = ({ t, frame }) => {
  const style = { position: 'absolute', fontFamily: 'IBM Plex Mono', fontSize: 18, lineHeight: 1, color: '#9EC9FF' } as const;
  return (
    <AbsoluteFill style={{ opacity: 0.8 }}>
      {Array.from({ length: 48 }, (_, j) => {
        const v = Math.sin(t * (1 + j * 0.13) + j) * 100;
        const label = `CH${String(j).padStart(2, '0')} ${v < 0 ? '-' : '+'}${Math.abs(v).toFixed(3)}`;
        return <div key={j} style={{ ...style, left: j < 24 ? 40 : 1720, top: 60 + (j % 24) * 30 }}>{label}</div>;
      })}
      <div style={{ ...style, right: 40, bottom: 20 }}>{`FRAME ${String(frame).padStart(4, '0')} / ${FRAMES}`}</div>
    </AbsoluteFill>
  );
};

export const Nebula: React.FC = () => {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  return (
    <AbsoluteFill>
      <Background t={t} />
      <Blobs t={t} />
      <Rings t={t} />
      <Particles t={t} />
      <Spectrum t={t} />
      <Title t={t} />
      <Hud t={t} frame={frame} />
    </AbsoluteFill>
  );
};
