// The VRM plugin's demo: Seed-san from the bust up, relaxed, playing a VRM
// animation on a loop, nodding once, looking at the camera and mouthing the
// vowels. Neither Seed-san nor the animation is in the repository, and the
// demo needs both: see README.md for where to download `Seed-san.vrm` and
// `test.vrma` into plugins/vrm/assets/. Build the plugin and the codec plugin
// first, then export with their directory:
//
//   node scripts/plugins/build.mjs plugins/vrm plugins/ffmpeg
//   cargo run -p celesta-exporter -- --react plugins/vrm/demo.tsx seed-san.mp4 --plugin-dir plugins/dist
import { Composition, Rect, useCurrentFrame } from '@celesta.studio/react';
import { Vrm } from '@celesta-plugin/vrm';

const VOWELS = ['a', 'i', 'u', 'e', 'o'] as const;

function Avatar() {
  const frame = useCurrentFrame();
  // A new vowel every 10 frames.
  const mouth = VOWELS[Math.floor(frame / 10) % VOWELS.length];
  return (
    <Vrm
      src="./assets/Seed-san.vrm"
      camera="bust"
      lookAt="camera"
      expressions={{ relaxed: 0.3 }}
      mouth={mouth}
      // Needs ./assets/test.vrma (see README.md); the demo does not run without it.
      motions={[{ src: './assets/test.vrma', loop: true }]}
      // A nod from frame 60 to 90, on top of the animation: a negative
      // pitch looks down.
      tracks={{
        head: {
          pitch: [
            { frame: 60, value: 0 },
            { frame: 75, value: -15, easing: 'ease-in-out' },
            { frame: 90, value: 0, easing: 'ease-in-out' },
          ],
        },
      }}
    />
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={180}>
      <Rect width={1920} height={1080} fill="#26364a" />
      <Avatar />
    </Composition>
  );
}
