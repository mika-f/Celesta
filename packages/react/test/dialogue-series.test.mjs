import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { planDialogue } from '../dist/index.js';
import { setMediaProbe } from '../dist/media.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

/** Seconds of each fake voice file, by base name. */
const VOICES = { 'a.wav': 1, 'b.wav': 0.5, 'c.wav': 2.01, 'd.wav': 0.2 };

function voiceDir() {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-dialogue-'));
  for (const name of Object.keys(VOICES)) {
    writeFileSync(join(dir, name), '');
  }
  return dir;
}

function fakeProbe(path) {
  const seconds = VOICES[path.split(/[\\/]/).pop()];
  return seconds === undefined
    ? { error: `cannot probe ${path}` }
    : { media: { durationSeconds: seconds, audio: [{ durationSeconds: seconds }] } };
}

/** Runs `fn` with `preloadMedia()` answered from VOICES, relative to a temp entry dir. */
async function withVoices(fn) {
  const dir = voiceDir();
  const previous = process.env.CELESTA_REACT_ENTRY_DIR;
  process.env.CELESTA_REACT_ENTRY_DIR = dir;
  const probed = [];
  setMediaProbe(async (path) => {
    probed.push(path);
    const response = fakeProbe(path);
    if (response.error) throw new Error(response.error);
    return response.media;
  });
  try {
    return await fn(probed);
  } finally {
    if (previous === undefined) delete process.env.CELESTA_REACT_ENTRY_DIR;
    else process.env.CELESTA_REACT_ENTRY_DIR = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}

test('planDialogue lays lines out from their voice lengths, gaps, and lead-ins', async () => {
  await withVoices(async (probed) => {
    const plan = await planDialogue(
      [
        { id: 'hello', text: 'Hello', audio: './a.wav', scene: 'intro' },
        { id: 'short', text: 'Hi', audio: 'b.wav', gap: 1, scene: 'intro' },
        { id: 'topic', text: 'Topic', audio: './c.wav', scene: 'body' },
        { id: 'silent', text: '…', durationInFrames: 12, leadIn: 0.5, gap: 0, scene: 'body' },
        { text: 'Bye', audio: './a.wav', scene: 'outro', leadIn: 0 },
      ],
      { fps: 30, sceneLeadIn: 1 },
    );
    // a: 30f + 0.25s (8f) gap; b: 15f + 30f gap; c: ceil(60.3)=61f after a
    // 30f scene lead-in; silent: 15f lead-in, 12f, no gap; last: no lead-in.
    assert.deepEqual(
      plan.lines.map(({ id, from, durationInFrames, gapInFrames, leadInFrames, spanInFrames }) => ({
        id, from, durationInFrames, gapInFrames, leadInFrames, spanInFrames,
      })),
      [
        { id: 'hello', from: 0, durationInFrames: 30, gapInFrames: 8, leadInFrames: 0, spanInFrames: 38 },
        { id: 'short', from: 38, durationInFrames: 15, gapInFrames: 30, leadInFrames: 0, spanInFrames: 45 },
        { id: 'topic', from: 113, durationInFrames: 61, gapInFrames: 8, leadInFrames: 30, spanInFrames: 69 },
        { id: 'silent', from: 197, durationInFrames: 12, gapInFrames: 0, leadInFrames: 15, spanInFrames: 12 },
        { id: '4', from: 209, durationInFrames: 30, gapInFrames: 8, leadInFrames: 0, spanInFrames: 38 },
      ],
    );
    assert.equal(plan.durationInFrames, 247);
    assert.equal(plan.startOf('topic'), 113);
    assert.equal(plan.line('silent').line.text, '…');
    assert.deepEqual(plan.range('hello', 'short'), { from: 0, durationInFrames: 83 });
    assert.deepEqual(plan.range('topic'), { from: 113, durationInFrames: 69 });
    assert.deepEqual(
      plan.scenes.map(({ id, from, durationInFrames, lines }) => ({ id, from, durationInFrames, lines: lines.map((l) => l.id) })),
      [
        { id: 'intro', from: 0, durationInFrames: 83, lines: ['hello', 'short'] },
        { id: 'body', from: 83, durationInFrames: 126, lines: ['topic', 'silent'] },
        { id: 'outro', from: 209, durationInFrames: 38, lines: ['4'] },
      ],
    );
    assert.deepEqual(plan.scene('body'), plan.scenes[1]);
    // Explicit lengths are not probed; a repeated voice is probed once.
    assert.equal(probed.filter((path) => path.endsWith('a.wav')).length, 1);
    assert.equal(probed.length, 3);
    assert.throws(() => plan.startOf('nope'), /no line "nope"/);
    assert.throws(() => plan.scene('nope'), /no scene "nope"/);
    assert.throws(() => plan.range('topic', 'hello'), /comes before/);
  });
});

test('planDialogue defaults to a short gap and is deterministic', async () => {
  await withVoices(async () => {
    const lines = [{ text: 'a', audio: 'a.wav' }, { text: 'b', audio: 'b.wav' }];
    const first = await planDialogue(lines, { fps: 24 });
    const second = await planDialogue(lines, { fps: 24 });
    assert.deepEqual(first.lines.map((l) => [l.from, l.gapInFrames]), [[0, 6], [30, 6]]);
    assert.equal(first.durationInFrames, 48);
    assert.deepEqual(second.lines.map((l) => l.from), first.lines.map((l) => l.from));
  });
});

test('planDialogue explains missing voices and bad input', async () => {
  await withVoices(async () => {
    await assert.rejects(
      planDialogue([{ id: 'intro', text: 'x', audio: './voices/missing.wav' }], { fps: 30 }),
      /voice file for line "intro" not found: \.\/voices\/missing\.wav \(looked at .*missing\.wav\)/,
    );
    await assert.rejects(planDialogue([{ text: 'x' }], { fps: 30 }), /line "0" needs `audio` or `durationInFrames`/);
    await assert.rejects(
      planDialogue([{ id: 'a', text: 'x', durationInFrames: 3 }, { id: 'a', text: 'y', durationInFrames: 3 }], { fps: 30 }),
      /line id "a" is used more than once/,
    );
    await assert.rejects(planDialogue([{ text: 'x', durationInFrames: 3, gap: -1 }], { fps: 30 }), /gap must be/);
    await assert.rejects(planDialogue([], { fps: 0 }), /positive finite fps/);
    await assert.rejects(
      planDialogue(
        [
          { text: 'x', durationInFrames: 3, scene: 's' },
          { text: 'y', durationInFrames: 3, scene: 't' },
          { text: 'z', durationInFrames: 3, scene: 's' },
        ],
        { fps: 30 },
      ),
      /scene "s" is split/,
    );
  });
});

/**
 * Runs an entry through the CLI, answering media probes from VOICES, and
 * returns `{ config, frames }` for the requested frames.
 */
async function renderEntry(t, source, frames) {
  const dir = voiceDir();
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, source);
  const child = spawn(process.execPath, [cli, entry], { stdio: ['pipe', 'pipe', 'pipe'] });
  t.signal.addEventListener('abort', () => child.kill(), { once: true });
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const exited = new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', (code) => resolve(code));
  });
  const send = (value) => child.stdin.write(JSON.stringify(value) + '\n');
  const time = (frame) => ({ time: { value: frame, timescale: 30 } });
  let config;
  const results = [];
  try {
    for await (const line of createInterface({ input: child.stdout })) {
      const message = JSON.parse(line);
      if (message.probeMedia) {
        send(fakeProbe(message.probeMedia.path));
      } else if (message.config) {
        config = message.config;
        send(time(frames[0]));
      } else {
        assert.equal(message.error, undefined, message.error);
        results.push(message);
        if (results.length < frames.length) send(time(frames[results.length]));
        else child.stdin.end();
      }
    }
    assert.equal(await exited, 0, stderr);
    return { config, frames: results };
  } finally {
    child.kill();
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Every text layer's string, depth first. */
function texts(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'text' ? [layer.content.text] : layer.content.type === 'group' ? texts(layer.content.layers) : [],
  );
}

