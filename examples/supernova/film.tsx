import { Assets, Composition, Font, Group, Instances, Mask, Rect, Span, Text, useCurrentFrame } from '@celesta.studio/react';
import type { Paint, TextStyle } from '@celesta.studio/react';
import { Circle, Ellipse, Path } from '@celesta.studio/shapes';
import { defineShader } from '@celesta.studio/shader';

const W = 1920;
const H = 1080;
const FPS = 30;
const DURATION = 1080;
const CHAPTER = 216;
const COLORS = ['#55E7FF', '#667AFF', '#B06CFF', '#FF5FA2', '#FFB477'];
const NAMES = ['01 / DEPARTURE', '02 / ORBITAL MECHANICS', '03 / LIQUID LIGHT', '04 / EVENT HORIZON', '05 / AFTER THE LIGHT'];
const CAPTIONS = ['A journey through the render pipeline', 'Gravity is a drawing instrument', 'Matter dissolves into color', 'The universe folds back on itself', 'Nothing disappears. It becomes light.'];
const range = (n: number) => Array.from({ length: n }, (_, i) => i);
const fract = (v: number) => v - Math.floor(v);
const rand = (i: number, channel: number) => fract(Math.sin(i * 127.1 + channel * 311.7) * 43758.5453);
const ease = (v: number) => { const p = Math.max(0, Math.min(1, v)); return p * p * (3 - 2 * p); };
const gradient = (size: number, a: string, b: string): Paint => ({
  type: 'linear', start: { x: -size, y: -size }, end: { x: size, y: size },
  stops: [{ offset: 0, color: a }, { offset: 0.5, color: '#E1F9FF' }, { offset: 1, color: b }],
});
const style = (size: number, fill: string | Paint = '#EDF6FF'): TextStyle => ({
  fontFamily: 'Bebas Neue', fontSize: size, fill: typeof fill === 'string' ? { type: 'solid', color: fill } : fill,
});
const lens = defineShader({
  name: 'Supernova gravitational lens', params: { phase: 'f32' },
  wgsl: `fn effect(input: EffectInput) -> vec4f {
    let center = (celesta.content.xy + celesta.content.zw) * 0.5;
    let p = input.position - center;
    let r = length(p);
    let a = 0.13 * sin(r * 0.012 - params.phase);
    let q = vec2f(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a));
    return source_at(center + q);
  }`,
});

function Stars({ t, warp }: { t: number; warp: number }) {
  const count = 12000;
  const x = new Float32Array(count);
  const y = new Float32Array(count);
  const width = new Float32Array(count);
  const height = new Float32Array(count);
  const rotation = new Float32Array(count);
  const color = new Uint8Array(count * 4);
  for (let i = 0; i < count; i++) {
    const angle = rand(i, 1) * Math.PI * 2 + t * 0.025;
    const z = fract(rand(i, 2) + t * 0.09);
    const radius = 40 + z * z * 1200;
    x[i] = W / 2 + Math.cos(angle) * radius;
    y[i] = H / 2 + Math.sin(angle) * radius * 0.68;
    width[i] = 1 + z * 3 + warp * z * z * 38;
    height[i] = 0.6 + z * 1.5;
    rotation[i] = angle * 180 / Math.PI;
    color.set([110 + rand(i, 3) * 100, 150 + rand(i, 4) * 100, 255, 35 + z * 125], i * 4);
  }
  return <Instances count={count} x={x} y={y} width={width} height={height} rotation={rotation} color={color}
    blendMode="add" zoomBlur={{ amount: warp * 0.12 }} />;
}

function Rings({ t, strength }: { t: number; strength: number }) {
  return <Group x={960} y={540} rotation={t * 7} opacity={0.16 + strength * 0.56}>
    {range(240).map(i => {
      const r = 175 + i * 3.1;
      return <Ellipse key={i} x={-r} y={-r * 0.62} width={r * 2} height={r * 1.24}
        rotation={i * 0.66 + 18 * Math.sin(t * 0.4 + i * 0.018)} scaleY={0.58 + 0.3 * Math.sin(i * 0.013 + t * 0.3)}
        stroke={gradient(r, COLORS[i % 5], COLORS[(i + 2) % 5])} strokeWidth={0.7 + (i % 3) * 0.3} />;
    })}
    {range(48).map(i => <Path key={`curve-${i}`} rotation={i * 7.5 + t * 4}
      commands={[{ type: 'moveTo', x: -880, y: 0 }, { type: 'cubicTo', x1: -400, y1: -700 + i * 12, x2: 400, y2: 700 - i * 12, x: 880, y: 0 }]}
      stroke={gradient(880, '#55E7FF', '#FF5FA2')} strokeWidth={1.2} />)}
  </Group>;
}

function Bokeh({ t, strength }: { t: number; strength: number }) {
  return <Group opacity={0.18 + strength * 0.67}>
    {range(32).map(i => {
      const r = 80 + rand(i, 8) * 135;
      return <Circle key={i} x={160 + rand(i, 1) * 1600 + Math.sin(t * 0.3 + i) * 100}
        y={140 + rand(i, 2) * 800 + Math.cos(t * 0.4 + i) * 90} radius={r}
        fill={COLORS[i % 5]} opacity={0.15} blur={20 + i % 5 * 9} blendMode={i % 2 ? 'screen' : 'add'} />;
    })}
    {range(18).map(i => <Group key={`glass-${i}`} x={300 + i % 6 * 260} y={250 + Math.floor(i / 6) * 280}
      rotation={t * 6 + i * 13} saturation={0.7 + Math.sin(t + i) * 0.3} hue={t * 12 + i * 3} blendMode="screen">
      <Rect x={-150} y={-95} width={300} height={190} cornerRadius={28}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 300, y: 190 },
          stops: [{ offset: 0, color: `${COLORS[i % 5]}55` }, { offset: 0.5, color: '#09223D33' }, { offset: 1, color: `${COLORS[(i + 2) % 5]}99` }] }}
        stroke="#B6EEFF66" strokeWidth={1} />
      <Rect x={-115} y={-65} width={160} height={3} fill="#FFFFFFAA" />
    </Group>)}
  </Group>;
}

