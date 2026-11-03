// The VRM plugin's demo: Seed-san from the bust up, relaxed, looking at the
// camera and mouthing the vowels. Seed-san is not in the repository; see
// README.md for where to download it. Build the plugin first, then export
// with its directory:
//
//   node plugins/vrm/build.mjs
//   cargo run -p celesta-exporter -- --react plugins/vrm/demo.tsx seed-san.mp4 --plugin-dir plugins/vrm/dist
import { Composition, Rect, useCurrentFrame } from '@celesta/react';
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
