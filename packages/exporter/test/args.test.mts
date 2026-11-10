import assert from 'node:assert/strict';
import { test } from 'vitest';

import { componentsArgs, compositionArgs, describeArgs, stillsArgs, videoArgs } from '../src/args.js';

test('a video export passes its options as --name=value flags', () => {
  assert.deepEqual(
    videoArgs({
      source: 'film.tsx',
      output: 'film.mp4',
      composition: 'intro',
      props: { title: 'Hi' },
      from: 1.5,
      to: '00:10',
      preset: 'web',
      subtitles: 'film.{lang}.vtt',
      overwrite: true,
      onProgress() {},
    }),
    [
      '--json',
      '--stdin-cancel',
      '--json-progress',
      '--react',
      '--composition=intro',
      '--props={"title":"Hi"}',
      '--overwrite',
      '--from=1.5',
      '--to=00:10',
      '--subtitles=film.{lang}.vtt',
      '--preset=web',
      '--',
      'film.tsx',
      'film.mp4',
    ],
  );
});

test('a video export passes its component flags', () => {
  const flags = videoArgs({
    source: 'film.tsx',
    output: 'film.mp4',
    muxer: 'ffmpeg/mp4',
    videoEncoder: { id: 'ffmpeg/libx264', options: { crf: 20, preset: 'slow' } },
    audioEncoder: 'none',
  }).filter((arg) => /^--(muxer|video-|audio-)/.test(arg));
  assert.deepEqual(flags, [
    '--muxer=ffmpeg/mp4',
    '--video-encoder=ffmpeg/libx264',
    '--video-option=crf=20',
    '--video-option=preset=slow',
    '--audio-encoder=none',
  ]);
  const audio = videoArgs({
    source: 'film.tsx',
    output: 'film.webm',
    presetFile: 'p.json',
    audioEncoder: { id: 'ffmpeg/opus', options: { bitrate: 96, vbr: true } },
  });
  assert.ok(audio.includes('--preset-file=p.json'));
  assert.deepEqual(audio.filter((arg) => arg.startsWith('--audio-')), [
    '--audio-encoder=ffmpeg/opus',
    '--audio-option=bitrate=96',
    '--audio-option=vbr=true',
  ]);
});

test('listComponents and describeComponent build their commands', () => {
  assert.deepEqual(componentsArgs({ check: true }), ['--json', '--list-components', '--check']);
  assert.deepEqual(componentsArgs(), ['--json', '--list-components']);
  assert.deepEqual(describeArgs('ffmpeg/libx264'), ['--json', '--describe', 'ffmpeg/libx264']);
});

test('a loudness target passes LUFS or a preset, with its peak options', () => {
  const flags = (options: Partial<Parameters<typeof videoArgs>[0]>) =>
    videoArgs({ source: 'film.tsx', output: 'film.mp4', ...options }).filter((arg) => /^--(loudness|true-peak|peak-handling)=/.test(arg));
  assert.deepEqual(flags({}), []);
  assert.deepEqual(flags({ loudness: -14, truePeak: -1.5, peakHandling: 'lower' }), ['--loudness=-14', '--true-peak=-1.5', '--peak-handling=lower']);
  assert.deepEqual(flags({ loudness: 'broadcast' }), ['--loudness=broadcast']);
});

test('a .json source is a project unless react says otherwise', () => {
  assert.deepEqual(compositionArgs({ source: 'demo.celesta.json' }), ['--json', '--stdin-cancel', '--info', '--', 'demo.celesta.json']);
  assert.ok(compositionArgs({ source: 'entry.json', react: true }).includes('--react'));
  assert.ok(compositionArgs({ source: 'film.TSX' }).includes('--react'));
});

test('seconds become millisecond timecodes', () => {
  const from = (time: number) => videoArgs({ source: 'p.celesta.json', output: 'o.mp4', from: time }).find((arg) => arg.startsWith('--from='));
  assert.equal(from(0), '--from=0');
  assert.equal(from(10), '--from=10');
  assert.equal(from(2.0004), '--from=2');
  assert.equal(from(1 / 3), '--from=0.333');
  assert.throws(() => from(-1), { code: 'usage' });
  assert.throws(() => from(Number.NaN), { code: 'usage' });
});

test('a video needs an output or subtitles', () => {
  assert.throws(() => videoArgs({ source: 'film.tsx' }), { name: 'CelestaExportError', code: 'usage' });
  assert.deepEqual(videoArgs({ source: 'film.tsx', subtitles: ['a.srt'] }).slice(-3), ['--subtitles=a.srt', '--', 'film.tsx']);
});

test('stills take frames or every, and contact sheet options', () => {
  assert.deepEqual(stillsArgs({ source: 'film.tsx', output: 'f.png', frames: [0, 90] }), [
    '--json',
    '--stdin-cancel',
    '--react',
    '--frame=0,90',
    '--',
    'film.tsx',
    'f.png',
  ]);
  assert.ok(stillsArgs({ source: 'film.tsx', output: 'f.png', frames: 3 }).includes('--frame=3'));
  const sheet = stillsArgs({ source: 'film.tsx', output: 's.webp', every: 30, from: 1, contactSheet: { columns: 4 }, quality: 80 });
  for (const flag of ['--every=30', '--from=1', '--quality=80', '--contact-sheet', '--columns=4']) {
    assert.ok(sheet.includes(flag), flag);
  }
  assert.throws(() => stillsArgs({ source: 'film.tsx', output: 'f.png' }), { code: 'usage' });
  assert.throws(() => stillsArgs({ source: 'film.tsx', output: 'f.png', frames: [1], every: 2 }), { code: 'usage' });
  assert.throws(() => stillsArgs({ source: 'film.tsx', output: 'f.png', frames: [1], from: 1 }), { code: 'usage' });
});

test('stills take a muxer and a video encoder', () => {
  const args = stillsArgs({
    source: 'film.tsx',
    output: 'f.png',
    frames: 0,
    muxer: 'builtin/image',
    videoEncoder: { options: { depth: 16 } },
  });
  assert.deepEqual(args.slice(args.indexOf('--muxer=builtin/image'), args.indexOf('--')), [
    '--muxer=builtin/image',
    '--video-option=depth=16',
  ]);
});

test('a source that starts with a dash stays positional', () => {
  assert.deepEqual(compositionArgs({ source: '-film.tsx' }).slice(-2), ['--', '-film.tsx']);
});
