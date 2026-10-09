# @celesta/voicevox

VOICEVOX AudioQuery lip sync for Celesta React compositions, including engines
that use the same query format. Dialogue components, WAV lip sync, and
`LipSyncTrack` are in `@celesta/character`.

```tsx
import { Dialogue } from '@celesta/character';
import { lipSyncFromVoicevox } from '@celesta/voicevox';
import helloQuery from './voices/hello.json';

const hello = lipSyncFromVoicevox(helloQuery);
// Inside the sequence that starts the voice:
<Dialogue character={view} audio="./voices/hello.wav" lipSync={hello}>
  こんにちは、ずんだもんなのだ。
</Dialogue>;
```

Use the query that produced the WAV. The track reflects speed, pauses, and
interrogative upspeak, with phoneme lengths rounded to the engine's frame grid.
Pass `{ frameRate: null }` to use unrounded lengths, or
`{ interrogativeUpspeak: false }` when synthesis disables upspeak.

Exports: `lipSyncFromVoicevox`, `voicevoxVowelShape`, and the types
`VoicevoxAudioQuery`, `VoicevoxAccentPhrase`, `VoicevoxMora`,
`VoicevoxLipSyncOptions`.

Building `@celesta/cli` also builds this package and stages its TypeScript
support. Celesta's desktop runtime ships it separately and the CLI resolves it
when a composition imports it. **File > Set Up TypeScript** includes its types
and import mapping, without requiring an install in the composition's project.
The package is private and has not been published to npm.

```sh
pnpm install
pnpm --dir packages/react run codegen
pnpm --filter "@celesta/cli..." run build
pnpm --dir packages/voicevox run test
```

`@celesta/character` does not depend on or re-export the adapter.
