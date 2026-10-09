import { Audio, Composition, Sequence, Text, frameKeyframes } from '@celesta/react';

const FPS = 30;
const DURATION = 90;

// The BGM spans the whole composition: fade in over the first 15 frames and
// out over the last 15.
const bgmVolume = frameKeyframes(
  [
    { frame: 0, value: 0 },
    { frame: 15, value: 0.8, easing: 'ease-out' },
    { frame: DURATION - 15, value: 0.8 },
    { frame: DURATION, value: 0, easing: 'ease-in' },
  ],
  { fps: FPS },
);

// The voice sits in nested sequences that start at frame 30 + 15 = 45. Its
// keys are written in composition frames, so `origin` names the frame the
// voice's own sequence starts on.
const VOICE_FROM = 30 + 15;
const voiceVolume = frameKeyframes(
  [
    { frame: VOICE_FROM, value: 0 },
    { frame: VOICE_FROM + 6, value: 1 },
  ],
  { fps: FPS, origin: VOICE_FROM },
);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={FPS} durationInFrames={DURATION}>
      <Text>Volume fade</Text>
      <Audio src="./bgm.wav" volume={bgmVolume} />
      <Sequence from={30}>
        <Sequence from={15}>
          <Audio src="./voice.wav" volume={voiceVolume} />
        </Sequence>
      </Sequence>
    </Composition>
  );
}
