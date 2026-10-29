// readWavHeader reports a WAV's sample rate and frame count from its fmt and
// data chunks alone. Every case here also runs through decodeWav, so the
// header-only reader is pinned to the decoder: the same frame count for
// readable files, and the same error for unreadable ones.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { decodeWav, decodeWavChannels, readWavHeader } from '../src/index.js';

const voicePath = fileURLToPath(new URL('../../../examples/assets/voices/001.wav', import.meta.url));

interface WavSpec {
  audioFormat?: number;
  channels?: number;
  bits?: number;
  sampleRate?: number;
  frames?: number;
  /** Overrides the data chunk's declared size (defaults to the frames' size). */
  dataSize?: number;
  /** Writes a WAVE_FORMAT_EXTENSIBLE fmt chunk whose sub-format is `audioFormat`. */
  extensible?: boolean;
  /** Cuts the fmt chunk's payload to this many bytes. */
  fmtLength?: number;
  /** Chunks written between fmt and data, as [id, payload size]. */
  before?: [string, number][];
  /** Chunks written after data. */
  after?: [string, number][];
}

function chunk(id: string, payload: Buffer): Buffer {
  const header = Buffer.alloc(8);
  header.write(id, 0, 'ascii');
  header.writeUInt32LE(payload.length, 4);
  return Buffer.concat([header, payload, Buffer.alloc(payload.length & 1)]);
}

/** A WAV whose samples are a byte ramp: arbitrary, but the same every run. */
function wav({
  audioFormat = 1,
  channels = 1,
  bits = 16,
  sampleRate = 24000,
  frames = 240,
  dataSize,
  extensible = false,
  fmtLength,
  before = [],
  after = [],
}: WavSpec = {}): Buffer {
  const blockAlign = channels * (bits >> 3);
  const fmt = Buffer.alloc(extensible ? 40 : 16);
  fmt.writeUInt16LE(extensible ? 0xfffe : audioFormat, 0);
  fmt.writeUInt16LE(channels, 2);
  fmt.writeUInt32LE(sampleRate, 4);
  fmt.writeUInt32LE(sampleRate * blockAlign, 8);
  fmt.writeUInt16LE(blockAlign, 12);
  fmt.writeUInt16LE(bits, 14);
  if (extensible) {
    fmt.writeUInt16LE(22, 16);
    fmt.writeUInt16LE(bits, 18);
    fmt.writeUInt16LE(audioFormat, 24);
  }
  const samples = Buffer.alloc(frames * blockAlign);
  for (let i = 0; i < samples.length; i += 1) samples[i] = (i * 37) & 0xff;
  const data = chunk('data', samples);
  if (dataSize !== undefined) data.writeUInt32LE(dataSize, 4);
  const body = Buffer.concat([
    Buffer.from('WAVE', 'ascii'),
    chunk('fmt ', fmt.subarray(0, fmtLength)),
    ...before.map(([id, size]) => chunk(id, Buffer.alloc(size))),
    data,
    ...after.map(([id, size]) => chunk(id, Buffer.alloc(size))),
  ]);
  const riff = Buffer.alloc(8);
  riff.write('RIFF', 0, 'ascii');
  riff.writeUInt32LE(body.length, 4);
  return Buffer.concat([riff, body]);
}

/** Asserts readWavHeader agrees with what decodeWav decodes. */
function assertMatchesDecoder(bytes: Uint8Array, expectedFrames: number): void {
  const header = readWavHeader(bytes);
  const decoded = decodeWav(bytes);
  assert.equal(header.frameCount, expectedFrames);
  assert.equal(header.frameCount, decoded.samples.length);
  assert.equal(header.sampleRate, decoded.sampleRate);
  assert.equal(header.frameCount / header.sampleRate, decoded.samples.length / decoded.sampleRate);
}

test('readWavHeader matches decodeWav on the bundled voice', () => {
  const bytes = readFileSync(voicePath);
  const decoded = decodeWav(bytes);
  assertMatchesDecoder(bytes, decoded.samples.length);
  assert.ok(decoded.samples.length > 0);
});

