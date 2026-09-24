import { Audio, Composition, Text, useCurrentFrame } from '@celesta/react';

// Two things at once: an <Audio> behind an ordinary React conditional
// (gathered from each rendered frame's tree, so it only sounds on frames 15+)
// whose volume is a keyframed animation — the TypeScript mirror of the
// project format's Animatable<f64>, fading in over the first half second it
// is audible.
export default function Root() {
  const frame = useCurrentFrame();
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={60}>
      {frame >= 15 && (
        <Audio
          src="./voice.wav"
          volume={{
            type: 'keyframes',
            keyframes: [
              { time: { value: 0, timescale: 1 }, value: 0.25 },
              { time: { value: 500_000, timescale: 1_000_000 }, value: 1, easing: 'ease-in' },
            ],
          }}
        />
      )}
      <Text>{frame < 15 ? 'before' : 'after'}</Text>
    </Composition>
  );
}
