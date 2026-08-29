import { Audio, Composition, Sequence, Text, Video, useCurrentFrame } from '@mikan/react';

// Hooks read the enclosing <Sequence>'s shifted clock only when called from
// a component *inside* it — JSX children evaluate where they are written,
// so this caption reports its own local frame.
function Caption() {
  return (
    <Text
      x={320}
      y={180}
      anchorX={0.5}
      anchorY={0.5}
      style={{
        fontFamily: 'sans-serif',
        fontSize: 48,
        fill: { type: 'solid', color: '#ffffff' },
        align: 'center',
      }}
    >
      frame {useCurrentFrame()} inside
    </Text>
  );
}

// <Sequence> offsets its children on the composition's timeline: the video
// and audio start playing at frame 30 (1s in) synced to the sequence's own
// clock, and the caption is only present during frames 60-89. The audio
// clip's audible range follows the sequence that contains it.
export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={90}>
      <Sequence from={30}>
        <Video src="./clip.mp4" startFrom={1} playbackRate={2} />
        <Audio src="./voice.wav" />
      </Sequence>
      <Sequence from={60} durationInFrames={30}>
        <Caption />
      </Sequence>
    </Composition>
  );
}
