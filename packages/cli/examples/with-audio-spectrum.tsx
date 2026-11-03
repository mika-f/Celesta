import * as fs from 'node:fs';
import * as path from 'node:path';

import { Audio, Composition, Rect } from '@celesta.studio/react';
import { decodeWav } from '@celesta.studio/character';
import { analyzeAudioSpectrum, loadAudioSpectrum, useAudioSpectrum } from '@celesta.studio/media-utils';
import type { AudioSpectrum, AudioSpectrumOptions } from '@celesta.studio/media-utils';

// 001.wav is a ~1 s committed spoken fixture.
const VOICE = '../../../examples/assets/voices/001.wav';
const OPTIONS: AudioSpectrumOptions = { bands: 8, smoothing: { attack: 0.01, release: 0.15 } };

let music: AudioSpectrum;
// The same file read by the JavaScript WAV parser, as @celesta.studio/web decodes it.
let reference: AudioSpectrum;

export async function prepare(): Promise<void> {
  music = await loadAudioSpectrum(VOICE, OPTIONS);
  const bytes = await fs.promises.readFile(path.join(process.env.CELESTA_REACT_ENTRY_DIR ?? '.', VOICE));
  reference = analyzeAudioSpectrum(decodeWav(bytes), OPTIONS);
}

function Meter({ name, spectrum }: { name: string; spectrum: AudioSpectrum }) {
  const { bands, rms } = useAudioSpectrum(spectrum);
  return (
    <>
      <Rect id={`${name}-rms`} width={rms * 100} height={1} fill="#ffffff" />
      {bands.map((level, band) => (
        <Rect key={band} id={`${name}-band-${band}`} width={1} height={level * 100} fill="#ffffff" />
      ))}
    </>
  );
}

export default function Root() {
  return (
    <Composition width={320} height={180} fps={30} durationInFrames={45}>
      <Audio src={music.src!} />
      <Meter name="decoded" spectrum={music} />
      <Meter name="parsed" spectrum={reference} />
    </Composition>
  );
}