/** Every image asset path, depth first. */
function images(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'image'
      ? [layer.content.asset.location.path.split(/[\\/]/).pop()]
      : layer.content.type === 'group' ? images(layer.content.layers) : [],
  );
}

test('DialogueSeries plays each planned line in its window with its voice and speaker', { timeout: 15000 }, async (t) => {
  const { config, frames } = await renderEntry(
    t,
    `
    import * as React from 'react';
    import { Assets, Character, CharacterView, Composition, DialogueSeries, Rect, Sequence, planDialogue } from '@celesta/react';
    const mira = React.createRef();
    const hana = React.createRef();
    const miraView = React.createRef();
    const hanaView = React.createRef();
    let plan;
    export async function prepare() {
      plan = await planDialogue([
        { id: 'one', text: 'First', audio: './a.wav', speaker: 'mira' },
        { id: 'two', text: 'Second', audio: './b.wav', speaker: 'hana', expression: 'smile', scene: 'cut', leadIn: 0.5 },
        { id: 'three', text: 'Third', audio: './a.wav', character: miraView, gap: 0, volume: 0.5 },
      ], { fps: 30 });
    }
    const portrait = (name) => ({ defaultExpression: 'calm', expressions: { calm: name + '-calm.png', smile: name + '-smile.png' } });
    export default function Root() {
      return (
        <Composition width={320} height={180} fps={30} durationInFrames={plan.durationInFrames}>
          <Assets>
            <Character ref={mira} name="Mira" portrait={portrait('mira')} />
            <Character ref={hana} name="Hana" portrait={portrait('hana')} />
          </Assets>
          <CharacterView ref={miraView} character={mira} />
          <CharacterView ref={hanaView} character={hana} />
          <Sequence {...plan.scene('cut')}><Rect id="cut" width={1} height={1} /></Sequence>
          <DialogueSeries plan={plan} views={{ mira: miraView, hana: hanaView }} />
        </Composition>
      );
    }
    `,
    [0, 29, 30, 52, 53, 67, 68, 98],
  );
  // one: 0–30 (+8 gap); two: lead-in 15, 53–68 (+8); three: 76–106, no gap.
  assert.equal(config.durationInFrames, 106);
  assert.deepEqual(
    frames.map((frame) => texts(frame.scene.layers)),
    [['First'], ['First'], [], [], ['Second'], ['Second'], [], ['Third']],
  );
  assert.deepEqual(images(frames[4].scene.layers), ['mira-calm.png', 'hana-smile.png']);
  assert.deepEqual(images(frames[7].scene.layers), ['mira-calm.png', 'hana-calm.png']);
  // The scene window starts at its first line's lead-in.
  assert.deepEqual(
    frames.map((frame) => frame.scene.layers.some((layer) => layer.id === 'cut')),
    [false, false, false, true, true, true, true, true],
  );
  // Clip windows in composition frames.
  const clips = (frame) => frame.audio.map(({ src, start, duration, volume }) => ({
    src: src.split(/[\\/]/).pop(), start: Math.round(start * 30), frames: Math.round(duration * 30), volume,
  }));
  assert.deepEqual(clips(frames[0]), [{ src: 'a.wav', start: 0, frames: 30, volume: 1 }]);
  assert.deepEqual(clips(frames[2]), []);
  assert.deepEqual(clips(frames[4]), [{ src: 'b.wav', start: 53, frames: 15, volume: 1 }]);
  assert.deepEqual(clips(frames[7]), [{ src: 'a.wav', start: 76, frames: 30, volume: 0.5 }]);
});

