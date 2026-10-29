# WAV duration from the header (2026-10-29)

`@celesta/voicevox`'s `wavDuration` no longer decodes the narration it
measures (issue #599). It used `decodeWav`, which converts every sample to
`Float32Array` and down-mixes to mono, only to divide the sample count by the
sample rate. It ran after every synthesis and on every cache hit, so a cached
script decoded all of its narration on each `prepareNarration` call.

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
pnpm --filter @celesta/character run test
pnpm --filter @celesta/voicevox run test
pnpm --filter @celesta/aivis-speech run test
```

`readWavHeader` is a helper export like `decodeWavChannels`, which the user
guide and the Agent Skill do not list, so neither is changed.
