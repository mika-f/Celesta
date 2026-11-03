import { Audio, Composition, Sequence, Text, frameKeyframes } from '@celesta.studio/react';

const FPS = 30;
const DURATION = 90;

// The voice plays from frame 45 for 30 frames.
const VOICE_FROM = 45;
const VOICE_FRAMES = 30;

// The BGM starts at frame 15, fades in and out with `fadeIn` / `fadeOut`,
// and dips under the voice with keyframes. The keys are written in
// composition frames, so `origin` names the frame the BGM's sequence starts
// on.
const BGM_FROM = 15;
const bgmVolume = frameKeyframes(
  [
    { frame: VOICE_FROM - 5, value: 0.8 },
    { frame: VOICE_FROM, value: 0.3, easing: 'ease-out' },
    { frame: VOICE_FROM + VOICE_FRAMES, value: 0.3 },
    { frame: VOICE_FROM + VOICE_FRAMES + 5, value: 0.8, easing: 'ease-in' },
  ],
  { fps: FPS, origin: BGM_FROM },
);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={FPS} durationInFrames={DURATION}>
      <Text>Volume fade</Text>
      <Sequence from={BGM_FROM}>
        <Audio src="./bgm.wav" volume={bgmVolume} fadeIn={0.5} fadeOut={0.5} />
      </Sequence>
      <Sequence from={VOICE_FROM} durationInFrames={VOICE_FRAMES}>
        <Audio src="./voice.wav" />
      </Sequence>
    </Composition>
  );
}
