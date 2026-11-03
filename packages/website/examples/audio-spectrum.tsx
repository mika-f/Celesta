import { Audio, Composition, Rect } from '@celesta.studio/react';
import { loadAudioSpectrum, useAudioSpectrum } from '@celesta.studio/media-utils';
import type { AudioSpectrum } from '@celesta.studio/media-utils';

let music: AudioSpectrum;

// Decode and analyze the whole file once, before the first frame.
export async function prepare() {
  music = await loadAudioSpectrum('./media/music.wav', {
    bands: 24,
    smoothing: { attack: 0.01, release: 0.2 },
  });
}

function Bars() {
  // This frame's band levels (0–1) and loudness.
  const { bands, rms } = useAudioSpectrum(music);
  return (
    <>
      {bands.map((level, band) => (
        <Rect key={band} x={160 + band * 66} y={900} width={50}
          height={8 + level * 600} anchorY={1} fill="#4e62bc" />
      ))}
      <Rect x={960} y={180} width={200 + rms * 1200} height={24}
        anchorX={0.5} fill="#f55138" />
    </>
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={300}>
      <Bars />
      <Audio src={music.src!} />
    </Composition>
  );
}