test('DialogueSeries can hold subtitles through the gap and reports unmapped speakers', { timeout: 15000 }, async (t) => {
  const source = (series) => `
    import * as React from 'react';
    import { Assets, Character, CharacterView, Composition, DialogueSeries, planDialogue } from '@celesta/react';
    const mira = React.createRef();
    const view = React.createRef();
    let plan;
    export async function prepare() {
      plan = await planDialogue([{ text: 'Only', audio: 'b.wav', speaker: 'mira' }], { fps: 30, gap: 1 });
    }
    export default function Root() {
      return (
        <Composition width={320} height={180} fps={30} durationInFrames={plan.durationInFrames}>
          <Assets><Character ref={mira} name="Mira" portrait={{ defaultExpression: 'calm', expressions: { calm: 'calm.png' } }} /></Assets>
          <CharacterView ref={view} character={mira} />
          ${series}
        </Composition>
      );
    }
  `;
  const held = await renderEntry(t, source('<DialogueSeries plan={plan} views={{ mira: view }} holdThroughGap />'), [14, 15, 44]);
  assert.deepEqual(held.frames.map((frame) => texts(frame.scene.layers)), [['Only'], ['Only'], ['Only']]);

  const dir = voiceDir();
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, source('<DialogueSeries plan={plan} views={{}} />'));
  try {
    const child = spawn(process.execPath, [cli, entry], { stdio: ['pipe', 'pipe', 'pipe'] });
    t.signal.addEventListener('abort', () => child.kill(), { once: true });
    const lines = createInterface({ input: child.stdout });
    let error;
    for await (const line of lines) {
      const message = JSON.parse(line);
      if (message.probeMedia) child.stdin.write(JSON.stringify(fakeProbe(message.probeMedia.path)) + '\n');
      else if (message.error) { error = message.error; child.stdin.end(); }
    }
    assert.match(error, /line "0": speaker "mira" is not in `views`/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
