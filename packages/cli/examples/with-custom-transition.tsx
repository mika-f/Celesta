import { Composition, Rect, Text, useVideoConfig } from '@celesta/react';
import { defineShader } from '@celesta/shader';
import { TransitionSeries, computeTransitionSeries, defineTransition } from '@celesta/transitions';

// Two transitions written with defineTransition(): blinds, which masks the
// next scene with slats that open, and pixelate, which runs a shader on both
// scenes. The built-in transitions are written the same way.

/** The next scene shows through slats that widen from the top of each. */
function blinds(slats: number) {
  return defineTransition({
    name: 'blinds',
    present: ({ progress, width, height }) => {
      const pitch = height / slats;
      const open = Math.min(1, Math.max(0, progress));
      return {
        enter: {
          mask: (
            <>
              {Array.from({ length: slats }, (_, index) => (
                <Rect key={index} y={index * pitch} width={width} height={pitch * open} fill="#FFFFFF" />
              ))}
            </>
          ),
        },
        // The leaving scene darkens behind them.
        exit: { brightness: 1 - 0.5 * open },
      };
    },
  });
}

// Each pixel takes the colour at the centre of its `size`-pixel block, kept
// inside the layer so the blocks along its edges are not half transparent.
const blocks = defineShader({
  name: 'blocks',
  params: { size: 'f32' },
  wgsl: `
fn effect(input: EffectInput) -> vec4f {
  let size = max(params.size, 1.0);
  let centre = (floor(input.position / size) + 0.5) * size;
  return source_at(clamp(centre, celesta.content.xy + 0.5, celesta.content.zw - 0.5));
}
`,
});

/** Both scenes break into blocks that grow, then shrink, as one fades into the other. */
const pixelate = defineTransition({
  name: 'pixelate',
  present: ({ progress }) => {
    // Largest halfway through, where the picture changes.
    const size = 2 + 46 * Math.sin(Math.PI * Math.min(1, Math.max(0, progress)));
    return {
      enter: { shader: blocks({ size }), opacity: progress < 0.5 ? 0 : 1 },
      exit: { shader: blocks({ size }) },
    };
  },
});

const SCENES = [
  { label: 'Blinds', color: '#22314F' },
  { label: 'Pixelate', color: '#B5651D' },
  { label: 'Done', color: '#2F6F62' },
];
const BLINDS = { type: blinds(8), durationInFrames: 24 };
const PIXELATE = { type: pixelate, durationInFrames: 30 };
const { durationInFrames } = computeTransitionSeries([
  { durationInFrames: 60 }, BLINDS, { durationInFrames: 60 }, PIXELATE, { durationInFrames: 60 },
]);

function Card({ label, color }: { label: string; color: string }) {
  const { width, height } = useVideoConfig();
  return (
    <>
      <Rect width={width} height={height} fill={color} />
      <Text x={width / 2} y={height / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontSize: 140, fontWeight: 800, fill: { type: 'solid', color: '#FFFFFF' } }}>
        {label}
      </Text>
    </>
  );
}

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={durationInFrames}>
      <TransitionSeries>
        <TransitionSeries.Sequence durationInFrames={60}><Card {...SCENES[0]} /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...BLINDS} />
        <TransitionSeries.Sequence durationInFrames={60}><Card {...SCENES[1]} /></TransitionSeries.Sequence>
        <TransitionSeries.Transition {...PIXELATE} />
        <TransitionSeries.Sequence durationInFrames={60}><Card {...SCENES[2]} /></TransitionSeries.Sequence>
      </TransitionSeries>
    </Composition>
  );
}
