// A script read from a speech tool's export folder: importVoiceFolder()
// pairs each name.wav with its name.txt, so the lines, subtitles, lengths,
// and lip sync come from the files rather than from code. Order follows
// the names naturally (voice-2 before voice-10); manifest.json gives ids,
// a kana spokenText for the mouth shapes, and expressions.
//
// While this is open in the editor, resave a recording (or edit a .txt)
// under examples/assets/voice-folder/shizuku and the preview reloads with
// the new audio, subtitle, and timing; later lines move with it. A pair
// caught halfway through being written keeps the last good preview.
//
//   cargo run -p celesta-editor -- packages/cli/examples/with-voice-folder.tsx
//   cargo run -p celesta-exporter -- --react packages/cli/examples/with-voice-folder.tsx out.mp4
//
// Assets: examples/assets/voice-folder (see its README.md) and the
// dialogue-demo portraits.
import * as React from 'react';

import { Assets, Composition, Rect } from '@celesta.studio/react';
import { Character, CharacterView, DialogueSeries, importVoiceFolder, planDialogue } from '@celesta.studio/character';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterViewReference, DialoguePlan, VoiceLine } from '@celesta.studio/character';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const PORTRAITS = '../../../examples/assets/dialogue-demo/portraits';

const shizuku = React.createRef<AssetReference>();
const view = React.createRef<CharacterViewReference>();

let plan: DialoguePlan<VoiceLine>;

export async function prepare(): Promise<void> {
  const lines = await importVoiceFolder({
    directory: '../../../examples/assets/voice-folder/shizuku',
    manifest: '../../../examples/assets/voice-folder/shizuku/manifest.json',
    speaker: 'shizuku',
  });
  plan = await planDialogue(lines, { fps: FPS, gap: 0.4 });
}

export default function Root() {
  const mouth = (shape: string) => `${PORTRAITS}/mouth-${shape}.png`;
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames}>
      <Assets>
        <Character
          ref={shizuku}
          name="しずく"
          portrait={{
            defaultExpression: 'normal',
            expressions: { normal: `${PORTRAITS}/shizuku-normal.png`, smile: `${PORTRAITS}/shizuku-smile.png` },
            lipSync: { a: mouth('a'), i: mouth('i'), u: mouth('u'), e: mouth('e'), o: mouth('o'), closed: mouth('closed') },
          }}
          subtitle={{
            x: WIDTH / 2,
            y: 600,
            anchorX: 0.5,
            anchorY: 0.5,
            maxWidth: 1100,
            style: { fontSize: 40, fontWeight: 700, align: 'center', fill: { type: 'solid', color: '#ffffff' } },
          }}
        />
      </Assets>
      <Rect width={WIDTH} height={HEIGHT} fill="#3a4d63" />
      <CharacterView ref={view} character={shizuku} x={460} y={60} mouth="closed" />
      <DialogueSeries plan={plan} views={{ shizuku: view }} />
    </Composition>
  );
}