test('readWavHeader counts frames for every supported sample format', () => {
  const formats: [audioFormat: number, bits: number][] = [
    [1, 8],
    [1, 16],
    [1, 24],
    [1, 32],
    [3, 32],
    [3, 64],
  ];
  for (const [audioFormat, bits] of formats) {
    for (const channels of [1, 2, 6]) {
      for (const sampleRate of [8000, 44100, 48000]) {
        const bytes = wav({ audioFormat, bits, channels, sampleRate, frames: 333 });
        assertMatchesDecoder(bytes, 333);
        const decoded = decodeWavChannels(bytes);
        assert.deepEqual(readWavHeader(bytes), {
          sampleRate,
          channelCount: decoded.channels.length,
          audioFormat: decoded.audioFormat,
          bitsPerSample: decoded.bitsPerSample,
          frameCount: decoded.channels[0].length,
        });
      }
    }
  }
});

test('readWavHeader reads WAVE_FORMAT_EXTENSIBLE, extra chunks, and a partial last frame like decodeWav', () => {
  assertMatchesDecoder(wav({ extensible: true, audioFormat: 3, bits: 32, channels: 2, frames: 100 }), 100);
  assert.equal(readWavHeader(wav({ extensible: true, audioFormat: 3, bits: 32 })).audioFormat, 3);
  // An odd-sized chunk is padded to a word boundary before the next one.
  assertMatchesDecoder(wav({ before: [['LIST', 3], ['fact', 4]], after: [['id3 ', 7]], frames: 50 }), 50);
  // A data chunk that ends mid-frame drops the partial frame.
  assertMatchesDecoder(wav({ channels: 2, bits: 16, frames: 10, dataSize: 10 * 4 - 1 }), 9);
  assertMatchesDecoder(wav({ frames: 0 }), 0);
});

test('readWavHeader rejects every WAV decodeWav rejects, with the same message', () => {
  const valid = wav();
  const missing = (id: string): Buffer => {
    const bytes = Buffer.from(valid);
    bytes.write(id === 'fmt ' ? 'junk' : 'junq', id === 'fmt ' ? 12 : 36, 'ascii');
    return bytes;
  };
  const cases: [name: string, bytes: Uint8Array, message: RegExp][] = [
    ['an engine error body', Buffer.from('RIFF....nope'), /not a RIFF\/WAVE file/],
    ['fewer than 12 bytes', Buffer.from('RIFF'), /not a RIFF\/WAVE file/],
    ['a non-WAVE RIFF file', Buffer.concat([Buffer.from('RIFF\0\0\0\0AVI '), valid.subarray(12)]), /not a RIFF\/WAVE file/],
    ['no fmt chunk', missing('fmt '), /missing its fmt or data chunk/],
    ['no data chunk', missing('data'), /missing its fmt or data chunk/],
    ['a short fmt chunk', wav({ fmtLength: 8 }), /fmt chunk is truncated/],
    ['a cut-off file', valid.subarray(0, 100), /data chunk is truncated: it declares 480 bytes but the file holds 56/],
    ['no channels', wav({ channels: 0 }), /declares 0 channels of 16-bit samples/],
    ['zero-bit samples', wav({ bits: 0, frames: 0 }), /declares 1 channels of 0-bit samples/],
    ['mu-law', wav({ audioFormat: 7, bits: 8 }), /unsupported WAV format 7/],
    ['MPEG audio', wav({ audioFormat: 0x55, frames: 0 }), /unsupported WAV format 85/],
    ['16-bit float', wav({ audioFormat: 3, bits: 16 }), /unsupported IEEE float WAV sample size: 16-bit/],
    ['12-bit PCM', wav({ bits: 12 }), /unsupported WAV sample size: 12-bit \(format 1\)/],
    ['40-bit PCM', wav({ bits: 40 }), /unsupported WAV sample size: 40-bit \(format 1\)/],
  ];
  for (const [name, bytes, message] of cases) {
    assert.throws(() => readWavHeader(bytes), message, `readWavHeader on ${name}`);
    assert.throws(() => decodeWav(bytes), message, `decodeWav on ${name}`);
  }
});
