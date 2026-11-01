// Every template under templates/ is staged with the runtime and evaluates
// through the real CLI. Per-template cases are added below as templates land.
// Build-integration tests: they read packages/cli/dist/templates/ and spawn the
// built CLI (harness.ts), so they need `pnpm run build:runtime` first.
import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';
import { entryPropsLayer, evaluate } from './harness.js';

const templatesDir = fileURLToPath(new URL('../', import.meta.url));

function templateNames() {
  return readdirSync(templatesDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && existsSync(join(templatesDir, entry.name, 'template.json')))
    .map((entry) => entry.name)
    .sort();
}

function pick<T extends object>(object: T | undefined, keys: (keyof T)[]) {
  return Object.fromEntries(keys.map((key) => [key, object?.[key]]));
}

test('staging copies every template and leaves out source/', () => {
  const staged = fileURLToPath(new URL('../../packages/cli/dist/templates/', import.meta.url));
  for (const name of templateNames()) {
    assert.ok(existsSync(join(staged, name, 'template.json')), name);
    assert.ok(!existsSync(join(staged, name, 'source')), `${name}/source`);
  }
});

// Stray OS and editor files never ship: staging a template that holds some
// leaves them out, and the staged tree holds none.
test('staging leaves out OS and editor files', () => {
  const stage = fileURLToPath(new URL('../../packages/cli/scripts/stage-templates.mjs', import.meta.url));
  const staged = fileURLToPath(new URL('../../packages/cli/dist/templates/', import.meta.url));
  const junk = ['.DS_Store', 'assets/.DS_Store', 'assets/._photo-01.png', 'Thumbs.db', 'desktop.ini', 'film.tsx~', '.film.tsx.swp'];
  const planted = junk.map((name) => join(templatesDir, 'slideshow', name));
  const isJunk = (name: string) => /^(\.DS_Store|Thumbs\.db|desktop\.ini|\._.*|.*~|\..*\.sw.)$/s.test(name);
  const stagedFiles = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory() ? stagedFiles(join(dir, entry.name)) : [join(dir, entry.name)]);
  try {
    for (const file of planted) writeFileSync(file, 'junk');
    execFileSync(process.execPath, [stage]);
    for (const name of junk) assert.ok(!existsSync(join(staged, 'slideshow', name)), name);
    assert.ok(existsSync(join(staged, 'slideshow/assets/photo-01.png')));
    assert.deepEqual(stagedFiles(staged).filter((file) => isJunk(file.split('/').pop() ?? '')), []);
  } finally {
    for (const file of planted) rmSync(file, { force: true });
    execFileSync(process.execPath, [stage]);
  }
});

test('blank wraps its layers in a Group', () => {
  const source = readFileSync(join(templatesDir, 'blank/film.tsx'), 'utf8');
  assert.match(source, /<Group>[\s\S]*<Rect[\s\S]*<Text[\s\S]*<\/Group>/);
});

test('blank evaluates without inputs', async () => {
  const result = await evaluate(join(templatesDir, 'blank/film.tsx'), []);
  assert.deepEqual(pick(result.config, ['width', 'height', 'fps', 'durationInFrames']),
    { width: 1920, height: 1080, fps: 30, durationInFrames: 150 });
});

// ROW_START + 3 rows * ROW_STAGGER + BAR_GROW + HOLD = 24 + 24 + 30 + 75 for
// the three sample rows in data.json.
const RANKING_SAMPLE_FRAMES = 153;

function rankingWithData(contents: string) {
  const dir = join(templatesDir, 'ranking');
  const data = join(mkdtempSync(join(tmpdir(), 'celesta-ranking-')), 'data.json');
  writeFileSync(data, contents);
  return evaluate(join(dir, 'film.tsx'), [entryPropsLayer(dir), { source: 'props', baseDir: dir, values: { data } }]);
}

for (const format of ['16:9', '9:16']) {
  test(`ranking evaluates in ${format}`, async () => {
    const dir = join(templatesDir, 'ranking');
    const result = await evaluate(join(dir, 'film.tsx'),
      [entryPropsLayer(dir), { source: 'props', baseDir: dir, values: { format } }]);
    assert.ok(result.config, JSON.stringify(result));
    assert.deepEqual([result.config.width, result.config.height], format === '16:9' ? [1920, 1080] : [1080, 1920]);
    assert.equal(result.config.fps, 30);
    assert.equal(result.config.durationInFrames, RANKING_SAMPLE_FRAMES);
  });
}

