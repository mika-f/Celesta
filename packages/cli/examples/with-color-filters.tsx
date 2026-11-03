import { Composition, Group, Rect, Text, interpolate, useCurrentFrame } from '@celesta.studio/react';
import { grayscale, tint, vignette } from '@celesta.studio/shader';

// Built-in color filters. The top row shows each filter prop on the same
// swatch; the bottom panel grades a whole group with props that animate,
// and an "old photo" look that orders its filters through the `shader`
// array. They run in the browser editor too.

const FILTERS = [
  ['original', {}],
  ['brightness 1.4', { brightness: 1.4 }],
  ['contrast 1.6', { contrast: 1.6 }],
  ['saturation 0', { saturation: 0 }],
  ['hue 120', { hue: 120 }],
  ['grayscale 1', { grayscale: 1 }],
  ['invert 1', { invert: 1 }],
  ['tint', { tint: { color: '#2060ff', amount: 0.5 } }],
  ['vignette', { vignette: { amount: 1, radius: 0.2, softness: 0.6 } }],
] as const;

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Rect width={1280} height={720} fill="#101014" />
      {FILTERS.map(([label, props], index) => (
        <Group key={label} x={40 + index * 136} y={40}>
          <Swatch {...props} />
          <Text y={140} style={{ fontFamily: 'sans-serif', fontSize: 16, fill: { type: 'solid', color: '#d0d0d8' } }}>
            {label}
          </Text>
        </Group>
      ))}
      <Graded />
      <OldPhoto />
    </Composition>
  );
}

/** Four colors and a gray ramp, with the filter props given. */
function Swatch(props: Record<string, unknown>) {
  const colors = ['#e83a30', '#f0b020', '#30b860', '#3070e0'];
  return (
    <Group {...props}>
      {colors.map((color, index) => (
        <Rect key={color} x={(index % 2) * 60} y={Math.floor(index / 2) * 60} width={60} height={60} fill={color} />
      ))}
      <Rect
        y={120}
        width={120}
        height={8}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 120, y: 0 }, stops: [{ offset: 0, color: '#000000' }, { offset: 1, color: '#ffffff' }] }}
      />
    </Group>
  );
}

/** A landscape of shapes whose hue and saturation change every frame. */
function Graded() {
  const frame = useCurrentFrame();
  return (
    <Group x={40} y={240} hue={interpolate(frame, [0, 90], [0, 90])} saturation={interpolate(frame, [0, 45, 90], [1, 1.6, 1])} vignette={0.7}>
      <Landscape />
    </Group>
  );
}

/** The same landscape, gray first, then tinted sepia, then darkened at the edges. */
function OldPhoto() {
  return (
    <Group
      x={660}
      y={240}
      contrast={1.1}
      shader={[grayscale(), tint({ color: '#704214', amount: 0.45 }), vignette({ amount: 0.8, radius: 0.3 })]}
    >
      <Landscape />
    </Group>
  );
}

function Landscape() {
  return (
    <>
      <Rect
        width={580}
        height={440}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 440 }, stops: [{ offset: 0, color: '#4aa0f0' }, { offset: 1, color: '#f6d39a' }] }}
      />
      <Rect x={400} y={60} width={100} height={100} cornerRadius={50} fill="#ffe070" />
      <Rect y={300} width={580} height={140} fill="#3c8a3c" />
      <Rect x={60} y={200} width={120} height={140} fill="#c04a30" />
      <Rect x={50} y={170} width={140} height={40} fill="#6a2a20" />
      <Text x={290} y={380} anchorX={0.5} style={{ fontFamily: 'sans-serif', fontSize: 40, fontWeight: 800, fill: { type: 'solid', color: '#ffffff' } }}>
        CELESTA
      </Text>
    </>
  );
}
