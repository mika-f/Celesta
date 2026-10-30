# WAV duration from the header (2026-10-29)

`@celesta/voicevox`'s `wavDuration` no longer decodes the narration it
measures (issue #599). It used `decodeWav`, which converts every sample to
`Float32Array` and down-mixes to mono, only to divide the sample count by the
sample rate. It ran after every synthesis and on every cache hit, so a cached
script decoded all of its narration on each `prepareNarration` call.

## Investigation

### Who decodes WAVs, and what they need

| Caller | Needs |
| --- | --- |
| `voicevox` `wavDuration` (after synthesis, and in `readEntry` on a cache hit) | length only |
| `character` `loadLipSync`, `voice-folder.ts`, `spoken-text.ts` (`decodeWav` → `lipSyncFromAudio`) | samples (envelope) |
| `character` `voice-level.ts` (`decodeWavChannels`) | samples per channel (levels) |
| `aivis-speech` `timingFromAivisSpeech` (`decodeWav` when given bytes) | samples (`voicing()` reads RMS between moras) |
| `examples/feature-tour/voice.ts`, `examples/zunda/src/scenes/voice.tsx`, `packages/cli/examples/with-audio-spectrum.tsx`, `packages/web/test/engine-worker.test.mts` | samples (envelope / spectrum) |

`wavDuration` is the only caller that throws the samples away. The other
callers still decode, so they are unchanged here.

### What the removed work costs

- `prepareNarration` runs from an entry's `prepare()`. That runs once per
  entry load: on each export, and on each preview reload, which happens
  after the entry or a tracked dependency is saved and after an editor
  property edit rewrites the source. Every line of the script goes through
  `readEntry` on each of those runs, so every run measured every cached WAV.
- VOICEVOX synthesizes 24 kHz mono 16-bit PCM by default
  (`VOICEVOX_FRAME_RATE` in `packages/voicevox/src/timing.ts`): 48 kB per
  second of speech. For each second of speech, the old path allocated
  a 192 kB `Float64Array` channel and a 96 kB `Float32Array` mix, and ran two
  per-sample loops over 24,000 samples. A 10-minute script is a 28.8 MB set
  of WAVs, 14.4 million samples, and about 173 MB of short-lived arrays per
  run. `readWavHeader` reads only the chunk headers, so its cost does not
  depend on the audio length.
- A cache hit still reads the WAV and hashes it with SHA-256 to check the
  entry (`readEntry`), so the file I/O stays. Only the decode is removed.
- The decode was not benchmarked for this note. To measure it, build the
  runtime (`pnpm run build:runtime`), then run the following from
  `packages/character`. `SECONDS_OF_AUDIO` sets the synthetic WAV's length
  (default 600):

  ```bash
  SECONDS_OF_AUDIO=600 node --input-type=module -e '
  import { decodeWav, readWavHeader } from "@celesta/character";
  const seconds = Number(process.env.SECONDS_OF_AUDIO ?? 600);
  const rate = 24000, frames = seconds * rate, wav = Buffer.alloc(44 + frames * 2);
  wav.write("RIFF", 0); wav.writeUInt32LE(36 + frames * 2, 4); wav.write("WAVEfmt ", 8);
  wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(rate, 24); wav.writeUInt32LE(rate * 2, 28); wav.writeUInt16LE(2, 32); wav.writeUInt16LE(16, 34);
  wav.write("data", 36); wav.writeUInt32LE(frames * 2, 40);
  for (const [name, fn] of [["decodeWav", () => decodeWav(wav).samples.length], ["readWavHeader", () => readWavHeader(wav).frameCount]]) {
    fn(); const start = performance.now(); for (let i = 0; i < 5; i += 1) fn();
    console.log(name, ((performance.now() - start) / 5).toFixed(3), "ms per call");
  }'
  ```

### Can a header-only reader reject the same files?

Every check the old `readWav` made before its sample loop looks only at the
header: RIFF/WAVE magic, the chunk walk (with odd sizes padded to a word),
the fmt and data chunks being present, a fmt chunk shorter than 16 bytes,
a data chunk larger than the remaining file, WAVE_FORMAT_EXTENSIBLE's
sub-format, and zero channels or sub-byte samples. A data chunk of size 0 is
valid there (zero frames). `wavDuration` turns it into `it has no samples`.

The only check in the sample loop was `unsupported WAV sample size` for
PCM-like formats. It fired only when the file had at least one frame. The
format checks (`unsupported WAV format`, `unsupported IEEE float WAV sample
size`) were in `decodeWavChannels` only, so `decodeWav` never ran them.
Moving all three into the header parser lets the header-only path reject
every file the decoders reject. It also makes `decodeWav` reject the
non-PCM/float files that it used to decode wrongly (see "Behavior
differences").

### Docs

`@celesta/character`'s guide pages and `skills/celesta/` do not list the WAV
helpers. `decodeWav` appears only as the type of `lipSyncFromAivisSpeech`'s
argument (`04-aivis-speech.md` in en/ja, `skills/celesta/references/dialogue.md`).
`decodeWavChannels` is not mentioned anywhere. Both already say only
uncompressed PCM/float WAV is read
(`02-(guides)/07-dialogue.md`, `dialogue.md`). Adding `readWavHeader` and
making `decodeWav` enforce what they describe needs no doc change.

## What changed

- `packages/character/src/lipsync.ts`: the header part of the old `readWav`
  is now `parseWavHeader`. It checks RIFF/WAVE, finds the fmt and data
  chunks, rejects a truncated fmt or data chunk, resolves
  WAVE_FORMAT_EXTENSIBLE, rejects zero channels or sub-byte samples, rejects
  formats other than PCM (1) and IEEE float (3), and rejects sample sizes
  other than PCM 8/16/24/32-bit and float 32/64-bit. It returns the frame
  count, `floor(data.size / (bytesPerSample * channelCount))`, which is the
  length `decodeWav` returns.
- New export `readWavHeader(bytes): WavHeader` (`sampleRate`,
  `channelCount`, `audioFormat`, `bitsPerSample`, `frameCount`) returns that
  header without the data offset. `decodeWavChannels` parses the same header
  and then reads the samples, and `decodeWav` down-mixes
  `decodeWavChannels`. One parser serves all three, so the duration and the
  decoder accept and reject the same files.
- `packages/voicevox/src/narration.ts`: `wavDuration` is
  `frameCount / sampleRate` from `readWavHeader`, still throwing
  `it has no samples` when the sample rate is not positive or there are no
  frames.

## Behavior differences

- `decodeWav` used to skip the format check that `decodeWavChannels` ran.
  It decoded a mu-law, A-law, or other non-PCM WAV with a byte-sized sample
  as if it were PCM. It decoded a 16-bit "float" WAV by reading 32-bit floats
  at 2-byte steps. It accepted any sample size in a file with no frames. It
  now throws the same `unsupported WAV format …` /
  `unsupported IEEE float WAV sample size …` / `unsupported WAV sample size …`
  errors `decodeWavChannels` throws. The guide and the skill already say only
  uncompressed PCM or float WAV is read.
- For a non-PCM file, `wavDuration` now reports the format error instead of
  a duration. VOICEVOX returns 16-bit PCM, so engine output is unaffected.

## Tests

- `packages/character/test/wav-header.test.mts` checks every case against
  `decodeWav` and `decodeWavChannels`. It covers the bundled
  `examples/assets/voices/001.wav`, every supported format at 1/2/6 channels
  and three sample rates, WAVE_FORMAT_EXTENSIBLE, odd-sized chunks before
  `data` and a chunk after it, a data chunk that ends mid-frame, and an empty
  data chunk. Both functions must give the same frame count and sample rate,
  so the duration is the old one. For each invalid file both functions must
  throw the same message: not RIFF/WAVE, too short, no fmt, no data, short
  fmt, cut-off data, zero channels, zero bits, mu-law, MPEG, 16-bit float,
  12-bit and 40-bit PCM.
- `packages/voicevox/test/narration.test.ts` adds an engine that answers
  `/synthesis` with a header-only WAV. That is still reported as
  `not a readable WAV (it has no samples)` and nothing is cached. The
  existing tests still cover the measured duration, an engine body that is
  not a WAV, and a truncated cached WAV that is synthesized again.

## Checks

```bash
pnpm run build:runtime
pnpm --dir packages/character test
pnpm --dir packages/voicevox test
pnpm --dir packages/aivis-speech test
pnpm --dir packages/web exec vitest run test/engine-worker.test.mts
git diff --check origin/main...HEAD
```

`@celesta/aivis-speech` and the web engine-worker test also call `decodeWav`,
so they were run as well.

`readWavHeader` is a helper export like `decodeWavChannels`, which the user
guide and the Agent Skill do not list, so neither is changed.