function Kaleidoscope({ t }: { t: number }) {
  return <Group x={960} y={540} shader={lens({ phase: t })}>
    {range(12).map(i => <Group key={i} rotation={i * 30 + t * 8} blendMode={i % 2 ? 'screen' : 'add'}
      mask={<Mask mode="luminance"><Rect x={-780} y={-780} width={1560} height={1560}
        fill={{ type: 'radial', center: { x: 780, y: 780 }, radius: 780,
          stops: [{ offset: 0, color: '#000000' }, { offset: 0.3, color: '#FFFFFF' }, { offset: 0.78, color: '#FFFFFF' }, { offset: 1, color: '#000000' }] }} /></Mask>}>
      <Group rotationX={25 * Math.sin(t * 0.5)} rotationY={25 * Math.cos(t * 0.4)} perspective={1600}
        mask={<Circle x={0} y={-300} radius={490} fill="#FFFFFF" />}>
        {range(22).map(j => <Rect key={j} x={j * 24 - 280} y={-700} width={12 + j % 3 * 3} height={1400}
          rotation={20 * Math.sin(t * 0.4 + j * 0.15)} fill={COLORS[(i + j) % 5]} opacity={0.35} />)}
      </Group>
    </Group>)}
    <Circle x={-110} y={-110} radius={110} fill="#050915" stroke="#95F1FF" strokeWidth={2} />
    <Ellipse x={-145} y={-145} width={290} height={290} stroke="#FFFFFF" strokeWidth={1} />
  </Group>;
}

function Finale({ t }: { t: number }) {
  return <Group>
    {range(9).map(i => <Text key={i} x={960} y={140 + i * 100} anchorX={0.5}
      scale={1 + 0.04 * Math.sin(t + i)} opacity={0.09} rotation={-8}
      style={{ ...style(128, gradient(800, '#55E7FF', '#FF5FA2')), stroke: { paint: { type: 'solid', color: '#B2EFFF' }, width: 1 } }}>
      WE ARE MADE OF LIGHT
    </Text>)}
    <Text x={960} y={380} anchorX={0.5} style={style(240, gradient(700, '#55E7FF', '#FF5FA2'))}
      chromaticAberration={3 + 2 * Math.sin(t)} glow={{ color: '#496AFF88', blur: 24 }}
      shadow={{ color: '#000000', offsetX: 0, offsetY: 12, blur: 20 }}>
      SUPER<Span color="#FFFFFF">NOVA</Span>
    </Text>
    <Text x={960} y={670} anchorX={0.5} style={{ ...style(34), letterSpacing: 12 }}>A MILLION WAYS TO BECOME LIGHT</Text>
  </Group>;
}

function Film() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  const chapter = Math.min(4, Math.floor(frame / CHAPTER));
  const weight = (i: number) => ease((frame - i * CHAPTER + 18) / 36) * (1 - ease((frame - (i + 1) * CHAPTER + 18) / 36));
  return <>
    <Rect width={W} height={H} fill="#050915" />
    <Stars t={t} warp={weight(0)} />
    <Rings t={t} strength={weight(1)} />
    <Bokeh t={t} strength={weight(2)} />
    {range(5).map(i => {
      const opacity = weight(i);
      if (opacity <= 0) return null;
      return <Group key={i} opacity={opacity}>
        {i === 0 && <>
          <Circle x={825} y={405} radius={135} fill="#050915" stroke="#6DDFFF" strokeWidth={2} />
          <Text x={960} y={365} anchorX={0.5} style={style(240)} chromaticAberration={2}>SUPERNOVA</Text>
          <Text x={960} y={635} anchorX={0.5} style={{ ...style(32), letterSpacing: 12 }}>AN EXPEDITION INTO LIGHT</Text>
        </>}
        {i === 3 && <Kaleidoscope t={t} />}
        {i === 4 && <Finale t={t} />}
      </Group>;
    })}
    <Rect width={W} height={140} fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 140 },
      stops: [{ offset: 0, color: '#050915E0' }, { offset: 1, color: '#05091500' }] }} />
    <Rect y={900} width={W} height={180} fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 180 },
      stops: [{ offset: 0, color: '#05091500' }, { offset: 1, color: '#050915F0' }] }} />
    <Text x={64} y={48} style={{ ...style(25), letterSpacing: 4 }}>CELESTA / FIELD NOTES</Text>
    <Text x={1856} y={48} anchorX={1} style={{ ...style(25), letterSpacing: 3 }}>{NAMES[chapter]}</Text>
    <Text x={64} y={940} style={style(46)}>{CAPTIONS[chapter]}</Text>
    <Text x={1856} y={960} anchorX={1} style={style(23, '#8DADD0')}>{`${(frame / FPS).toFixed(2)} / 36.00 S`}</Text>
    <Rect x={64} y={1020} width={1792} height={2} fill="#FFFFFF22" />
    <Rect x={64} y={1020} width={1792 * frame / (DURATION - 1)} height={2} fill={COLORS[chapter]} />
  </>;
}

export default function Root() {
  return <Composition id="supernova" width={W} height={H} fps={FPS} durationInFrames={DURATION}>
    <Assets><Font src="../versus/bench/assets/fonts/BebasNeue-Regular.ttf" /></Assets>
    <Film />
  </Composition>;
}