test('ranking variants validate', async () => {
  const dir = join(templatesDir, 'ranking');
  const variants = join(dir, 'variants');
  // variants/ holds props files (objects) and the data files they name
  // (arrays). A props file replaces props.json, so it is the only layer.
  const propsFiles = readdirSync(variants)
    .filter((name) => name.endsWith('.json'))
    .map((name) => ({ name, file: join(variants, name) }))
    .map((entry) => ({ ...entry, values: JSON.parse(readFileSync(entry.file, 'utf8')) }))
    .filter(({ values }) => values !== null && typeof values === 'object' && !Array.isArray(values));
  assert.ok(propsFiles.length >= 2, `variants: ${propsFiles.map(({ name }) => name)}`);
  for (const { name, file, values } of propsFiles) {
    const result = await evaluate(join(dir, 'film.tsx'), [{ source: 'propsFile', file, baseDir: variants, values }]);
    assert.equal(result.invalidProperties, undefined, `${name}: no invalid properties`);
    assert.ok(result.config, `${name}: ${JSON.stringify(result)}`);
  }
});

test('ranking rejects an empty data file', async () => {
  const result = await rankingWithData('[]');
  assert.match(String(result.error), /must be a non-empty array/);
});

test('ranking names the data file when it is not JSON', async () => {
  const result = await rankingWithData('[{ "name": ');
  assert.match(String(result.error), /data\.json: /);
});

test('ranking rejects a row without a name, naming the file and row', async () => {
  const result = await rankingWithData('[{ "name": "りんご", "score": 3 }, { "score": 2 }]');
  assert.match(String(result.error), /data\.json: row 1 must be an object with a string "name" and a number "score"/);
});

test('ranking rejects a null row', async () => {
  const result = await rankingWithData('[{ "name": "りんご", "score": 3 }, null]');
  assert.match(String(result.error), /data\.json: row 1 /);
});

// A TITLE_SECONDS card (90 frames) plus five 4-second slides (120 frames
// each), overlapped by the five TRANSITION_FRAMES (20) transitions between
// them, for the five sample slides in slides.json.
const SLIDESHOW_SAMPLE_FRAMES = 590;

function slideshow(values: Record<string, unknown>) {
  const dir = join(templatesDir, 'slideshow');
  return evaluate(join(dir, 'film.tsx'), [entryPropsLayer(dir), { source: 'props', baseDir: dir, values }]);
}

function slideshowWithSlides(contents: string) {
  const slides = join(mkdtempSync(join(tmpdir(), 'celesta-slideshow-')), 'slides.json');
  writeFileSync(slides, contents);
  return slideshow({ slides });
}

for (const format of ['16:9', '9:16']) {
  test(`slideshow evaluates in ${format}`, async () => {
    const result = await slideshow({ format });
    assert.ok(result.config, JSON.stringify(result));
    assert.deepEqual([result.config.width, result.config.height], format === '16:9' ? [1920, 1080] : [1080, 1920]);
    assert.equal(result.config.fps, 30);
    assert.equal(result.config.durationInFrames, SLIDESHOW_SAMPLE_FRAMES);
  });
}

test('slideshow rejects empty slides', async () => {
  const result = await slideshowWithSlides('[]');
  assert.match(String(result.error), /slides\.json: slides must be a non-empty array/);
});

test('slideshow rejects a slide without an image, naming the file and slide', async () => {
  const result = await slideshowWithSlides('[{ "image": "a.png" }, { "caption": "朝の港" }]');
  assert.match(String(result.error), /slides\.json: slide 1 has no image/);
});

test('slideshow without music', async () => {
  const result = await slideshow({ music: '' });
  assert.ok(result.config, JSON.stringify(result));
  assert.equal(result.config.durationInFrames, SLIDESHOW_SAMPLE_FRAMES);
});

// The six sample lines have no voices, so each lasts SECONDS_PER_CHAR (0.15)
// per character of its text, at least MIN_LINE_SECONDS (1.5), rounded up to
// whole frames: 12, 23, 26, 17, 23 and 16 characters give 54, 104, 117, 77,
// 104 and 72 frames (528). Around them: OPENING_SECONDS (30 frames) before
// the first line, a LINE_GAP_SECONDS gap (9 frames) after each of the first
// five, ENDING_SECONDS (30 frames) after the last, and SCENE_LEAD_IN_SECONDS
// (45 frames) before the first line of the second scene:
// 30 + 528 + 5 * 9 + 30 + 45.
const DIALOGUE_SAMPLE_FRAMES = 678;

function dialogue(values: Record<string, unknown>) {
  const dir = join(templatesDir, 'dialogue');
  return evaluate(join(dir, 'film.tsx'), [entryPropsLayer(dir), { source: 'props', baseDir: dir, values }]);
}

// A tiny valid WAV (a few silent samples) for lip sync to read; the harness
// reports any existing file as 2 seconds long.
function writeWav(file: string) {
  const samples = 16;
  const wav = Buffer.alloc(44 + samples * 2);
  wav.write('RIFF', 0);
  wav.writeUInt32LE(36 + samples * 2, 4);
  wav.write('WAVEfmt ', 8);
  wav.writeUInt32LE(16, 16);
  wav.writeUInt16LE(1, 20);
  wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(24000, 24);
  wav.writeUInt32LE(48000, 28);
  wav.writeUInt16LE(2, 32);
  wav.writeUInt16LE(16, 34);
  wav.write('data', 36);
  wav.writeUInt32LE(samples * 2, 40);
  writeFileSync(file, wav);
  return file;
}

