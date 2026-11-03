import { Composition, Group, Rect, Text, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { Circle, Line, Path } from '@celesta.studio/shapes';
import { chromaticAberration, noise, scanlines } from '@celesta.studio/shader';

// Built-in stylize effects. Each panel shows the same scene with one
// effect prop whose values change every frame; the last stacks three
// through the `shader` array. They run in the browser editor too.

const SIZE = 280;

type Panel = [label: string, props: (frame: number) => Record<string, unknown>];

const PANELS: Panel[] = [
  ['pixelate', frame => ({ pixelate: interpolate(frame, [0, 60, 120], [1, 28, 1]) })],
  ['halftone', frame => ({ halftone: { size: 12, angle: 45 + frame / 2 } })],
  ['chromaticAberration', frame => ({ chromaticAberration: { offset: 8 * Math.abs(Math.sin(frame / 12)), angle: 20 } })],
  ['wave', frame => ({ wave: { amplitude: 10, wavelength: 90, phase: frame * 12 } })],
  // No seed: the grain changes every frame, the same on every render.
  ['noise', () => ({ noise: 0.3 })],
  ['scanlines', frame => ({ scanlines: { amount: 0.6, spacing: 6, offset: frame / 4 } })],
  ['zoomBlur', frame => ({ zoomBlur: { amount: interpolate(frame, [0, 60, 120], [0, 0.35, 0]), centerY: 0.4 } })],
  ['shader array', () => ({ shader: [chromaticAberration(3), noise({ amount: 0.12, monochrome: false }), scanlines(0.35)] })],
];

export default function Root() {
  const frame = useCurrentFrame();
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={120}>
      <Rect width={1280} height={720} fill="#101014" />
      {PANELS.map(([label, props], index) => (
        <Group key={label} x={24 + (index % 4) * (SIZE + 32)} y={24 + Math.floor(index / 4) * (SIZE + 64)}>
          <Group {...props(frame)}>
            <Scene />
          </Group>
          <Text y={SIZE + 12} style={{ fontFamily: 'sans-serif', fontSize: 20, fill: { type: 'solid', color: '#d0d0d8' } }}>
            {label}
          </Text>
        </Group>
      ))}
    </Composition>
  );
}

/** A coastal poster with crisp architecture, sunset tones, and fine sea lines. */
function Scene() {
  return (
    <Group clip={{ width: SIZE, height: SIZE }}>
      <Rect
        width={SIZE}
        height={SIZE}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: SIZE }, stops: [{ offset: 0, color: '#343957' }, { offset: 0.5, color: '#ed9582' }, { offset: 1, color: '#ffe3ac' }] }}
      />
      <Circle x={170} y={49} radius={36} fill="#ffe9b2" />
      <Rect x={19} y={39} width={49} height={2} fill="#edb6a1" />
      <Rect x={34} y={46} width={46} height={2} fill="#edb6a1" />
      <Rect x={222} y={112} width={42} height={2} fill="#ffe0ba" />
      <Path points={[[0, 144], [37, 119], [61, 135], [103, 102], [157, 150]]} closed fill="#77718a" />
      <Path points={[[126, 151], [203, 123], [238, 136], [280, 117], [280, 164]]} closed fill="#555d79" />
      <Rect y={150} width={SIZE} height={130} fill="#31566e" />
      <Rect y={150} width={SIZE} height={3} fill="#ffe1b5" />
      {/* Narrow reflections make the filters' sampling easy to see. */}
      {[0, 1, 2, 3, 4, 5].map(i => (
        <Rect key={i} x={171 - i * 4} y={159 + i * 9} width={52 + i * 8} height={2} fill={i % 2 === 0 ? '#efc299' : '#8098a1'} />
      ))}
      <Rect x={10} y={169} width={43} height={2} fill="#8098a1" />
      <Rect x={25} y={183} width={30} height={2} fill="#8098a1" />
      <Rect x={239} y={194} width={27} height={2} fill="#a9bec0" />
      {/* A striped lighthouse and the rocky headland beneath it. */}
      <Path points={[[0, 222], [35, 201], [62, 204], [89, 184], [115, 199], [143, 211], [167, 235], [0, 252]]} closed fill="#192d42" />
      <Path points={[[29, 219], [61, 210], [87, 193], [111, 207], [80, 211], [66, 222]]} closed fill="#596c7a" />
      <Path points={[[78, 97], [101, 97], [111, 200], [68, 200]]} closed fill="#fff0d4" />
      <Path points={[[75, 128], [104, 128], [107, 154], [72, 154]]} closed fill="#d66356" />
      <Path points={[[99, 97], [101, 97], [111, 200], [99, 200]]} closed fill="#cba98f" />
      <Rect x={84} y={111} width={8} height={11} cornerRadius={3} fill="#223a50" />
      <Rect x={84} y={162} width={8} height={11} cornerRadius={3} fill="#223a50" />
      <Rect x={82} y={183} width={13} height={17} cornerRadius={6} fill="#223a50" />
      <Rect x={71} y={76} width={36} height={23} fill="#223a50" />
      <Rect x={76} y={79} width={26} height={15} fill="#ffda83" />
      <Line x1={89} y1={78} x2={89} y2={96} stroke="#223a50" strokeWidth={3} />
      <Path points={[[67, 76], [89, 59], [111, 76]]} closed fill="#192d42" />
      <Line x1={89} y1={53} x2={89} y2={59} stroke="#ffe9b2" strokeWidth={2} />
      <Rect x={65} y={98} width={48} height={5} fill="#192d42" />
      <Line x1={65} y1={91} x2={113} y2={91} stroke="#fff0d4" strokeWidth={1} />
      {[65, 77, 101, 113].map(x => (
        <Line key={x} x1={x} y1={88} x2={x} y2={98} stroke="#fff0d4" strokeWidth={1} />
      ))}
      <Path points={[[126, 91], [132, 87], [138, 91], [144, 87], [150, 91]]} stroke="#26374f" strokeWidth={2} />
      <Path points={[[224, 37], [228, 34], [232, 37], [236, 34], [240, 37]]} stroke="#ffdebe" strokeWidth={1.5} />
      <Rect y={229} width={SIZE} height={51} fill="#15273c" />
      <Rect x={20} y={229} width={240} height={2} fill="#e9b18d" />
      <Text x={SIZE / 2} y={241} anchorX={0.5} style={{ fontFamily: 'sans-serif', fontSize: 36, fontWeight: 800, fill: { type: 'solid', color: '#fff5e5' } }}>
        CELESTA
      </Text>
    </Group>
  );
}
