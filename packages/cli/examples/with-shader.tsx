import { Composition, Text, Video, spring, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { defineShader } from '@celesta/shader';
import rgbSplitSource from './rgb-split.wgsl';
import rippleSource from './ripple.wgsl';

// Custom WGSL filters: a ripple on a title, and an RGB split on a video
// whose amount springs back to zero. The shaders get time and amounts as
// parameters, so they follow the composition's frames like any other prop.
const ripple = defineShader({
  name: 'ripple',
  wgsl: rippleSource,
  params: { time: 'f32', amplitude: { type: 'f32', default: 6 } },
  // The ripple pushes pixels up to `amplitude` past the text's edge.
  padding: 8,
});

const rgbSplit = defineShader({
  name: 'rgb-split',
  wgsl: rgbSplitSource,
  params: { amount: 'f32' },
  padding: 24,
});

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Footage />
      <Title />
    </Composition>
  );
}

function Footage() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const settle = spring({ frame, fps, config: { damping: 12 } });
  return <Video src="./clip.mp4" shader={rgbSplit({ amount: (1 - settle) * 24 })} />;
}

function Title() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <Text
      x={640}
      y={360}
      anchorX={0.5}
      anchorY={0.5}
      style={{ fontFamily: 'sans-serif', fontSize: 120, fontWeight: 900, fill: { type: 'solid', color: '#ffffff' } }}
      shader={ripple({ time: frame / fps })}
    >
      CELESTA
    </Text>
  );
}