/** A line of the dialogue template's script.json. */
interface DialogueLine {
  speaker: string;
  text: string;
  expression?: string;
  voice?: string;
}

// A one-character script whose portraits are the template's own, given as
// absolute paths so the script can live in a temporary folder. `lines` may
// name `voice.wav`, a WAV written beside the script; `setup(dir)` may replace
// it or add files there.
async function dialogueWithLines(lines: DialogueLine[], setup: (dir: string) => void = () => {}) {
  const assets = join(templatesDir, 'dialogue/assets');
  const mouths = Object.fromEntries(['closed', 'a', 'i', 'u', 'e', 'o'].map((shape) => [shape, join(assets, `mouth-${shape}.png`)]));
  const characters = {
    shizuku: {
      name: 'しずく',
      color: '#3f8fd6',
      portraits: { normal: join(assets, 'shizuku-normal.png'), smile: join(assets, 'shizuku-smile.png') },
      mouths,
    },
  };
  const dir = mkdtempSync(join(tmpdir(), 'celesta-dialogue-'));
  try {
    writeWav(join(dir, 'voice.wav'));
    setup(dir);
    const script = join(dir, 'script.json');
    writeFileSync(script, JSON.stringify({ characters, lines }));
    return await dialogue({ script });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

for (const format of ['16:9', '9:16']) {
  test(`dialogue evaluates in ${format}`, async () => {
    const result = await dialogue({ format });
    assert.ok(result.config, JSON.stringify(result));
    assert.deepEqual([result.config.width, result.config.height], format === '16:9' ? [1920, 1080] : [1080, 1920]);
    assert.equal(result.config.fps, 30);
    assert.equal(result.config.durationInFrames, DIALOGUE_SAMPLE_FRAMES);
  });
}

// One line of 6 characters, between OPENING_SECONDS (30 frames) and
// ENDING_SECONDS (30 frames): timed from its text it lasts MIN_LINE_SECONDS
// (45 frames), so 30 + 45 + 30; with a voice it lasts the voice (60 frames),
// so 30 + 60 + 30.
test('dialogue times a line from its voice when it has one', async () => {
  const line = { speaker: 'shizuku', text: 'こんにちは。' };
  const withoutVoice = await dialogueWithLines([line]);
  assert.equal(withoutVoice.config?.durationInFrames, 105, JSON.stringify(withoutVoice));
  const withVoice = await dialogueWithLines([{ ...line, voice: 'voice.wav' }]);
  assert.equal(withVoice.config?.durationInFrames, 120, JSON.stringify(withVoice));
});

test('dialogue rejects a missing voice, naming the file and line', async () => {
  const result = await dialogueWithLines([
    { speaker: 'shizuku', text: 'こんにちは。' },
    { speaker: 'shizuku', text: 'よろしくね。', voice: 'voices/missing.wav' },
  ]);
  assert.match(String(result.error), /script\.json: line 1: voice .*voices[\\/]missing\.wav does not exist/);
});

test('dialogue names the script, line, and voice when a recording cannot be read', async () => {
  // loadLipSync decodes the file in Node (only probeMedia is faked), and a
  // PNG named voice.wav is not a WAV.
  const result = await dialogueWithLines(
    [{ speaker: 'shizuku', text: 'こんにちは。', voice: 'voice.wav' }],
    (dir) => copyFileSync(join(templatesDir, 'dialogue/assets/shizuku-normal.png'), join(dir, 'voice.wav')),
  );
  assert.match(String(result.error), /script\.json: line 0: voice .*voice\.wav: \S/);
});

test('dialogue rejects an unknown speaker', async () => {
  const result = await dialogueWithLines([
    { speaker: 'shizuku', text: 'こんにちは。' },
    { speaker: 'mira', text: 'はじめまして。' },
  ]);
  assert.match(String(result.error), /script\.json: line 1: speaker "mira" is not in characters/);
});

test('dialogue rejects a missing expression', async () => {
  const result = await dialogueWithLines([{ speaker: 'shizuku', text: 'こんにちは。', expression: 'angry' }]);
  assert.match(String(result.error), /script\.json: line 0: shizuku has no "angry" portrait/);
});

test('dialogue with a background image and music', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-dialogue-'));
  try {
    const result = await dialogue({
      background: join(templatesDir, 'dialogue/assets/shizuku-normal.png'),
      music: writeWav(join(dir, 'music.wav')),
    });
    assert.ok(result.config, JSON.stringify(result));
    assert.equal(result.config.durationInFrames, DIALOGUE_SAMPLE_FRAMES);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
